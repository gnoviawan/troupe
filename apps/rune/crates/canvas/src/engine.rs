//! One mounted editor: FigCraft's `FigApp` and its egui context, answering the host's control
//! messages in process. The messages keep the shape figcraft-web's `postMessage` bridge uses
//! (`apps/figcraft-web/src/bridge.rs`), so the host side cannot tell the two apart:
//!
//! - request `{id, method, params, mountSeq}`;
//! - reply `{id, mountSeq, ok: true, result}` or `{id, mountSeq, ok: false, error: {code, message}}`;
//! - event `{method: "event.*", params, mountSeq}`: `event.ready` after the second frame (the
//!   first only installs fonts), `event.dirtyChanged {dirty, contentVersion}` and
//!   `event.crashed {reason}`.
//!
//! `params` and `result` are JSON text throughout. A `ui.*` request waits for the next drawn
//! frame; while the editor is not on screen it is answered `CANVAS_HIDDEN`, as the web bundle
//! answers it in a hidden tab.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::{Receiver, TryRecvError};

use figcraft_engine::Session;
use figcraft_ui_egui::{ControlReply, FigApp, NoServices, Shell};
use serde_json::{Map, Value, json};

use crate::host::{Answer, Code, Host, HostError};

/// A `ui.*` request waiting for the frame that answers it.
struct Pending {
    id: Value,
    mount_seq: Value,
    reply: Receiver<ControlReply>,
}

pub struct Engine {
    fig: FigApp,
    ctx: egui::Context,
    host: Host,
    mount_seq: Value,
    pending: Vec<Pending>,
    outbox: Vec<Value>,
    frames: u32,
    visible: bool,
    crashed: bool,
    /// Texture changes of frames run without painting, applied by the next painted frame.
    textures: egui::TexturesDelta,
    /// How long after the last frame egui asked for the next one.
    repaint: std::time::Duration,
}

impl Engine {
    /// An empty kiosk editor for the host mount `mount_seq`, as the bundle opens with
    /// `?embed=kiosk&ms=<mount_seq>`.
    pub fn new(mount_seq: u64) -> Engine {
        Self::with_repaint(mount_seq, None)
    }

    /// Like [`Engine::new`], wiring `on_repaint` to egui's repaint callback so FigCraft's
    /// background render worker wakes the Rune window when a rendered canvas texture arrives.
    pub fn with_repaint(mount_seq: u64, on_repaint: Option<Box<dyn Fn() + Send + Sync>>) -> Engine {
        let mut fig = FigApp::new(Session::new(), Box::new(NoServices));
        fig.shell = Shell::Kiosk;
        let ctx = egui::Context::default();
        if let Some(cb) = on_repaint {
            ctx.set_request_repaint_callback(move |_| cb());
        }
        Engine {
            fig,
            ctx,
            host: Host::default(),
            mount_seq: json!(mount_seq),
            pending: Vec::new(),
            outbox: Vec::new(),
            frames: 0,
            visible: false,
            crashed: false,
            textures: egui::TexturesDelta::default(),
            repaint: std::time::Duration::ZERO,
        }
    }

    pub fn ready(&self) -> bool {
        self.frames >= 2
    }

    pub fn crashed(&self) -> bool {
        self.crashed
    }

    /// Whether the last frame was drawn on screen; `ui.*` requests need it.
    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    pub fn has_messages(&self) -> bool {
        !self.outbox.is_empty()
    }

    /// The paint jobs of a frame's shapes.
    pub fn tessellate(&self, shapes: Vec<egui::epaint::ClippedShape>, pixels_per_point: f32) -> Vec<egui::ClippedPrimitive> {
        self.ctx.tessellate(shapes, pixels_per_point)
    }

    /// The messages for the host since the last call: replies and events, in order.
    pub fn take_outbox(&mut self) -> Vec<Value> {
        std::mem::take(&mut self.outbox)
    }

    /// Answer one host message at once, or queue a `ui.*` one for the next frame, then report a
    /// change it made to the document. Anything but a request is ignored, as the bundle ignores it.
    pub fn receive(&mut self, message: &Value) {
        let (Some(id), mount_seq) = (message.get("id").cloned(), message.get("mountSeq").cloned().unwrap_or(Value::Null)) else {
            return;
        };
        if self.crashed {
            let error = HostError { code: Code::EngineError, message: "the editor is not running".into() };
            self.reply(id, mount_seq, Err(error));
            return;
        }
        let method = message.get("method").and_then(Value::as_str).unwrap_or_default().to_owned();
        let params = message.get("params").and_then(Value::as_str).unwrap_or_default().to_owned();
        let hidden = !self.visible;
        let (host, fig, ctx) = (&mut self.host, &mut self.fig, &self.ctx);
        let answer = catch_unwind(AssertUnwindSafe(|| host.call(fig, ctx, &method, &params, hidden)));
        match answer {
            Ok(Answer::Now(result)) => self.reply(id, mount_seq, result),
            Ok(Answer::Later(reply)) => self.pending.push(Pending { id, mount_seq, reply }),
            Ok(Answer::FetchFonts) => {
                let error = HostError { code: Code::UnknownMethod, message: "`host.fallbackFonts` is not served in process".into() };
                self.reply(id, mount_seq, Err(error));
            }
            Err(panic) => {
                self.crash(&panic_text(panic.as_ref()));
                return;
            }
        }
        self.report();
    }

    /// Runs one editor frame on `raw` and returns what to paint, with the texture changes of
    /// frames run unpainted before it. `None` once the editor has crashed.
    pub fn frame(&mut self, raw: egui::RawInput) -> Option<egui::FullOutput> {
        if self.crashed {
            return None;
        }
        let (fig, ctx) = (&mut self.fig, &self.ctx);
        let ran = catch_unwind(AssertUnwindSafe(|| {
            let mut raw = raw;
            let events = figcraft_ui_egui::windows::take_input(fig, raw.viewport_id);
            raw.events.extend(events);
            figcraft_ui_egui::windows::guard(fig, ctx, |fig| fig.background_tick(ctx));
            ctx.run_ui(raw, |ui| fig.ui(ui))
        }));
        let mut out = match ran {
            Ok(out) => out,
            Err(panic) => {
                self.crash(&panic_text(panic.as_ref()));
                return None;
            }
        };
        self.repaint = out.viewport_output.get(&egui::ViewportId::ROOT).map_or(std::time::Duration::MAX, |v| v.repaint_delay);
        self.after_frame();
        let mut textures = std::mem::take(&mut self.textures);
        textures.append(std::mem::take(&mut out.textures_delta));
        out.textures_delta = textures;
        Some(out)
    }

    /// A frame nobody sees (the editor loads before its tab shows): its textures wait for the
    /// next painted frame.
    pub fn frame_unpainted(&mut self, raw: egui::RawInput) {
        if let Some(mut out) = self.frame(raw) {
            self.textures = std::mem::take(&mut out.textures_delta);
        }
    }

    /// When the editor wants its next frame, measured from the last one: at once while it loads
    /// or a `ui.*` request waits, else when egui asked.
    pub fn repaint_after(&self) -> Option<std::time::Duration> {
        if self.crashed {
            return None;
        }
        if !self.ready() || !self.pending.is_empty() {
            return Some(std::time::Duration::ZERO);
        }
        (self.repaint != std::time::Duration::MAX).then_some(self.repaint)
    }

    /// After each frame: answer the `ui.*` requests it ran, say when the editor is ready, and
    /// report a change the user made in it.
    fn after_frame(&mut self) {
        let mut answered = Vec::new();
        self.pending.retain(|p| match p.reply.try_recv() {
            Ok(r) => {
                let result = r.result.map(|v| v.to_string()).map_err(|message| HostError { code: Code::EngineError, message });
                answered.push((p.id.clone(), p.mount_seq.clone(), result));
                false
            }
            Err(TryRecvError::Empty) => true,
            Err(TryRecvError::Disconnected) => {
                let error = HostError { code: Code::EngineError, message: "the request was dropped".into() };
                answered.push((p.id.clone(), p.mount_seq.clone(), Err(error)));
                false
            }
        });
        for (id, mount_seq, result) in answered {
            self.reply(id, mount_seq, result);
        }
        self.frames = self.frames.saturating_add(1);
        if self.frames == 2 {
            self.host.mark_ready(&self.fig.session);
            self.event("event.ready", "{}".into());
        }
        self.report();
    }

    fn report(&mut self) {
        if let Some(state) = self.host.changed(&self.fig.session) {
            self.event("event.dirtyChanged", state.to_json());
        }
    }

    fn crash(&mut self, reason: &str) {
        log::error!("[canvas] the editor stopped: {reason}");
        self.crashed = true;
        for p in std::mem::take(&mut self.pending) {
            let error = HostError { code: Code::EngineError, message: "the editor stopped".into() };
            self.reply(p.id, p.mount_seq, Err(error));
        }
        self.event("event.crashed", json!({"reason": reason}).to_string());
    }

    fn reply(&mut self, id: Value, mount_seq: Value, result: Result<String, HostError>) {
        let mut msg = Map::new();
        msg.insert("id".into(), id);
        msg.insert("mountSeq".into(), mount_seq);
        match result {
            Ok(text) => {
                msg.insert("ok".into(), Value::Bool(true));
                msg.insert("result".into(), Value::String(text));
            }
            Err(e) => {
                msg.insert("ok".into(), Value::Bool(false));
                msg.insert("error".into(), json!({"code": e.code.as_str(), "message": e.message}));
            }
        }
        self.outbox.push(Value::Object(msg));
    }

    fn event(&mut self, method: &str, params: String) {
        self.outbox.push(json!({"method": method, "params": params, "mountSeq": self.mount_seq}));
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.textures.clear();
    }
}

fn panic_text(panic: &(dyn std::any::Any + Send)) -> String {
    panic.downcast_ref::<&str>().map(ToString::to_string).or_else(|| panic.downcast_ref::<String>().cloned()).unwrap_or_else(|| "panic".into())
}
