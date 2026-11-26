//! The app's `/terminal/ws` link and the terminals' cells. The machine is `client::terminal`;
//! each attached terminal's output feeds a `troupe_term::Term` here, which the view locks to
//! draw.
//!
//! Native: the link and the parser run on one thread, so a flood of output never blocks the
//! window; the view only reads the cells. Web: the page has one thread, so `drain` parses a
//! bounded slice of the queued output per frame and wakes the page for the rest.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use client::terminal::{self, Command, Output};
use client::{CallId, Config};
use rune::Waker;
use serde_json::json;
use troupe_protocol::terminal::WebTerminalRequestType;
use troupe_term::Term;

pub type Cells = Arc<Mutex<Term>>;
type Shared = Arc<Mutex<BTreeMap<String, Cells>>>;

/// What the link reports to the app.
#[derive(Debug)]
pub enum TermEvent {
    /// A terminal's cells changed.
    Changed(String),
    /// Anything else the machine said (replies, lifecycle events, lost claims, status).
    Link(Output),
}

/// What React writes after a replay that lost output (`web-terminal-api.ts`).
const GAP_MARKER: &str = "\x1b[33m\r\n[output gap \u{2014} some history was evicted]\r\n\x1b[0m";

/// Calls the link makes on its own (writing the terminal's replies to the PTY) count down
/// from here, so their replies never meet an app call.
const OWN_CALLS: CallId = 1 << 62;

/// What the link does with a terminal's bytes and its own calls.
struct Feed {
    shared: Shared,
    next_own: CallId,
}

impl Feed {
    fn new(shared: Shared) -> Feed {
        Feed { shared, next_own: OWN_CALLS }
    }

    fn cells(&self, id: &str) -> Option<Cells> {
        self.shared.lock().ok()?.get(id).cloned()
    }

    /// Parses `bytes` into the terminal's cells in slices, so the view never waits long for the
    /// lock, and returns the write call carrying the terminal's answers, if it answered.
    fn absorb(&mut self, id: &str, reset: bool, bytes: &[u8]) -> Option<Command> {
        let cells = self.cells(id)?;
        let mut replies = Vec::new();
        if reset && let Ok(mut term) = cells.lock() {
            let (cols, rows) = (term.cols() as u16, term.rows() as u16);
            *term = Term::new(cols, rows);
        }
        for slice in bytes.chunks(16 * 1024) {
            let Ok(mut term) = cells.lock() else { return None };
            term.advance(slice);
            replies.extend(term.take_replies());
        }
        if replies.is_empty() {
            return None;
        }
        self.next_own += 1;
        let data = String::from_utf8_lossy(&replies).into_owned();
        Some(Command::Call {
            call: self.next_own,
            kind: WebTerminalRequestType::Write,
            payload: json!({ "terminalId": id, "data": data }),
            ledgered: true,
        })
    }

    /// Sorts one machine output: bytes go to the cells (and the answers back to the PTY), the
    /// rest to the app. Replies to the link's own calls are dropped.
    fn output(&mut self, output: Output, events: &mut Vec<TermEvent>, writes: &mut Vec<Command>) {
        let (id, reset, bytes) = match output {
            Output::Data { terminal_id, bytes, .. } => (terminal_id, false, bytes),
            Output::Replay { terminal_id, reset, mut bytes, gap, .. } => {
                if gap {
                    log::info!("[term] {terminal_id} replay lost output; the screen may be incomplete");
                    bytes.extend_from_slice(GAP_MARKER.as_bytes());
                }
                (terminal_id, reset, bytes)
            }
            Output::Reply { call, result } if call > OWN_CALLS => {
                if let Err(e) = result {
                    log::warn!("[term] writing a terminal's answer failed: {}", e.code);
                }
                return;
            }
            other => return events.push(TermEvent::Link(other)),
        };
        if let Some(write) = self.absorb(&id, reset, &bytes) {
            writes.push(write);
        }
        if !events.iter().any(|e| matches!(e, TermEvent::Changed(c) if *c == id)) {
            events.push(TermEvent::Changed(id));
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
    use std::thread;
    use std::time::Duration;

    use client::native::Driver;

    use super::*;

    const SLICE: Duration = Duration::from_millis(8);

    pub struct TermHost {
        commands: Sender<Command>,
        events: Receiver<TermEvent>,
        shared: Shared,
    }

    impl TermHost {
        pub fn connect(config: Config, wake: Waker) -> TermHost {
            let (commands, inbox) = mpsc::channel::<Command>();
            let (outbox, events) = mpsc::channel();
            let shared = Shared::default();
            let mut feed = Feed::new(shared.clone());
            thread::Builder::new()
                .name("troupe-terminal".into())
                .spawn(move || {
                    let mut driver = Driver::new(terminal::TerminalClient::new(config));
                    driver.start();
                    loop {
                        loop {
                            match inbox.try_recv() {
                                Ok(command) => driver.command(command),
                                Err(TryRecvError::Empty) => break,
                                Err(TryRecvError::Disconnected) => {
                                    driver.stop();
                                    return;
                                }
                            }
                        }
                        let (mut batch, mut writes) = (Vec::new(), Vec::new());
                        let mut wait = SLICE;
                        while let Some(output) = driver.next(wait) {
                            feed.output(output, &mut batch, &mut writes);
                            wait = Duration::ZERO;
                        }
                        for write in writes {
                            driver.command(write);
                        }
                        if batch.is_empty() {
                            continue;
                        }
                        for event in batch {
                            if outbox.send(event).is_err() {
                                return;
                            }
                        }
                        wake();
                    }
                })
                .expect("spawn the /terminal/ws thread");
            TermHost { commands, events, shared }
        }

        pub fn send(&mut self, command: Command) {
            if self.commands.send(command).is_err() {
                log::warn!("[term] the /terminal/ws thread is gone; command dropped");
            }
        }

        pub fn drain(&mut self, _now: f64) -> Vec<TermEvent> {
            self.events.try_iter().collect()
        }

        pub fn open(&mut self, id: &str, cols: u16, rows: u16) -> Cells {
            open(&self.shared, id, cols, rows)
        }

        pub fn forget(&mut self, id: &str) {
            if let Ok(mut map) = self.shared.lock() {
                map.remove(id);
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use std::collections::VecDeque;
    use std::rc::Rc;

    use client::Event;
    use client::web::WebDriver;

    use super::*;

    /// The bytes an output carries for the parser.
    fn output_len(output: &Output) -> usize {
        match output {
            Output::Data { bytes, .. } | Output::Replay { bytes, .. } => bytes.len(),
            _ => 0,
        }
    }

    /// Output bytes parsed per frame on the page's one thread.
    const BUDGET: usize = 256 * 1024;
    /// The most output bytes left waiting for later frames. A frame parses past its budget
    /// rather than let a flood grow the queue without bound.
    const MAX_BACKLOG: usize = 4 * 1024 * 1024;

    pub struct TermHost {
        driver: WebDriver<terminal::TerminalClient>,
        queued: Vec<Command>,
        started: bool,
        wake: Waker,
        shared: Shared,
        feed: Feed,
        /// Output waiting for a frame's parse budget, in arrival order.
        backlog: VecDeque<Output>,
        /// The output bytes in `backlog`.
        backlog_bytes: usize,
        armed: Option<f64>,
    }

    impl TermHost {
        pub fn connect(config: Config, wake: Waker) -> TermHost {
            let notify = wake.clone();
            let shared = Shared::default();
            TermHost {
                driver: WebDriver::new(terminal::TerminalClient::new(config), Rc::new(move || notify())),
                queued: Vec::new(),
                started: false,
                wake,
                feed: Feed::new(shared.clone()),
                shared,
                backlog: VecDeque::new(),
                backlog_bytes: 0,
                armed: None,
            }
        }

        pub fn send(&mut self, command: Command) {
            self.queued.push(command);
            (self.wake)();
        }

        pub fn drain(&mut self, now: f64) -> Vec<TermEvent> {
            let mut outputs = Vec::new();
            if !self.started {
                self.started = true;
                outputs.extend(self.driver.pump(Some(Event::Start), now));
            }
            for command in std::mem::take(&mut self.queued) {
                outputs.extend(self.driver.pump(Some(Event::Command(command)), now));
            }
            outputs.extend(self.driver.pump(None, now));
            self.backlog_bytes += outputs.iter().map(output_len).sum::<usize>();
            self.backlog.extend(outputs);
            let (mut events, mut writes, mut spent) = (Vec::new(), Vec::new(), 0);
            while spent < BUDGET || self.backlog_bytes > MAX_BACKLOG {
                let Some(output) = self.backlog.pop_front() else { break };
                let len = output_len(&output);
                spent += len;
                self.backlog_bytes -= len;
                self.feed.output(output, &mut events, &mut writes);
            }
            if spent > BUDGET {
                log::debug!("[term] parsed {spent} bytes this frame to keep the backlog under {MAX_BACKLOG}");
            }
            for write in writes {
                self.send(write);
            }
            if !self.backlog.is_empty() {
                (self.wake)();
            }
            self.arm(now);
            events
        }

        pub fn open(&mut self, id: &str, cols: u16, rows: u16) -> Cells {
            open(&self.shared, id, cols, rows)
        }

        pub fn forget(&mut self, id: &str) {
            if let Ok(mut map) = self.shared.lock() {
                map.remove(id);
            }
        }

        fn arm(&mut self, now: f64) {
            use wasm_bindgen::JsCast;
            use wasm_bindgen::closure::Closure;
            let Some(due) = self.driver.next_wake() else { return };
            if self.armed.is_some_and(|armed| armed <= due && armed > now) {
                return;
            }
            self.armed = Some(due);
            let wake = self.wake.clone();
            let ms = ((due - now).max(0.0) * 1000.0).ceil() as i32 + 1;
            let fire = Closure::once_into_js(move || wake());
            if let Some(window) = web_sys::window()
                && let Err(e) = window.set_timeout_with_callback_and_timeout_and_arguments_0(fire.unchecked_ref(), ms)
            {
                log::warn!("[term] setTimeout failed: {e:?}");
            }
        }
    }
}

/// The cells of terminal `id`, made at `cols` x `rows` the first time.
fn open(shared: &Shared, id: &str, cols: u16, rows: u16) -> Cells {
    let mut map = shared.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    map.entry(id.to_owned()).or_insert_with(|| Arc::new(Mutex::new(Term::new(cols, rows)))).clone()
}

pub use imp::TermHost;
