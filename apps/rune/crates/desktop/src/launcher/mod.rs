//! The launcher: the app owns a local `troupe-server`, as the Tauri app owns its in-process
//! host, so a user sees their projects with no server to start and no token to paste.
//!
//! - **Attach.** `TROUPE_SERVER_URL` (and `TROUPE_SERVER_TOKEN`) names a server to use as is;
//!   the app never starts or stops it.
//! - **Reuse.** `server.json` names a server for this data dir that answers `/health`: the app
//!   uses it and leaves it running at quit. If it dies, the app starts its own on its port.
//! - **Spawn.** Otherwise the app refuses when another host holds the data dir
//!   (`data_lock`), then starts `troupe-server` on loopback with a fresh token in
//!   `TROUPE_WEB_AUTH_TOKEN` and waits for `/health`.
//!
//! The runner thread supervises what it started or reused (`supervisor`) and stops a server
//! it started when the app quits. `TROUPE_PREVIEW_KEEP_SERVER=1` starts one that outlives the
//! app instead.

pub mod process;
pub mod record;
pub mod supervisor;

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::data_lock::{self, Holder};
use crate::paths::Paths;
use process::Server;
use record::ServerRecord;
use supervisor::{Effect, Event, Notice, Supervisor};

/// How often the runner looks at the server and its timers.
const TICK: Duration = Duration::from_millis(200);
/// How often a reused server, which is not our child, is probed.
const PROBE_EVERY: Duration = Duration::from_secs(1);

#[derive(Debug)]
pub enum LaunchError {
    /// Another Troupe host holds the data dir.
    Locked(Holder),
    Failed(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Attached,
    Reused,
    Spawned,
}

pub struct Started {
    pub config: client::Config,
    pub mode: Mode,
    /// The runner, for a server the app started or reused.
    pub control: Option<Control>,
}

type Sink = Arc<Mutex<Option<Box<dyn Fn(Notice) + Send>>>>;

enum Msg {
    Retry,
    Stop,
}

/// The app's hold on the runner thread.
pub struct Control {
    tx: Sender<Msg>,
    sink: Sink,
    thread: Option<JoinHandle<()>>,
}

/// The running runner's inbox, for `retry` from a hook that holds no state.
static RUNNER: OnceLock<Mutex<Sender<Msg>>> = OnceLock::new();

/// Starts the server again after the supervisor gave up; a no-op otherwise. A second launch
/// of the app calls it.
pub fn retry() {
    if let Some(tx) = RUNNER.get() {
        let _ = tx.lock().unwrap_or_else(|e| e.into_inner()).send(Msg::Retry);
    }
}

impl Control {
    /// Where the runner reports restarts; set once the window exists.
    pub fn on_notice(&self, f: impl Fn(Notice) + Send + 'static) {
        *self.sink.lock().unwrap_or_else(|e| e.into_inner()) = Some(Box::new(f));
    }

    /// Stops a server the app started (up to 5 s) and ends the runner.
    pub fn stop(mut self) {
        let _ = self.tx.send(Msg::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

pub fn start(paths: &Paths) -> Result<Started, LaunchError> {
    if let Some(url) = env("TROUPE_SERVER_URL") {
        log::info!("[launcher] attaching to {url}; it is not managed");
        let base = url.trim_end_matches('/').to_owned();
        return Ok(Started { config: config(paths, base, env("TROUPE_SERVER_TOKEN")), mode: Mode::Attached, control: None });
    }
    let keep_alive = env("TROUPE_PREVIEW_KEEP_SERVER").is_some_and(|v| v == "1" || v == "true");
    if let Some((record, token)) = reusable(paths) {
        log::info!("[launcher] reusing server pid {} on 127.0.0.1:{}", record.pid, record.port);
        let runner = Runner::new(paths, record.binary.clone(), record.port, token.clone(), keep_alive, Proc::adopted());
        let config = config(paths, base_url(record.port), Some(token));
        return Ok(Started { config, mode: Mode::Reused, control: Some(runner.run()) });
    }
    match data_lock::holder(&paths.data_dir) {
        Ok(Some(holder)) => return Err(LaunchError::Locked(holder)),
        Ok(None) => {}
        Err(e) => log::warn!("[launcher] could not probe the data lock: {e}"),
    }
    let bin = process::server_binary()
        .ok_or_else(|| LaunchError::Failed("No troupe-server found beside the app or on PATH. Set TROUPE_SERVER_BIN.".into()))?;
    let token = process::new_token().map_err(LaunchError::Failed)?;
    let pinned = env("TROUPE_PREVIEW_SERVER_PORT").and_then(|p| p.parse().ok());
    let port = match pinned {
        Some(port) => port,
        None => process::pick_port(record::read_port(&paths.last_port_file()))
            .map_err(|e| LaunchError::Failed(format!("no free loopback port: {e}")))?,
    };
    record::write_private(&paths.token_file(), token.as_bytes())
        .map_err(|e| LaunchError::Failed(format!("cannot save the server token: {e}")))?;
    let mut server = process::spawn(&bin, paths, port, &token, keep_alive).map_err(LaunchError::Failed)?;
    match process::wait_ready(&mut server, port) {
        Ok(took) => log::info!("[launcher] server ready on 127.0.0.1:{port} in {} ms", took.as_millis()),
        Err(e) => {
            log::error!("[launcher] first start failed: {e}");
            return Err(LaunchError::Failed(e));
        }
    }
    let _ = record::write_private(&paths.last_port_file(), port.to_string().as_bytes());
    let mut runner = Runner::new(paths, bin, port, token.clone(), keep_alive, Proc::None);
    runner.adopt(server);
    let config = config(paths, base_url(port), Some(token));
    Ok(Started { config, mode: Mode::Spawned, control: Some(runner.run()) })
}

/// A recorded server for this data dir that answers, with its token.
fn reusable(paths: &Paths) -> Option<(ServerRecord, String)> {
    let record = record::read_record(&paths.record_file())?;
    if record.data_dir != paths.data_dir {
        return None;
    }
    let token = record::read_token(&paths.token_file())?;
    process::healthy(record.port).then_some((record, token))
}

fn base_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
}

fn config(paths: &Paths, base_url: String, token: Option<String>) -> client::Config {
    let pid = std::process::id();
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.subsec_nanos());
    client::Config {
        base_url,
        token,
        device_id: env("TROUPE_DEVICE_ID").unwrap_or_else(|| record::device_id(&paths.device_id_file())),
        client_instance_id: format!("troupe-preview-{pid}-{nanos:x}"),
        seed: (u64::from(pid) << 32) | u64::from(nanos),
    }
}

/// What the runner watches.
enum Proc {
    None,
    Owned(Server),
    /// A server another launch started: probed over `/health`, never stopped.
    Adopted { misses: u32, probed: Instant },
}

impl Proc {
    fn adopted() -> Proc {
        Proc::Adopted { misses: 0, probed: Instant::now() }
    }
}

struct Runner {
    paths: Paths,
    bin: PathBuf,
    port: u16,
    token: String,
    keep_alive: bool,
    proc: Proc,
    supervisor: Supervisor,
    sink: Sink,
}

impl Runner {
    fn new(paths: &Paths, bin: PathBuf, port: u16, token: String, keep_alive: bool, proc: Proc) -> Runner {
        Runner {
            paths: paths.clone(),
            bin,
            port,
            token,
            keep_alive,
            proc,
            supervisor: Supervisor::running(Instant::now()),
            sink: Arc::default(),
        }
    }

    fn adopt(&mut self, server: Server) {
        let record = ServerRecord {
            pid: server.pid(),
            port: self.port,
            binary: self.bin.clone(),
            data_dir: self.paths.data_dir.clone(),
            keep_alive: self.keep_alive,
        };
        if let Err(e) = record::write_record(&self.paths.record_file(), &record) {
            log::warn!("[launcher] cannot write {}: {e}", self.paths.record_file().display());
        }
        self.proc = Proc::Owned(server);
    }

    fn run(self) -> Control {
        let (tx, rx) = mpsc::channel();
        let _ = RUNNER.set(Mutex::new(tx.clone()));
        let sink = self.sink.clone();
        let thread = std::thread::Builder::new()
            .name("troupe-launcher".into())
            .spawn(move || self.watch(rx))
            .expect("spawn the launcher thread");
        Control { tx, sink, thread: Some(thread) }
    }

    fn watch(mut self, rx: Receiver<Msg>) {
        loop {
            match rx.recv_timeout(TICK) {
                Ok(Msg::Stop) | Err(RecvTimeoutError::Disconnected) => return self.stop(),
                Ok(Msg::Retry) => {
                    log::info!("[launcher] retry asked");
                    self.step(Event::Retry);
                }
                Err(RecvTimeoutError::Timeout) => {}
            }
            let now = Instant::now();
            if let Some(status) = self.exited(now) {
                log::warn!("[launcher] server stopped ({status})");
                self.proc = Proc::None;
                self.step(Event::Exited { at: now, status });
            }
            self.step(Event::Tick { at: now });
        }
    }

    fn exited(&mut self, now: Instant) -> Option<String> {
        match &mut self.proc {
            Proc::None => None,
            Proc::Owned(server) => server.exited(),
            Proc::Adopted { misses, probed } => {
                if now.duration_since(*probed) < PROBE_EVERY {
                    return None;
                }
                *probed = now;
                *misses = if process::healthy(self.port) { 0 } else { *misses + 1 };
                (*misses >= 2).then(|| "the reused server stopped answering".to_owned())
            }
        }
    }

    fn step(&mut self, event: Event) {
        match self.supervisor.step(event) {
            Some(Effect::Spawn) => self.respawn(),
            Some(Effect::Notify(notice)) => self.notify(notice),
            None => {}
        }
    }

    fn respawn(&mut self) {
        let now = Instant::now();
        match process::spawn(&self.bin, &self.paths, self.port, &self.token, self.keep_alive) {
            Ok(server) => {
                self.adopt(server);
                self.step(Event::Started { at: now });
            }
            Err(e) => {
                log::error!("[launcher] restart failed: {e}");
                self.step(Event::Exited { at: now, status: e });
            }
        }
    }

    fn notify(&self, notice: Notice) {
        match &notice {
            Notice::Restarting { delay, .. } => log::info!("[launcher] restarting in {} ms", delay.as_millis()),
            Notice::Recovered => log::info!("[launcher] server restarted on 127.0.0.1:{}", self.port),
            Notice::GaveUp { exits, .. } => log::error!("[launcher] gave up after {exits} fast exits"),
        }
        if let Some(sink) = self.sink.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            sink(notice);
        }
    }

    fn stop(mut self) {
        self.supervisor.step(Event::Stop);
        match std::mem::replace(&mut self.proc, Proc::None) {
            Proc::Owned(mut server) if !self.keep_alive => {
                log::info!("[launcher] stopping server pid {}", server.pid());
                server.stop();
                let _ = std::fs::remove_file(self.paths.record_file());
                log::info!("[launcher] server stopped");
            }
            Proc::Owned(server) => log::info!("[launcher] leaving keep-alive server pid {} running", server.pid()),
            Proc::Adopted { .. } => log::info!("[launcher] leaving the reused server running"),
            Proc::None => {}
        }
    }
}

#[cfg(test)]
mod tests;
