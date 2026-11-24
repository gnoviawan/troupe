//! The app's `/ws` link to the Troupe server, one backend per target. Both take commands from
//! the view's handlers and hand outputs back to `poll`; the machine itself is `client::ws`.
//!
//! Native: the blocking `client::native::Driver` runs on its own thread and wakes the window
//! when an output is queued. Web: the page's `client::web::WebDriver` is pumped from `poll`,
//! and a `setTimeout` wakes the page for the machine's next timer.

use client::{Config, ws};
use rune::Waker;

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
    }

    impl Host {
        pub fn connect(config: Config, wake: Waker) -> Host {
            let (commands, inbox) = mpsc::channel::<ws::Command>();
            let (outbox, outputs) = mpsc::channel();
            let url = config.base_url.clone();
            thread::Builder::new()
                .name("troupe-ws".into())
                .spawn(move || {
                    let mut driver = Driver::new(ws::WsClient::new(config));
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
                            wake();
                        }
                    }
                })
                .expect("spawn the /ws thread");
            log::info!("[app] connecting to {url}");
            Host { commands, outputs }
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
    }

    impl Host {
        pub fn connect(config: Config, wake: Waker) -> Host {
            log::info!("[app] connecting to {}", config.base_url);
            let notify = wake.clone();
            Host {
                driver: WebDriver::new(ws::WsClient::new(config), Rc::new(move || notify())),
                queued: Vec::new(),
                started: false,
                wake,
                armed: None,
            }
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
