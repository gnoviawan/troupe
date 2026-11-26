//! The app's `/ws` link to the Troupe server, one backend per target. Both take commands from
//! the view's handlers and hand outputs back to `poll`; the machine itself is `client::ws`.
//!
//! Native: the blocking `client::native::Driver` runs on its own thread and wakes the window
//! when an output is queued. Web: the page's `client::web::WebDriver` is pumped from `poll`,
//! and a `setTimeout` wakes the page for the machine's next timer.

use client::http::{HttpError, HttpRequest};
use client::{Config, ws};
use rune::Waker;
use serde_json::Value;

/// What an HTTP read is for.
#[derive(Clone, Debug, PartialEq)]
pub enum Fetch {
    /// `/skills` for a project folder.
    Skills(String),
    /// `/fs/ls` of a folder the mention menu's Explorer shows, under a project root.
    Listing { dir: String, root: String },
    /// `/fs/read` of a file an editor tab opened.
    FileRead(String),
    /// `/fs/write` of an editor's buffer.
    FileWrite { path: String, content: String },
    /// `/fs/read` after a write whose reply was lost: does the file hold `content`?
    FileReadBack { path: String, content: String },
    /// `/fs/ls` of a folder the explorer opened.
    ExplorerList(String),
    /// An explorer change (`/fs/write`, `/fs/mkdir`, `/fs/rename`, `/fs/delete`).
    ExplorerOp(model::explorer::Op),
    /// A `gitApi` call of a `model::git` task.
    Git(model::git::TaskId),
    /// A history read (`/git/log`, `/git/commit-detail`, `/git/commit-file-diff`).
    History(crate::git_history::HistoryFetch),
}

pub type Fetched = (Fetch, Result<Value, HttpError>);
/// A media file's path and its raw bytes (`GET /fs/raw`).
pub type MediaFetched = (String, Result<Vec<u8>, HttpError>);

fn raw_path(path: &str) -> String {
    format!("/fs/raw?path={}", ui::composer::encode_query(path))
}

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
    use std::thread;
    use std::time::Duration;

    use client::native::Driver;

    use super::*;

    /// How long the link thread waits for an output before it takes new commands.
    const SLICE: Duration = Duration::from_millis(15);

    pub struct Host {
        commands: Sender<ws::Command>,
        outputs: Receiver<ws::Output>,
        config: Config,
        wake: Waker,
        fetched: (Sender<Fetched>, Receiver<Fetched>),
        media: (Sender<MediaFetched>, Receiver<MediaFetched>),
    }

    impl Host {
        pub fn connect(config: Config, wake: Waker) -> Host {
            let (commands, inbox) = mpsc::channel::<ws::Command>();
            let (outbox, outputs) = mpsc::channel();
            let url = config.base_url.clone();
            let (link_config, link_wake) = (config.clone(), wake.clone());
            thread::Builder::new()
                .name("troupe-ws".into())
                .spawn(move || {
                    let mut driver = Driver::new(ws::WsClient::new(link_config));
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
                        let mut woke = false;
                        let mut wait = SLICE;
                        while let Some(output) = driver.next(wait) {
                            if outbox.send(output).is_err() {
                                return;
                            }
                            woke = true;
                            wait = Duration::ZERO;
                        }
                        if woke {
                            link_wake();
                        }
                    }
                })
                .expect("spawn the /ws thread");
            log::info!("[app] connecting to {url}");
            Host { commands, outputs, config, wake, fetched: mpsc::channel(), media: mpsc::channel() }
        }

        /// GETs `path` off the window's thread; the answer comes back through [`Host::fetched`].
        pub fn fetch(&mut self, path: String, tag: Fetch) {
            self.request(HttpRequest::get(&path), tag);
        }

        /// Sends `request` off the window's thread; the answer comes back through
        /// [`Host::fetched`].
        pub fn request(&mut self, request: HttpRequest, tag: Fetch) {
            let (config, wake, done) = (self.config.clone(), self.wake.clone(), self.fetched.0.clone());
            let spawned = thread::Builder::new().name("troupe-http".into()).spawn(move || {
                let result = client::native::http_send(&config, &request);
                if done.send((tag, result)).is_ok() {
                    wake();
                }
            });
            if let Err(e) = spawned {
                log::warn!("[app] could not start an HTTP read: {e}");
            }
        }

        pub fn fetched(&mut self) -> Vec<Fetched> {
            self.fetched.1.try_iter().collect()
        }

        /// Reads `path`'s raw bytes off the window's thread; they come back through
        /// [`Host::media_fetched`].
        pub fn fetch_bytes(&mut self, path: String) {
            let (config, wake, done) = (self.config.clone(), self.wake.clone(), self.media.0.clone());
            let asked = path.clone();
            let spawned = thread::Builder::new().name("troupe-media".into()).spawn(move || {
                let result = client::native::http_get_bytes(&config, &raw_path(&path));
                if done.send((path, result)).is_ok() {
                    wake();
                }
            });
            if let Err(e) = spawned {
                log::warn!("[app] could not start a media read: {e}");
                let _ = self.media.0.send((asked, Err(HttpError::Network(format!("could not start the read: {e}")))));
                (self.wake)();
            }
        }

        pub fn media_fetched(&mut self) -> Vec<MediaFetched> {
            self.media.1.try_iter().collect()
        }

        pub fn send(&mut self, command: ws::Command) {
            if self.commands.send(command).is_err() {
                log::warn!("[app] the /ws thread is gone; command dropped");
            }
        }

        pub fn drain(&mut self, _now: f64) -> Vec<ws::Output> {
            self.outputs.try_iter().collect()
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use std::cell::RefCell;
    use std::rc::Rc;

    use client::Event;
    use client::web::WebDriver;
    use wasm_bindgen::JsCast;
    use wasm_bindgen::closure::Closure;

    use super::*;

    pub struct Host {
        driver: WebDriver<ws::WsClient>,
        queued: Vec<ws::Command>,
        started: bool,
        wake: Waker,
        /// The wake time a `setTimeout` is already armed for.
        armed: Option<f64>,
        config: Config,
        fetched: Rc<RefCell<Vec<Fetched>>>,
        media: Rc<RefCell<Vec<MediaFetched>>>,
    }

    impl Host {
        pub fn connect(config: Config, wake: Waker) -> Host {
            log::info!("[app] connecting to {}", config.base_url);
            let notify = wake.clone();
            Host {
                driver: WebDriver::new(ws::WsClient::new(config.clone()), Rc::new(move || notify())),
                queued: Vec::new(),
                started: false,
                wake,
                armed: None,
                config,
                fetched: Rc::default(),
                media: Rc::default(),
            }
        }

        /// GETs `path` with the page's `fetch`; the answer comes back through [`Host::fetched`].
        pub fn fetch(&mut self, path: String, tag: Fetch) {
            self.request(HttpRequest::get(&path), tag);
        }

        /// Sends `request` with the page's `fetch`; the answer comes back through
        /// [`Host::fetched`].
        pub fn request(&mut self, request: HttpRequest, tag: Fetch) {
            let (config, wake, done) = (self.config.clone(), self.wake.clone(), self.fetched.clone());
            wasm_bindgen_futures::spawn_local(async move {
                let result = client::web::http_send(&config, &request).await;
                done.borrow_mut().push((tag, result));
                wake();
            });
        }

        pub fn fetched(&mut self) -> Vec<Fetched> {
            std::mem::take(&mut *self.fetched.borrow_mut())
        }

        /// Reads `path`'s raw bytes with the page's `fetch`; they come back through
        /// [`Host::media_fetched`].
        pub fn fetch_bytes(&mut self, path: String) {
            let (config, wake, done) = (self.config.clone(), self.wake.clone(), self.media.clone());
            wasm_bindgen_futures::spawn_local(async move {
                let result = client::web::http_get_bytes(&config, &raw_path(&path)).await;
                done.borrow_mut().push((path, result));
                wake();
            });
        }

        pub fn media_fetched(&mut self) -> Vec<MediaFetched> {
            std::mem::take(&mut *self.media.borrow_mut())
        }

        pub fn send(&mut self, command: ws::Command) {
            self.queued.push(command);
            (self.wake)();
        }

        pub fn drain(&mut self, now: f64) -> Vec<ws::Output> {
            let mut outputs = Vec::new();
            if !self.started {
                self.started = true;
                outputs.extend(self.driver.pump(Some(Event::Start), now));
            }
            for command in std::mem::take(&mut self.queued) {
                outputs.extend(self.driver.pump(Some(Event::Command(command)), now));
            }
            outputs.extend(self.driver.pump(None, now));
            self.arm(now);
            outputs
        }

        /// Wakes the page when the machine's next timer comes due.
        fn arm(&mut self, now: f64) {
            let Some(due) = self.driver.next_wake() else {
                return;
            };
            if self.armed.is_some_and(|armed| armed <= due && armed > now) {
                return;
            }
            self.armed = Some(due);
            let wake = self.wake.clone();
            let ms = ((due - now).max(0.0) * 1000.0).ceil() as i32 + 1;
            let fire = Closure::once_into_js(move || wake());
            if let Some(window) = web_sys::window()
                && let Err(e) = window
                    .set_timeout_with_callback_and_timeout_and_arguments_0(fire.unchecked_ref(), ms)
            {
                log::warn!("[app] setTimeout failed: {e:?}");
            }
        }
    }
}

pub use imp::Host;
