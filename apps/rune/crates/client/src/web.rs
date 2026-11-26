//! The browser backend: `web-sys` WebSockets whose callbacks queue
//! [`SocketEvent`]s for a [`WebDriver`], and HTTP through `fetch`. The page owns
//! the clock and the event loop: it calls [`WebDriver::pump`] when `notify`
//! fires or [`WebDriver::next_wake`] comes due.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

use rune_core::machine::Attempt;
use serde_json::Value;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::{Closure, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    AbortSignal, CloseEvent, Headers, MessageEvent, Request, RequestInit, Response, WebSocket,
};

use crate::http::{COMMAND_ID_HEADER, HttpError, HttpRequest, Method, parse_body};
use crate::machine::{Config, Effect, Event, Machine, SocketEvent};

type Queue = Rc<RefCell<VecDeque<SocketEvent>>>;

/// One open browser socket and the callbacks that must live as long as it.
struct Socket {
    ws: WebSocket,
    _on_open: Closure<dyn FnMut(web_sys::Event)>,
    _on_message: Closure<dyn FnMut(MessageEvent)>,
    _on_close: Closure<dyn FnMut(CloseEvent)>,
}

impl Socket {
    fn open(
        socket: Attempt,
        url: &str,
        queue: &Queue,
        notify: &Rc<dyn Fn()>,
    ) -> Result<Self, JsValue> {
        let ws = WebSocket::new(url)?;
        let push = |queue: &Queue, notify: &Rc<dyn Fn()>| {
            let (queue, notify) = (queue.clone(), notify.clone());
            move |event: SocketEvent| {
                queue.borrow_mut().push_back(event);
                notify();
            }
        };
        let opened = push(queue, notify);
        let on_open =
            Closure::<dyn FnMut(web_sys::Event)>::new(move |_| opened(SocketEvent::Opened(socket)));
        let text = push(queue, notify);
        let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
            if let Some(frame) = event.data().as_string() {
                text(SocketEvent::Text(socket, frame));
            }
        });
        let closed = push(queue, notify);
        let on_close = Closure::<dyn FnMut(CloseEvent)>::new(move |event: CloseEvent| {
            closed(SocketEvent::Closed {
                socket,
                code: Some(event.code()),
            });
        });
        ws.set_onopen(Some(on_open.as_ref().unchecked_ref()));
        ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        ws.set_onclose(Some(on_close.as_ref().unchecked_ref()));
        Ok(Self {
            ws,
            _on_open: on_open,
            _on_message: on_message,
            _on_close: on_close,
        })
    }
}

/// Dropping a socket (an explicit close or the driver going away) detaches its
/// handlers before their closures are freed, and closes it.
impl Drop for Socket {
    fn drop(&mut self) {
        self.ws.set_onopen(None);
        self.ws.set_onmessage(None);
        self.ws.set_onclose(None);
        let _ = self.ws.close();
    }
}

/// Runs one machine in the page.
pub struct WebDriver<M: Machine> {
    machine: M,
    queue: Queue,
    notify: Rc<dyn Fn()>,
    sockets: HashMap<Attempt, Socket>,
    wakes: Vec<f64>,
}

impl<M: Machine> WebDriver<M> {
    /// `notify` runs when a socket event is queued; it should schedule a pump.
    pub fn new(machine: M, notify: Rc<dyn Fn()>) -> Self {
        Self {
            machine,
            queue: Rc::default(),
            notify,
            sockets: HashMap::new(),
            wakes: Vec::new(),
        }
    }

    #[must_use]
    pub fn machine(&self) -> &M {
        &self.machine
    }

    /// The earliest time the page must call [`Self::pump`] again.
    #[must_use]
    pub fn next_wake(&self) -> Option<f64> {
        self.wakes.iter().copied().reduce(f64::min)
    }

    /// Step the machine with `event` and every queued socket event and due timer.
    pub fn pump(&mut self, event: Option<Event<M::Command>>, now: f64) -> Vec<M::Output> {
        let mut outputs = Vec::new();
        if let Some(event) = event {
            self.apply(event, now, &mut outputs);
        }
        loop {
            let next = self.queue.borrow_mut().pop_front();
            match next {
                Some(event) => self.apply(Event::Socket(event), now, &mut outputs),
                None => break,
            }
        }
        if self.wakes.iter().any(|t| *t <= now) {
            self.wakes.retain(|t| *t > now);
            self.apply(Event::Tick, now, &mut outputs);
        }
        outputs
    }

    fn apply(&mut self, event: Event<M::Command>, now: f64, outputs: &mut Vec<M::Output>) {
        for effect in self.machine.step(event, now) {
            match effect {
                Effect::Open { socket, url } => {
                    match Socket::open(socket, &url, &self.queue, &self.notify) {
                        Ok(s) => {
                            self.sockets.insert(socket, s);
                        }
                        Err(e) => {
                            log::info!("[client] WebSocket {url} failed: {e:?}");
                            self.queue
                                .borrow_mut()
                                .push_back(SocketEvent::Closed { socket, code: None });
                            (self.notify)();
                        }
                    }
                }
                Effect::Send { socket, text } => {
                    if let Some(s) = self.sockets.get(&socket)
                        && let Err(e) = s.ws.send_with_str(&text)
                    {
                        log::info!("[client] WebSocket send failed: {e:?}");
                    }
                }
                Effect::Close { socket } => {
                    self.sockets.remove(&socket);
                }
                Effect::WakeAt(due) => self.wakes.push(due),
                Effect::Output(output) => outputs.push(output),
            }
        }
    }
}

/// One request, body included, may take this long, as on native.
const HTTP_BUDGET_MS: u32 = 30_000;

/// `GET path` for a raw body (`/fs/raw`): the bytes on success, else the Troupe refusal.
pub async fn http_get_bytes(config: &Config, path: &str) -> Result<Vec<u8>, HttpError> {
    let js = |e: JsValue| HttpError::Network(format!("{e:?}"));
    let url = format!("{}{}", config.base_url.trim_end_matches('/'), path);
    let headers = Headers::new().map_err(js)?;
    if let Some(token) = &config.token {
        headers.set("Authorization", &format!("Bearer {token}")).map_err(js)?;
    }
    let init = RequestInit::new();
    init.set_method("GET");
    init.set_headers(&headers);
    init.set_signal(Some(&AbortSignal::timeout_with_u32(4 * HTTP_BUDGET_MS)));
    let req = Request::new_with_str_and_init(&url, &init).map_err(js)?;
    let window = web_sys::window().ok_or_else(|| HttpError::Network("no window".into()))?;
    let response: Response = JsFuture::from(window.fetch_with_request(&req)).await.map_err(js)?.dyn_into().map_err(js)?;
    let status = response.status();
    let buffer = JsFuture::from(response.array_buffer().map_err(js)?).await.map_err(js)?;
    let bytes = js_sys::Uint8Array::new(&buffer).to_vec();
    if (200..300).contains(&status) {
        return Ok(bytes);
    }
    parse_body(status, &bytes).and(Err(HttpError::Network(format!("HTTP {status}"))))
}

/// Send one HTTP request with `fetch` and read the Troupe body.
pub async fn http_send(config: &Config, request: &HttpRequest) -> Result<Value, HttpError> {
    let js = |e: JsValue| HttpError::Network(format!("{e:?}"));
    let url = format!("{}{}", config.base_url.trim_end_matches('/'), request.path);
    let headers = Headers::new().map_err(js)?;
    headers.set("Accept", "application/json").map_err(js)?;
    if let Some(token) = &config.token {
        headers
            .set("Authorization", &format!("Bearer {token}"))
            .map_err(js)?;
    }
    if let Some(id) = &request.command_id {
        headers.set(COMMAND_ID_HEADER, id).map_err(js)?;
    }
    let init = RequestInit::new();
    init.set_method(request.method.as_str());
    if let (Some(body), Method::Post | Method::Put) = (&request.body, request.method) {
        headers
            .set("Content-Type", "application/json")
            .map_err(js)?;
        init.set_body(&JsValue::from_str(&body.to_string()));
    }
    init.set_headers(&headers);
    // The signal also aborts the body read below.
    init.set_signal(Some(&AbortSignal::timeout_with_u32(HTTP_BUDGET_MS)));
    let req = Request::new_with_str_and_init(&url, &init).map_err(js)?;
    let window = web_sys::window().ok_or_else(|| HttpError::Network("no window".into()))?;
    let response: Response = JsFuture::from(window.fetch_with_request(&req))
        .await
        .map_err(js)?
        .dyn_into()
        .map_err(js)?;
    let status = response.status();
    let buffer = JsFuture::from(response.array_buffer().map_err(js)?)
        .await
        .map_err(js)?;
    parse_body(status, &js_sys::Uint8Array::new(&buffer).to_vec())
}
