//! Connects `ui::canvas::Canvases` to the Troupe server (`/canvas/*` HTTP routes and `/ws`
//! gateway frames) and to the platform's editor host: in-process `troupe-canvas` (egui-wgpu in an
//! `El::effect` slot) on desktop, and the DOM `<iframe>` host (`canvas_web`) on wasm.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use client::http::{HttpError, HttpRequest, Method};
use model::explorer::Entry;
use model::workspace::Tab;
use rune::{Color, Cx, El, Tx, el};
use serde_json::{Value, json};
use theme::Tokens;
use troupe_protocol::ws::{CanvasLeaseLostReason, WsError, WsRequestType, WsServerFrame};
use ui::canvas::{Effect, LeaseLoss, NEW_CANVAS_DOC_NAME, OpenInfo, Then, canvas_doc_file, pick_canvas_doc, same_doc};
use ui::overlay::ToastKind;

use crate::State;
use crate::canvas_web::WebCanvases;
use crate::editor_host::epoch_ms;
use crate::host::Fetch;
use crate::sync::Pending;

/// Largest slice of a `canvas_reply` sent in one `/ws` frame (matches `REPLY_SLICE_UNITS`).
pub(crate) const REPLY_SLICE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub enum CanvasFetch {
    ProjectList { project_id: String, root: String },
    Post { project_id: String, then: Then },
}

#[derive(Clone, Debug, PartialEq)]
pub enum CanvasWsPending {
    Mount { project_id: String, mount_seq: u64 },
    Unmount,
    Reply { slice_id: Option<u64> },
}

struct PendingSlice {
    call_id: u64,
    mount_seq: u64,
    result: String,
    offset: usize,
}

#[derive(Default)]
pub struct CanvasHost {
    #[cfg(not(target_arch = "wasm32"))]
    editors: BTreeMap<String, (u64, troupe_canvas::Handle)>,
    web: WebCanvases,
    /// Projects whose canvas tab is visible in the current frame (computed in `view`).
    shown_now: RefCell<BTreeSet<String>>,
    /// Projects last reported visible to `Canvases::set_visible`.
    reported_visible: BTreeSet<String>,
    last_theme: Option<Value>,
    opening_project: BTreeSet<String>,
    next_slice_id: u64,
    slices: BTreeMap<u64, PendingSlice>,
}

/// Converts Troupe's semantic theme tokens into the `#rrggbb` `host.theme` payload.
pub(crate) fn theme_tokens(t: &Tokens) -> Value {
    let c = &t.colors;
    let hex = |color: Color| {
        let ch = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        format!("#{:02x}{:02x}{:02x}", ch(color.r), ch(color.g), ch(color.b))
    };
    json!({
        "mode": if t.dark { "dark" } else { "light" },
        "background": hex(c.background),
        "card": hex(c.card),
        "foreground": hex(c.foreground),
        "mutedForeground": hex(c.muted_foreground),
        "border": hex(c.border),
        "primary": hex(c.primary),
        "primaryFill": hex(c.primary_fill),
        "destructive": hex(c.destructive),
    })
}

/// Splits `text` starting at `start` on a UTF-8 char boundary, at most `max_bytes` long.
pub(crate) fn slice_end_utf8(text: &str, start: usize, max_bytes: usize) -> usize {
    let mut end = (start + max_bytes).min(text.len());
    while end > start && !text.is_char_boundary(end) {
        end -= 1;
    }
    end
}

fn canvases_get(s: &State) -> &ui::canvas::Canvases {
    &s.canvases
}

fn canvases_lens(s: &mut State) -> &mut ui::canvas::Canvases {
    &mut s.canvases
}

fn locale_tag() -> String {
    #[cfg(target_arch = "wasm32")]
    if let Some(lang) = web_sys::window().and_then(|w| w.navigator().language()).filter(|l| !l.is_empty()) {
        return lang;
    }
    "en-US".to_owned()
}

impl State {
    /// Opens the active project's first `.figcraft` document (or `design.figcraft`), matching
    /// `openProjectCanvas`.
    pub fn open_project_canvas(&mut self) {
        let Some(project) = self.active_project() else { return };
        let Some(root) = project.path.clone().filter(|p| !p.is_empty()) else {
            self.toast_error("Canvas unavailable", "This project has no folder.");
            return;
        };
        let project_id = project.id.clone();
        if !self.canvas_host.opening_project.insert(project_id.clone()) {
            return;
        }
        let url = format!("/fs/ls?path={}", ui::composer::encode_query(&root));
        if let Some(host) = &mut self.host {
            host.fetch(url, Fetch::Canvas(CanvasFetch::ProjectList { project_id, root }));
        } else {
            self.canvas_host.opening_project.remove(&project_id);
            let doc_path = format!("{}/{NEW_CANVAS_DOC_NAME}", root.trim_end_matches('/'));
            self.open_canvas(&project_id, &doc_path);
        }
    }

    /// Opens a specific `.figcraft` file in the active project (e.g. from the file explorer).
    pub fn open_canvas_for_active(&mut self, doc_path: &str) {
        let Some(project_id) = self.active_project().map(|p| p.id.clone()) else { return };
        self.open_canvas(&project_id, doc_path);
    }

    /// Mounts `doc_path` for `project_id` and opens its canvas tab.
    pub fn open_canvas(&mut self, project_id: &str, doc_path: &str) {
        self.refresh_canvas_facts(project_id, doc_path);
        self.canvases.open(project_id, doc_path, true, epoch_ms());
        self.run_canvas_effects();
    }

    /// Saves the active pane's canvas tab if one is active; returns true when handled.
    pub(crate) fn save_active_canvas(&mut self) -> bool {
        let Some(project_id) = self.active_canvas_project() else { return false };
        self.canvases.save(&project_id);
        self.run_canvas_effects();
        true
    }

    fn active_canvas_project(&self) -> Option<String> {
        let pane_id = &self.workspace.tree().active_pane_id;
        let leaf = model::workspace::leaves(self.workspace.root()).into_iter().find(|l| &l.id == pane_id)?;
        let tab = leaf.tabs.iter().find(|t| Some(t.id()) == leaf.active_tab_id.as_deref())?;
        match tab {
            Tab::Canvas { project_id, .. } => Some(project_id.clone()),
            _ => None,
        }
    }

    fn refresh_canvas_facts(&mut self, project_id: &str, doc_path: &str) {
        if let Some(host) = &self.host {
            self.canvases.facts.client_instance_id = host.client_id().to_owned();
        } else if self.canvases.facts.client_instance_id.is_empty() {
            self.canvases.facts.client_instance_id = "rune-local".to_owned();
        }
        let tokens = theme_tokens(self.tokens());
        self.canvases.facts.theme = Some(tokens.clone());
        self.canvas_host.last_theme = Some(tokens);
        self.canvases.facts.locale = locale_tag();
        self.canvases.facts.release = !cfg!(debug_assertions);
        self.canvases.facts.in_process = cfg!(not(target_arch = "wasm32"));
        let project = self.projects.store.projects.iter().find(|p| p.id == project_id);
        let file = canvas_doc_file(
            doc_path,
            project.map(|p| p.name.as_str()),
            project.and_then(|p| p.path.as_deref()),
        );
        self.canvases.facts.file.insert(project_id.to_owned(), file);
        let s = &self.settings.settings;
        let mut opts = serde_json::Map::new();
        for (setting, field) in [
            ("canvasAutosaveMs", "autosaveMs"),
            ("canvasAutosaveMaxMs", "autosaveMaxMs"),
            ("canvasLeaseGraceMs", "leaseGraceMs"),
            ("canvasMaxMounted", "maxMounted"),
            ("canvasCrashTimeouts", "crashTimeouts"),
        ] {
            if let Some(v) = s.get(setting) {
                opts.insert(field.into(), v.clone());
            }
        }
        self.canvases.options = Value::Object(opts);
    }

    /// Renders a `Tab::Canvas` body (desktop or phone).
    pub(crate) fn canvas_tab_body(&self, t: &'static Tokens, tab: &Tab, phone: bool) -> Option<El> {
        let Tab::Canvas { project_id, doc_path, .. } = tab else { return None };
        let body = ui::canvas::view::panel(self, canvases_get, canvases_lens, t, project_id, doc_path, phone, |mount_seq| {
            self.canvas_slot_el(t, project_id, mount_seq)
        });
        Some(body)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn canvas_slot_el(&self, t: &'static Tokens, project_id: &str, mount_seq: u64) -> El {
        let Some((seq, handle)) = self.canvas_host.editors.get(project_id) else {
            return el().key(format!("canvas-slot-pending-{project_id}")).w_full().grow(1.0).min_h(0.0).fill(t.colors.background);
        };
        if *seq != mount_seq {
            return el().key(format!("canvas-slot-pending-{project_id}")).w_full().grow(1.0).min_h(0.0).fill(t.colors.background);
        }
        let mark = handle.mark();
        let slot_key = troupe_canvas::slot_key(mark);
        let cursor = handle.cursor();
        let p_move = project_id.to_owned();
        let p_hover = project_id.to_owned();
        let p_drag = project_id.to_owned();
        let p_sec = project_id.to_owned();
        let p_wheel = project_id.to_owned();
        let p_mag = project_id.to_owned();
        let p_key = project_id.to_owned();
        let p_keyup = project_id.to_owned();
        let focus_key_drag = slot_key.clone();
        let focus_key_sec = slot_key.clone();

        el().key(slot_key)
            .effect(mark)
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .fill(t.colors.background)
            .focusable(true)
            .cursor(cursor)
            .pointer_buttons([rune::PointerButton::Middle, rune::PointerButton::Back, rune::PointerButton::Forward])
            .on_pointer_move_local(move |s: &mut State, p: rune::PointerEvent, _| {
                if let Some((_, h)) = s.canvas_host.editors.get(&p_move) {
                    h.input(troupe_canvas::egui::Event::PointerMoved(troupe_canvas::egui::pos2(p.x, p.y)));
                }
            })
            .on_hover_change(move |s: &mut State, on: bool, _| {
                if !on && let Some((_, h)) = s.canvas_host.editors.get(&p_hover) {
                    h.input(troupe_canvas::egui::Event::PointerGone);
                }
            })
            .on_drag(move |s: &mut State, d: &mut rune::Drag, tx: &mut Tx| {
                let button = troupe_canvas::input::pointer_button(tx.button());
                let pos = troupe_canvas::egui::pos2(d.x, d.y);
                let modifiers = troupe_canvas::input::modifiers(tx.mods());
                if d.phase == rune::Phase::Down {
                    tx.focus(&focus_key_drag, false);
                }
                if let Some((_, h)) = s.canvas_host.editors.get(&p_drag) {
                    match d.phase {
                        rune::Phase::Down => {
                            h.set_focused(true);
                            h.inputs([
                                troupe_canvas::egui::Event::PointerMoved(pos),
                                troupe_canvas::egui::Event::PointerButton { pos, button, pressed: true, modifiers },
                            ]);
                        }
                        rune::Phase::Move => h.input(troupe_canvas::egui::Event::PointerMoved(pos)),
                        rune::Phase::Up => {
                            h.inputs([
                                troupe_canvas::egui::Event::PointerMoved(pos),
                                troupe_canvas::egui::Event::PointerButton { pos, button, pressed: false, modifiers },
                            ]);
                        }
                    }
                }
            })
            .on_secondary_press_local(move |s: &mut State, p: rune::PointerEvent, tx: &mut Tx| {
                tx.focus(&focus_key_sec, false);
                let pos = troupe_canvas::egui::pos2(p.x, p.y);
                let modifiers = troupe_canvas::input::modifiers(tx.mods());
                if let Some((_, h)) = s.canvas_host.editors.get(&p_sec) {
                    h.set_focused(true);
                    h.inputs([
                        troupe_canvas::egui::Event::PointerMoved(pos),
                        troupe_canvas::egui::Event::PointerButton { pos, button: troupe_canvas::egui::PointerButton::Secondary, pressed: true, modifiers },
                        troupe_canvas::egui::Event::PointerButton { pos, button: troupe_canvas::egui::PointerButton::Secondary, pressed: false, modifiers },
                    ]);
                }
            })
            .on_wheel(move |s: &mut State, e: rune::WheelEvent, _| {
                if let Some((_, h)) = s.canvas_host.editors.get(&p_wheel) {
                    h.inputs(troupe_canvas::input::wheel_events(&e));
                    return true;
                }
                false
            })
            .on_magnify(move |s: &mut State, e: rune::MagnifyEvent, _| {
                if let Some((_, h)) = s.canvas_host.editors.get(&p_mag) {
                    h.inputs(troupe_canvas::input::magnify_events(&e));
                    return true;
                }
                false
            })
            .on_key(move |s: &mut State, key: &rune::KeyEvent, tx: &mut Tx| {
                if key.is_command('s') && !key.mods.shift && !key.mods.alt {
                    s.canvases.save(&p_key);
                    s.run_canvas_effects();
                    return true;
                }
                // Let app-level command palette / settings / theme shortcuts bubble to on_key.
                if let Some(chord) = crate::keys::chord(key, s.shortcuts.platform)
                    && let Some(sc) = model::shortcuts::find_match(&s.shortcuts, &chord)
                    && matches!(
                        sc.id.as_str(),
                        "commandPalette" | "commandPaletteAlt" | "settings" | "colorThemePicker" | "sidebarToggle" | "toggleFileExplorer" | "zoomIn" | "zoomOut" | "zoomReset"
                    )
                {
                    return false;
                }
                if key.is_command('v') && !key.mods.shift && !key.mods.alt {
                    let project = p_key.clone();
                    tx.read_clipboard(move |s: &mut State, content, _| {
                        let text = content.text();
                        if !text.is_empty() && let Some((_, h)) = s.canvas_host.editors.get(&project) {
                            h.input(troupe_canvas::egui::Event::Paste(text.to_owned()));
                        }
                    });
                    return true;
                }
                if let Some((_, h)) = s.canvas_host.editors.get(&p_key) {
                    h.set_focused(true);
                    let events = troupe_canvas::input::key_events(key, true);
                    if !events.is_empty() {
                        h.inputs(events);
                        return true;
                    }
                }
                false
            })
            .on_key_up(move |s: &mut State, key: &rune::KeyEvent, _| {
                if let Some((_, h)) = s.canvas_host.editors.get(&p_keyup) {
                    let events = troupe_canvas::input::key_events(key, false);
                    if !events.is_empty() {
                        h.inputs(events);
                        return true;
                    }
                }
                false
            })
    }

    #[cfg(target_arch = "wasm32")]
    fn canvas_slot_el(&self, t: &'static Tokens, project_id: &str, _mount_seq: u64) -> El {
        el().key(format!("canvas-web-slot-{project_id}")).w_full().grow(1.0).min_h(0.0).fill(t.colors.background)
    }

    /// Called from `view` after layout: records visible canvas tabs and positions wasm iframes
    /// (moving them offscreen whenever a Rune overlay or phone layout is active so menus remain
    /// readable over the canvas).
    pub(crate) fn sync_canvas_slots(&self, cx: &Cx<State>, phone: bool, covered_by_overlay: bool) {
        let mut shown = BTreeSet::new();
        let mut rects = BTreeMap::new();
        if !phone && self.workspace.record().is_some() {
            let tree = self.workspace.tree();
            let fullscreen = tree.fullscreen_pane_id.as_deref();
            for leaf in model::workspace::leaves(self.workspace.root()) {
                if fullscreen.is_some_and(|id| id != leaf.id) {
                    continue;
                }
                let active = leaf.tabs.iter().find(|t| Some(t.id()) == leaf.active_tab_id.as_deref());
                if let Some(Tab::Canvas { project_id, .. }) = active {
                    shown.insert(project_id.clone());
                    if !covered_by_overlay && let Some(rect) = cx.layout(&format!("canvas-web-slot-{project_id}")) {
                        rects.insert(project_id.clone(), rect);
                    }
                }
            }
        }
        *self.canvas_host.shown_now.borrow_mut() = shown;
        self.canvas_host.web.sync_positions(&rects);
    }

    /// Pumps mounted editors, visibility changes, theme sync, clipboard/URL actions, and timers.
    pub(crate) fn pump_canvases(&mut self, tx: &mut Tx) -> (bool, Option<f64>) {
        let mut changed = false;
        let now_ms = epoch_ms();

        // Sync visibility changes detected during the last view pass.
        let shown = self.canvas_host.shown_now.borrow().clone();
        if shown != self.canvas_host.reported_visible {
            let prev = std::mem::replace(&mut self.canvas_host.reported_visible, shown.clone());
            for id in prev.difference(&shown) {
                self.canvases.set_visible(id, false);
                changed = true;
            }
            for id in shown.difference(&prev) {
                self.canvases.set_visible(id, true);
                changed = true;
            }
        }

        // Sync theme token changes to ready editors without reloading.
        let tokens = theme_tokens(self.tokens());
        if self.canvas_host.last_theme.as_ref() != Some(&tokens) {
            self.canvas_host.last_theme = Some(tokens.clone());
            self.canvases.facts.theme = Some(tokens);
            self.canvases.push_theme_all();
        }

        // Drain editor messages and platform actions.
        #[cfg(not(target_arch = "wasm32"))]
        {
            let focused_key = tx.focused().map(str::to_owned);
            let mut inbox = Vec::new();
            let mut actions = Vec::new();
            for (project_id, (_, handle)) in &self.canvas_host.editors {
                let is_focused = focused_key.as_deref() == Some(troupe_canvas::slot_key(handle.mark()).as_str());
                handle.set_focused(is_focused);
                let msgs = handle.take_messages();
                if !msgs.is_empty() {
                    inbox.push((project_id.clone(), msgs));
                }
                actions.extend(handle.take_actions());
            }
            for (project_id, msgs) in inbox {
                self.canvases.editor_messages(&project_id, &msgs, now_ms);
                changed = true;
            }
            for action in actions {
                match action {
                    troupe_canvas::PlatformAction::CopyText(text) => tx.copy(text),
                    troupe_canvas::PlatformAction::OpenUrl(url) => {
                        if let Err(e) = tx.open_url(&url) {
                            log::warn!("[canvas] open_url failed: {e}");
                        }
                    }
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = tx;
            for (project_id, msgs) in self.canvas_host.web.take_messages() {
                self.canvases.editor_messages(&project_id, &msgs, now_ms);
                changed = true;
            }
        }

        let next_due_ms = self.canvases.tick(now_ms);
        changed |= self.run_canvas_effects();
        let next_wake = next_due_ms.map(|due| self.now + ((due - now_ms).max(0.0) / 1000.0));
        (changed, next_wake)
    }

    /// Forwards IME composition events to the focused desktop canvas editor.
    pub(crate) fn canvas_ime(&mut self, ime: &rune::Ime, tx: &mut Tx) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let Some(focused) = tx.focused() else { return false };
            for (_, handle) in self.canvas_host.editors.values() {
                if troupe_canvas::slot_key(handle.mark()) == focused {
                    if let Some(ev) = troupe_canvas::input::ime_event(ime) {
                        handle.input(ev);
                    }
                    return true;
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        let _ = (ime, tx);
        false
    }

    /// Handles `/ws` `canvas_call` and `canvas_lease_lost` frames.
    pub(crate) fn canvas_ws_frame(&mut self, frame: WsServerFrame) {
        let now_ms = epoch_ms();
        match frame {
            WsServerFrame::CanvasCall(call) => {
                let deadline = call.timeout_ms.map(|ms| now_ms + ms as f64);
                self.canvases.gateway_call(call.call_id, &call.project_id, call.mount_seq, &call.method, &call.params, deadline, now_ms);
                self.run_canvas_effects();
            }
            WsServerFrame::CanvasLeaseLost(lost) => {
                let reason = match lost.reason {
                    CanvasLeaseLostReason::Evicted => LeaseLoss::Evicted,
                    CanvasLeaseLostReason::TakenOver => LeaseLoss::Elsewhere,
                };
                self.canvases.lease_lost(&lost.project_id, lost.mount_seq, reason);
                self.run_canvas_effects();
            }
            _ => {}
        }
    }

    /// Re-reports held canvas mounts after a `/ws` reconnect.
    pub(crate) fn canvas_reconnected(&mut self) {
        self.canvases.reconnected();
        self.run_canvas_effects();
    }

    /// Handles `/ws` replies for `canvas_mount`, `canvas_unmount` and chunked `canvas_reply`.
    pub(crate) fn canvas_ws_reply(&mut self, pending: CanvasWsPending, result: Result<Option<Value>, WsError>) {
        match pending {
            CanvasWsPending::Mount { project_id, mount_seq } => {
                if let Ok(Some(ref v)) = result
                    && v.get("attached").and_then(Value::as_bool) == Some(false)
                {
                    let held_elsewhere = v.get("heldElsewhere").and_then(Value::as_bool).unwrap_or(false);
                    self.canvases.mount_refused(&project_id, mount_seq, held_elsewhere);
                    self.run_canvas_effects();
                } else if let Err(e) = result {
                    log::warn!("[canvas] canvas_mount failed for {project_id}: {}: {}", e.code, e.message);
                }
            }
            CanvasWsPending::Unmount => {
                if let Err(e) = result {
                    log::warn!("[canvas] canvas_unmount failed: {}: {}", e.code, e.message);
                }
            }
            CanvasWsPending::Reply { slice_id: Some(slice_id) } => {
                let accepted = matches!(&result, Ok(Some(v)) if v.get("accepted").and_then(Value::as_bool) == Some(true));
                if let Some(mut slice) = self.canvas_host.slices.remove(&slice_id)
                    && accepted
                {
                    self.send_next_slice(slice_id, &mut slice);
                }
            }
            CanvasWsPending::Reply { slice_id: None } => {
                if let Err(e) = result {
                    log::warn!("[canvas] canvas_reply failed: {}: {}", e.code, e.message);
                }
            }
        }
    }

    /// Handles HTTP answers for `/fs/ls` (open project canvas) and `/canvas/*` routes.
    pub(crate) fn canvas_fetched(&mut self, tag: CanvasFetch, result: Result<Value, HttpError>) {
        match tag {
            CanvasFetch::ProjectList { project_id, root } => {
                self.canvas_host.opening_project.remove(&project_id);
                let entries = match result.and_then(|v| Entry::parse_listing(&v).ok_or_else(|| HttpError::Network("unreadable directory listing".into()))) {
                    Ok(entries) => entries,
                    Err(e) => {
                        self.toast_error("Could not read the project folder", &e.to_string());
                        return;
                    }
                };
                let picked = pick_canvas_doc(entries.iter().map(|e| (e.name.as_str(), e.dir))).unwrap_or(NEW_CANVAS_DOC_NAME);
                let doc_path = format!("{}/{picked}", root.trim_end_matches('/'));
                self.open_canvas(&project_id, &doc_path);
            }
            CanvasFetch::Post { project_id, then } => {
                let mapped = result.map_err(|e| match e {
                    HttpError::Refused { code, message, .. } => (code, message),
                    HttpError::Network(m) => ("NETWORK_ERROR".into(), m),
                    HttpError::InProgress => ("COMMAND_IN_PROGRESS".into(), String::new()),
                });
                self.canvases.replied(&project_id, then, mapped, epoch_ms());
                self.run_canvas_effects();
            }
        }
    }

    /// Checks disk for any mounted canvas whose document path matches `path`.
    pub(crate) fn canvas_file_changed(&mut self, path: &str) {
        let projects: Vec<String> = self
            .canvases
            .sessions
            .values()
            .filter(|s| same_doc(&s.doc_path, path))
            .map(|s| s.project_id.clone())
            .collect();
        for project_id in projects {
            self.canvases.check_disk(&project_id);
        }
        self.run_canvas_effects();
    }

    /// Executes queued `ui::canvas::Effect`s. Returns true if any effect ran.
    pub(crate) fn run_canvas_effects(&mut self) -> bool {
        let effects = self.canvases.take_effects();
        if effects.is_empty() {
            return false;
        }
        for effect in effects {
            match effect {
                Effect::Post { route, body, then } => {
                    let project_id = body.get("projectId").and_then(Value::as_str).unwrap_or_default().to_owned();
                    let req = HttpRequest { method: Method::Post, path: route.into(), body: Some(body), command_id: None };
                    if let Some(host) = &mut self.host {
                        host.request(req, Fetch::Canvas(CanvasFetch::Post { project_id, then }));
                    } else {
                        self.canvases.replied(&project_id, then, Err(("NETWORK_ERROR".into(), "No server".into())), epoch_ms());
                    }
                }
                Effect::Ws { kind, payload } => self.send_canvas_ws(kind, payload),
                Effect::MountEditor { project_id, info } => self.mount_canvas_editor(&project_id, info),
                Effect::DropEditor { project_id } => self.drop_canvas_editor(&project_id),
                Effect::ToEditor { project_id, messages } => self.post_to_canvas_editor(&project_id, &messages),
                Effect::ShowTab { project_id, doc_path } => {
                    self.workspace.step(model::workspace::Event::AddCanvasTab(project_id, doc_path, None));
                }
                Effect::Toast { error, title, detail } => {
                    let kind = if error { ToastKind::Error } else { ToastKind::Info };
                    self.toasts.push(self.now, kind, title, detail.as_deref());
                }
            }
        }
        true
    }

    fn send_canvas_ws(&mut self, kind: &str, payload: Value) {
        match kind {
            "canvas_mount" => {
                let project_id = payload.get("projectId").and_then(Value::as_str).unwrap_or_default().to_owned();
                let mount_seq = payload.get("mountSeq").and_then(Value::as_u64).unwrap_or(0);
                self.call(WsRequestType::CanvasMount, payload, Pending::Canvas(CanvasWsPending::Mount { project_id, mount_seq }));
            }
            "canvas_unmount" => {
                self.call(WsRequestType::CanvasUnmount, payload, Pending::Canvas(CanvasWsPending::Unmount));
            }
            "canvas_reply" => {
                if payload.get("ok").and_then(Value::as_bool) == Some(true)
                    && let Some(result) = payload.get("result").and_then(Value::as_str)
                    && result.len() > REPLY_SLICE_BYTES
                {
                    let call_id = payload.get("callId").and_then(Value::as_u64).unwrap_or(0);
                    let mount_seq = payload.get("mountSeq").and_then(Value::as_u64).unwrap_or(0);
                    self.canvas_host.next_slice_id += 1;
                    let slice_id = self.canvas_host.next_slice_id;
                    let mut slice = PendingSlice { call_id, mount_seq, result: result.to_owned(), offset: 0 };
                    self.send_next_slice(slice_id, &mut slice);
                    return;
                }
                self.call(WsRequestType::CanvasReply, payload, Pending::Canvas(CanvasWsPending::Reply { slice_id: None }));
            }
            _ => {}
        }
    }

    fn send_next_slice(&mut self, slice_id: u64, slice: &mut PendingSlice) {
        let end = slice_end_utf8(&slice.result, slice.offset, REPLY_SLICE_BYTES);
        let chunk = slice.result[slice.offset..end].to_owned();
        let more = end < slice.result.len();
        slice.offset = end;
        let payload = if more {
            json!({"callId": slice.call_id, "mountSeq": slice.mount_seq, "ok": true, "result": chunk, "partial": true})
        } else {
            json!({"callId": slice.call_id, "mountSeq": slice.mount_seq, "ok": true, "result": chunk})
        };
        if more {
            self.canvas_host.slices.insert(
                slice_id,
                PendingSlice { call_id: slice.call_id, mount_seq: slice.mount_seq, result: std::mem::take(&mut slice.result), offset: slice.offset },
            );
        }
        self.call(WsRequestType::CanvasReply, payload, Pending::Canvas(CanvasWsPending::Reply { slice_id: more.then_some(slice_id) }));
    }

    fn mount_canvas_editor(&mut self, project_id: &str, info: OpenInfo) {
        let wake = self.waker.clone().unwrap_or_else(|| std::sync::Arc::new(|| {}));
        #[cfg(not(target_arch = "wasm32"))]
        {
            let handle = troupe_canvas::Handle::mount(info.mount_seq, wake);
            self.canvas_host.editors.insert(project_id.to_owned(), (info.mount_seq, handle));
        }
        #[cfg(target_arch = "wasm32")]
        {
            let name = self
                .canvases
                .session(project_id)
                .map(|s| s.doc_path.rsplit(['/', '\\']).next().unwrap_or(&s.doc_path).to_owned())
                .unwrap_or_else(|| NEW_CANVAS_DOC_NAME.to_owned());
            self.canvas_host.web.mount(project_id, &name, &info, wake);
        }
    }

    fn drop_canvas_editor(&mut self, project_id: &str) {
        #[cfg(not(target_arch = "wasm32"))]
        self.canvas_host.editors.remove(project_id);
        #[cfg(target_arch = "wasm32")]
        self.canvas_host.web.drop_editor(project_id);
    }

    fn post_to_canvas_editor(&mut self, project_id: &str, messages: &[Value]) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some((_, handle)) = self.canvas_host.editors.get(project_id) {
            for message in messages {
                handle.post(message);
            }
        }
        #[cfg(target_arch = "wasm32")]
        self.canvas_host.web.post(project_id, messages);
    }
}
