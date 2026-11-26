//! The socket server and the compositor decorator. Each connection gets a thread. `tree`, `find`
//! and `wait` read the snapshot the compositor stores each frame while a client is connected;
//! every other command goes to the compositor, which runs it on the window's `Runtime` at the
//! start of the next frame.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use rune::compose::{Compositor, Frame};
use rune::render::{Offscreen, write_png};
use rune::{App, AppHandle};
use serde_json::json;

use crate::protocol::{self, Command, Element, Reply, Target};

/// How long a command sent to the window waits for the frame that runs it.
const FRAME_TIMEOUT: Duration = Duration::from_secs(10);
/// How long a new connection waits for a first snapshot before it serves anyway.
const FRESH_TIMEOUT: Duration = Duration::from_secs(5);
/// Frames kept running after an injection so the runtime handles the queued input.
const SETTLE_FRAMES: u32 = 3;

/// The default frame, for apps without a compositor of their own.
pub struct UiOnly;

impl<S: 'static> Compositor<S> for UiOnly {
    fn render(&mut self, f: &mut Frame<'_, S>) {
        f.draw_ui();
    }
}

#[derive(Default)]
struct Snapshot {
    seq: u64,
    elements: Vec<Element>,
}

#[derive(Default)]
struct Shared {
    snapshot: Mutex<Snapshot>,
    fresh: Condvar,
    clients: AtomicUsize,
}

struct Job {
    cmd: Command,
    reply: Sender<Reply>,
}

struct PendingSnap {
    path: PathBuf,
    reply: Sender<Reply>,
}

/// A snap drawn this frame, read back the next (its copy is submitted with this frame).
struct InFlightSnap {
    off: Offscreen,
    bgra: bool,
    snap: PendingSnap,
}

struct Driver<C> {
    inner: C,
    jobs: Receiver<Job>,
    shared: Arc<Shared>,
    pending_snaps: VecDeque<PendingSnap>,
    in_flight: Option<InFlightSnap>,
    settle: u32,
}

pub fn install<S: 'static, C: Compositor<S>>(app: App<S>, path: PathBuf, inner: C) -> App<S> {
    let _ = std::fs::remove_file(&path);
    let listener = match UnixListener::bind(&path) {
        Ok(listener) => listener,
        Err(e) => {
            log::error!("rune-drive: bind {} failed: {e}", path.display());
            return app.compositor(inner);
        }
    };
    log::info!("rune-drive: listening on {}", path.display());
    let shared = Arc::new(Shared::default());
    let (jobs_tx, jobs) = channel();
    let handle = app.handle();
    let server_shared = shared.clone();
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            match conn {
                Ok(conn) => {
                    let (shared, jobs_tx, handle) =
                        (server_shared.clone(), jobs_tx.clone(), handle.clone());
                    std::thread::spawn(move || serve(conn, &shared, &jobs_tx, &handle));
                }
                Err(e) => log::warn!("rune-drive: accept failed: {e}"),
            }
        }
    });
    app.compositor(Driver {
        inner,
        jobs,
        shared,
        pending_snaps: VecDeque::new(),
        in_flight: None,
        settle: 0,
    })
}

fn serve<S: 'static>(conn: UnixStream, shared: &Shared, jobs: &Sender<Job>, handle: &AppHandle<S>) {
    let Ok(mut out) = conn.try_clone() else {
        return;
    };
    shared.clients.fetch_add(1, Ordering::SeqCst);
    log::info!("rune-drive: client connected");
    await_fresh_snapshot(shared, handle);
    for line in BufReader::new(conn).lines() {
        let Ok(line) = line else { break };
        let t0 = Instant::now();
        let reply = match serde_json::from_str::<Command>(&line) {
            Ok(cmd) => dispatch(cmd, shared, jobs, handle),
            Err(e) => {
                log::warn!("rune-drive: bad command {line:?}: {e}");
                Err(format!("bad command: {e}"))
            }
        };
        let mut reply = protocol::reply_json(reply);
        reply["us"] = json!(t0.elapsed().as_micros() as u64);
        if writeln!(out, "{reply}").is_err() {
            break;
        }
    }
    shared.clients.fetch_sub(1, Ordering::SeqCst);
    log::info!("rune-drive: client disconnected");
}

/// Snapshots are taken only while a client is connected, so the first one waits for a frame.
fn await_fresh_snapshot<S: 'static>(shared: &Shared, handle: &AppHandle<S>) {
    let snapshot = shared.snapshot.lock().unwrap();
    let seq = snapshot.seq;
    handle.update_if(|_, _| true);
    let _ = shared
        .fresh
        .wait_timeout_while(snapshot, FRESH_TIMEOUT, |s| s.seq == seq);
}

fn dispatch<S: 'static>(
    cmd: Command,
    shared: &Shared,
    jobs: &Sender<Job>,
    handle: &AppHandle<S>,
) -> Reply {
    match cmd {
        Command::Tree => {
            let s = shared.snapshot.lock().unwrap();
            Ok(json!({ "seq": s.seq, "elements": s.elements }))
        }
        Command::Find(target) => {
            let s = shared.snapshot.lock().unwrap();
            protocol::find(&s.elements, &target)
        }
        Command::Wait { target, timeout_ms } => {
            wait(shared, &target, Duration::from_millis(timeout_ms))
        }
        cmd => {
            let (reply, rx) = channel();
            jobs.send(Job { cmd, reply })
                .map_err(|_| "app gone".to_string())?;
            handle.update_if(|_, _| true);
            rx.recv_timeout(FRAME_TIMEOUT)
                .unwrap_or_else(|_| Err("no frame within 10 s".into()))
        }
    }
}

fn wait(shared: &Shared, target: &Target, timeout: Duration) -> Reply {
    let snapshot = shared.snapshot.lock().unwrap();
    let (s, _) = shared
        .fresh
        .wait_timeout_while(snapshot, timeout, |s| {
            !s.elements.iter().any(|e| target.matches(e))
        })
        .unwrap();
    protocol::find(&s.elements, target).map_err(|_| "timeout".into())
}

impl<C> Driver<C> {
    fn finish_snap<S: 'static>(&mut self, f: &Frame<'_, S>) {
        let Some(InFlightSnap { off, bgra, snap }) = self.in_flight.take() else {
            return;
        };
        let mut px = off.read(f.device);
        if bgra {
            px.as_chunks_mut::<4>()
                .0
                .iter_mut()
                .for_each(|p| p.swap(0, 2));
        }
        let reply = write_png(&snap.path, off.width, off.height, &px)
            .map(|()| json!({ "path": snap.path, "size": [off.width, off.height] }))
            .map_err(|e| format!("write {}: {e}", snap.path.display()));
        if let Err(e) = &reply {
            log::warn!("rune-drive: snap failed: {e}");
        }
        let _ = snap.reply.send(reply);
    }

    fn store_snapshot<S: 'static>(&self, rt: &rune::Runtime<S>) {
        if self.shared.clients.load(Ordering::SeqCst) == 0 {
            return;
        }
        let elements = protocol::elements(rt);
        let mut s = self.shared.snapshot.lock().unwrap();
        s.seq += 1;
        s.elements = elements;
        drop(s);
        self.shared.fresh.notify_all();
    }
}

/// Draws the frame again through `inner` into an offscreen copy of the target.
fn draw_offscreen<S: 'static, C: Compositor<S>>(
    inner: &mut C,
    f: &mut Frame<'_, S>,
) -> (Offscreen, bool) {
    let (w, h) = f.size;
    let off = Offscreen::with_format(f.device, w, h, f.format);
    inner.render(&mut Frame {
        device: f.device,
        queue: f.queue,
        encoder: &mut *f.encoder,
        target: &off.view,
        format: f.format,
        size: f.size,
        dt: f.dt,
        update_ms: f.update_ms,
        rt: &mut *f.rt,
        ui: &mut *f.ui,
    });
    off.copy(f.encoder);
    let bgra = matches!(
        f.format,
        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
    );
    (off, bgra)
}

impl<S: 'static, C: Compositor<S>> Compositor<S> for Driver<C> {
    fn features(&self) -> wgpu::Features {
        self.inner.features()
    }

    fn render(&mut self, f: &mut Frame<'_, S>) {
        self.finish_snap(f);
        while let Ok(Job { cmd, reply }) = self.jobs.try_recv() {
            match cmd {
                Command::Snap { path } => self.pending_snaps.push_back(PendingSnap { path, reply }),
                cmd => {
                    let _ = reply.send(protocol::act(f.rt, &cmd));
                    self.settle = SETTLE_FRAMES;
                }
            }
        }
        self.inner.render(f);
        if self.in_flight.is_none()
            && let Some(snap) = self.pending_snaps.pop_front()
        {
            let (off, bgra) = draw_offscreen(&mut self.inner, f);
            self.in_flight = Some(InFlightSnap { off, bgra, snap });
        }
        self.store_snapshot(f.rt);
        self.settle = self.settle.saturating_sub(1);
    }

    fn animating(&self) -> bool {
        self.settle > 0
            || self.in_flight.is_some()
            || !self.pending_snaps.is_empty()
            || self.inner.animating()
    }

    fn summary(&mut self, device: &wgpu::Device) -> Option<String> {
        self.inner.summary(device)
    }
}

#[cfg(test)]
#[path = "driver_tests.rs"]
mod tests;
