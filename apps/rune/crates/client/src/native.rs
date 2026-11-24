//! The desktop backend: one thread per socket (blocking `tungstenite`), a
//! channel back to the [`Driver`] that steps the machine, and blocking HTTP
//! through `ureq`. `https://` and `wss://` hosts use rustls with the OS
//! verifier, so a tunnel's public certificate and a CA the user installed both
//! pass; plain `http://` (loopback) skips TLS.

use std::collections::{HashMap, VecDeque};
use std::io::ErrorKind;
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use rune_core::machine::Attempt;
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, StreamOwned};
use rustls_platform_verifier::BuilderVerifierExt;
use serde_json::Value;
use tungstenite::client::IntoClientRequest;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

use crate::http::{COMMAND_ID_HEADER, HttpError, HttpRequest, Method, parse_body};
use crate::machine::{Config, Effect, Event, Machine, SocketEvent};

/// How long a socket thread blocks on a read before it checks for frames to
/// write. It bounds the delay of an outgoing frame.
const READ_SLICE: Duration = Duration::from_millis(5);
/// The budget of the TCP connect and of each read or write of the upgrade, as
/// in [`crate::link::HANDSHAKE_S`].
const HANDSHAKE: Duration = Duration::from_secs(10);
/// How long one write may block on a peer that stopped reading.
const WRITE_BUDGET: Duration = Duration::from_secs(10);

enum Outgoing {
    Text(String),
    Close,
}

/// Runs one machine: carries its effects out and feeds it socket events and
/// timer ticks. Single-threaded; the socket threads only talk to it through a
/// channel.
pub struct Driver<M: Machine> {
    machine: M,
    events: Receiver<SocketEvent>,
    events_tx: Sender<SocketEvent>,
    sockets: HashMap<Attempt, Sender<Outgoing>>,
    wakes: Vec<f64>,
    outputs: VecDeque<M::Output>,
    epoch: Instant,
}

impl<M: Machine> Driver<M> {
    pub fn new(machine: M) -> Self {
        let (events_tx, events) = mpsc::channel();
        Self {
            machine,
            events,
            events_tx,
            sockets: HashMap::new(),
            wakes: Vec::new(),
            outputs: VecDeque::new(),
            epoch: Instant::now(),
        }
    }

    #[must_use]
    pub fn machine(&self) -> &M {
        &self.machine
    }

    /// Seconds since the driver started.
    #[must_use]
    pub fn now(&self) -> f64 {
        self.epoch.elapsed().as_secs_f64()
    }

    pub fn start(&mut self) {
        self.apply(Event::Start);
    }

    pub fn stop(&mut self) {
        self.apply(Event::Stop);
    }

    pub fn command(&mut self, command: M::Command) {
        self.apply(Event::Command(command));
    }

    /// The next output, waiting at most `timeout` for one.
    pub fn next(&mut self, timeout: Duration) -> Option<M::Output> {
        let deadline = self.now() + timeout.as_secs_f64();
        loop {
            if let Some(output) = self.outputs.pop_front() {
                return Some(output);
            }
            let now = self.now();
            if self.wakes.iter().any(|t| *t <= now) {
                self.wakes.retain(|t| *t > now);
                self.apply(Event::Tick);
                continue;
            }
            if now >= deadline {
                // A zero or spent timeout still takes an event that is ready.
                match self.events.try_recv() {
                    Ok(event) => {
                        self.apply(Event::Socket(event));
                        continue;
                    }
                    Err(_) => return None,
                }
            }
            let until = self.wakes.iter().copied().fold(deadline, f64::min);
            match self
                .events
                .recv_timeout(Duration::from_secs_f64((until - now).max(0.0)))
            {
                Ok(event) => self.apply(Event::Socket(event)),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return None,
            }
        }
    }

    fn apply(&mut self, event: Event<M::Command>) {
        let now = self.now();
        for effect in self.machine.step(event, now) {
            match effect {
                Effect::Open { socket, url } => {
                    let (tx, rx) = mpsc::channel();
                    self.sockets.insert(socket, tx);
                    let events = self.events_tx.clone();
                    thread::spawn(move || run_socket(socket, &url, &events, &rx));
                }
                Effect::Send { socket, text } => {
                    if let Some(tx) = self.sockets.get(&socket) {
                        let _ = tx.send(Outgoing::Text(text));
                    }
                }
                Effect::Close { socket } => {
                    if let Some(tx) = self.sockets.remove(&socket) {
                        let _ = tx.send(Outgoing::Close);
                    }
                }
                Effect::WakeAt(due) => self.wakes.push(due),
                Effect::Output(output) => self.outputs.push_back(output),
            }
        }
    }
}

impl<M: Machine> Drop for Driver<M> {
    fn drop(&mut self) {
        for (_, tx) in self.sockets.drain() {
            let _ = tx.send(Outgoing::Close);
        }
    }
}

fn run_socket(
    socket: Attempt,
    url: &str,
    events: &Sender<SocketEvent>,
    outgoing: &Receiver<Outgoing>,
) {
    let mut ws = match connect(url, HANDSHAKE) {
        Ok(ws) => ws,
        Err(e) => {
            log::info!("[client] connect {url} failed: {e}");
            let _ = events.send(SocketEvent::Closed { socket, code: None });
            return;
        }
    };
    let _ = events.send(SocketEvent::Opened(socket));
    let code = pump(socket, &mut ws, events, outgoing);
    let _ = events.send(SocketEvent::Closed { socket, code });
}

/// Open `ws://` or `wss://` within `budget` per step, then make reads return
/// every [`READ_SLICE`] so the pump can write between them.
fn connect(url: &str, budget: Duration) -> Result<WebSocket<MaybeTlsStream<TcpStream>>, String> {
    let request = url.into_client_request().map_err(|e| e.to_string())?;
    let secure = request.uri().scheme_str() == Some("wss");
    let host = request.uri().host().ok_or("no host in url")?;
    // An IPv6 literal keeps its brackets in the URI and the `Host` header,
    // but neither the TCP connect nor the TLS server name takes them.
    let host = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_owned();
    let port = request
        .uri()
        .port_u16()
        .unwrap_or(if secure { 443 } else { 80 });
    let tcp = connect_tcp(&host, port, budget)?;
    tcp.set_read_timeout(Some(budget))
        .and_then(|()| tcp.set_write_timeout(Some(budget)))
        .map_err(|e| e.to_string())?;
    let stream = if secure {
        let name = ServerName::try_from(host).map_err(|e| e.to_string())?;
        let tls = ClientConnection::new(tls_config()?, name).map_err(|e| e.to_string())?;
        MaybeTlsStream::Rustls(StreamOwned::new(tls, tcp))
    } else {
        MaybeTlsStream::Plain(tcp)
    };
    let (ws, _) = tungstenite::client(request, stream).map_err(|e| e.to_string())?;
    let tcp = match ws.get_ref() {
        MaybeTlsStream::Plain(tcp) => tcp,
        MaybeTlsStream::Rustls(tls) => tls.get_ref(),
        _ => return Err("unexpected socket stream".into()),
    };
    tcp.set_read_timeout(Some(READ_SLICE))
        .and_then(|()| tcp.set_write_timeout(Some(WRITE_BUDGET)))
        .map_err(|e| e.to_string())?;
    Ok(ws)
}

fn connect_tcp(host: &str, port: u16, budget: Duration) -> Result<TcpStream, String> {
    let mut last = format!("{host} resolved to no address");
    for addr in (host, port).to_socket_addrs().map_err(|e| e.to_string())? {
        match TcpStream::connect_timeout(&addr, budget) {
            Ok(tcp) => return Ok(tcp),
            Err(e) => last = e.to_string(),
        }
    }
    Err(last)
}

/// One rustls config for every `wss://` socket, on ring with the OS verifier.
fn tls_config() -> Result<Arc<ClientConfig>, String> {
    static CONFIG: OnceLock<Result<Arc<ClientConfig>, String>> = OnceLock::new();
    CONFIG
        .get_or_init(|| {
            let provider = Arc::new(rustls::crypto::ring::default_provider());
            ClientConfig::builder_with_provider(provider)
                .with_safe_default_protocol_versions()
                .and_then(BuilderVerifierExt::with_platform_verifier)
                .map(|b| Arc::new(b.with_no_client_auth()))
                .map_err(|e| format!("tls setup failed: {e}"))
        })
        .clone()
}

/// Moves frames both ways until the socket ends; returns its close code.
fn pump(
    socket: Attempt,
    ws: &mut WebSocket<MaybeTlsStream<TcpStream>>,
    events: &Sender<SocketEvent>,
    outgoing: &Receiver<Outgoing>,
) -> Option<u16> {
    loop {
        loop {
            match outgoing.try_recv() {
                Ok(Outgoing::Text(text)) => {
                    if let Err(e) = ws.send(Message::text(text)) {
                        log::info!("[client] send failed: {e}");
                        return None;
                    }
                }
                Ok(Outgoing::Close) | Err(TryRecvError::Disconnected) => {
                    let _ = ws.close(None);
                    let _ = ws.flush();
                    return None;
                }
                Err(TryRecvError::Empty) => break,
            }
        }
        match ws.read() {
            Ok(Message::Text(text)) => {
                if events
                    .send(SocketEvent::Text(socket, text.to_string()))
                    .is_err()
                {
                    return None;
                }
            }
            Ok(Message::Close(frame)) => {
                // Sends the close reply tungstenite queued.
                let _ = ws.flush();
                return frame.map(|f| u16::from(f.code));
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(e) => {
                log::info!("[client] socket ended: {e}");
                return None;
            }
        }
    }
}

/// Send one HTTP request and read the Troupe body.
pub fn http_send(config: &Config, request: &HttpRequest) -> Result<Value, HttpError> {
    let tls = ureq::tls::TlsConfig::builder()
        .provider(ureq::tls::TlsProvider::Rustls)
        .root_certs(ureq::tls::RootCerts::PlatformVerifier)
        .build();
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .tls_config(tls)
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(30)))
        .build()
        .into();
    let url = format!("{}{}", config.base_url.trim_end_matches('/'), request.path);
    let bearer = config.token.as_ref().map(|t| format!("Bearer {t}"));
    let network = |e: ureq::Error| HttpError::Network(e.to_string());
    let mut response = match request.method {
        Method::Get | Method::Delete => {
            let mut builder = if request.method == Method::Get {
                agent.get(&url)
            } else {
                agent.delete(&url)
            };
            if let Some(bearer) = &bearer {
                builder = builder.header("Authorization", bearer);
            }
            if let Some(id) = &request.command_id {
                builder = builder.header(COMMAND_ID_HEADER, id);
            }
            builder
                .header("Accept", "application/json")
                .call()
                .map_err(network)?
        }
        Method::Post | Method::Put => {
            let mut builder = if request.method == Method::Post {
                agent.post(&url)
            } else {
                agent.put(&url)
            };
            if let Some(bearer) = &bearer {
                builder = builder.header("Authorization", bearer);
            }
            if let Some(id) = &request.command_id {
                builder = builder.header(COMMAND_ID_HEADER, id);
            }
            let body = request
                .body
                .as_ref()
                .map_or_else(String::new, Value::to_string);
            builder
                .content_type("application/json")
                .send(body.as_bytes())
                .map_err(network)?
        }
    };
    let status = response.status().as_u16();
    let bytes = response.body_mut().read_to_vec().map_err(network)?;
    parse_body(status, &bytes)
}

#[cfg(test)]
#[path = "native_tests.rs"]
mod native_tests;
