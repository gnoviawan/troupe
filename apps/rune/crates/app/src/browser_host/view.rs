//! View, input/context-menu handling, and lifecycle pumping for server-hosted browser panes.

use std::collections::BTreeSet;

use model::browser::{SessionEvent, ViewportPreset, step_session};
use model::workspace::Tab;
use rune::{Cx, El, KeyEvent, PointerPress, TextEdit, Tx, col, icon, row, text, text_input};
use serde_json::{Value, json};
use theme::Tokens;
use theme::icons as i;
use ui::browser::{
    CONTEXT_MENU, ContextAction, ContextMenuProps, KeyDisposition, PaneCallbacks, PaneSurfaceProps,
    build_context_menu, input, surface,
};
use ui::overlay::ToastKind;
use ui::type_::sans;

use super::{BrowserWsPending, now_u64};
use crate::State;
use crate::sync::Pending;

impl State {
    /// Forwards IME composition events to the focused browser pane.
    pub(crate) fn browser_ime(&mut self, ime: &rune::Ime, tx: &mut Tx) -> bool {
        let Some(focused_key) = tx.focused() else {
            return false;
        };
        let Some(tab_id) = focused_key.strip_prefix("browser-surface-").map(str::to_owned) else {
            return false;
        };
        let Some(event) = input::ime_event(ime) else {
            return false;
        };
        self.browser_send_input(&tab_id, event);
        true
    }

    /// Handles a key press or release on the browser surface.
    pub(crate) fn browser_key_event(
        &mut self,
        tab_id: &str,
        key: &KeyEvent,
        down: bool,
        tx: &mut Tx,
    ) -> KeyDisposition {
        if let Some(chord) = crate::keys::chord(key, self.shortcuts.platform)
            && let Some(sc) = model::shortcuts::find_match(&self.shortcuts, &chord)
            && input::should_bubble_shortcut(&sc.id)
        {
            return KeyDisposition::Bubble;
        }
        if !down {
            return KeyDisposition::Forward;
        }
        let agent_controlled = self
            .browser
            .session
            .tabs
            .get(tab_id)
            .and_then(|t| t.agent_controlled)
            .unwrap_or(false);
        if key.is_command('c') && !key.mods.shift && !key.mods.alt {
            self.browser_clipboard(tab_id, "copy", None);
            return KeyDisposition::Handled;
        }
        if key.is_command('x') && !key.mods.shift && !key.mods.alt && !agent_controlled {
            self.browser_clipboard(tab_id, "cut", None);
            return KeyDisposition::Handled;
        }
        if key.is_command('a') && !key.mods.shift && !key.mods.alt {
            self.browser_clipboard(tab_id, "select_all", None);
            return KeyDisposition::Handled;
        }
        if key.is_command('v') && !key.mods.shift && !key.mods.alt && !agent_controlled {
            let id = tab_id.to_owned();
            tx.read_clipboard(move |s: &mut State, content, _| {
                let text = content.text();
                if !text.is_empty() {
                    s.browser_clipboard(&id, "paste", Some(text.to_owned()));
                }
            });
            return KeyDisposition::Handled;
        }
        KeyDisposition::Forward
    }

    /// Opens the custom Rune context menu for `tab_id` at `at`.
    pub(crate) fn open_browser_menu(&mut self, tab_id: &str, at: PointerPress, tx: &mut Tx) {
        self.browser.focused = Some(tab_id.to_owned());
        self.browser.menu = Some((tab_id.to_owned(), at));
        self.overlays.open(CONTEXT_MENU);
        tx.instant();
    }

    /// Executes an item from the browser pane context menu.
    pub(crate) fn browser_context_action(&mut self, tab_id: &str, action: ContextAction, tx: &mut Tx) {
        match action {
            ContextAction::Back => self.browser_history(tab_id, "back"),
            ContextAction::Forward => self.browser_history(tab_id, "forward"),
            ContextAction::Reload => self.browser_history(tab_id, "reload"),
            ContextAction::Cut => self.browser_clipboard(tab_id, "cut", None),
            ContextAction::Copy => self.browser_clipboard(tab_id, "copy", None),
            ContextAction::SelectAll => self.browser_clipboard(tab_id, "select_all", None),
            ContextAction::Paste => {
                let id = tab_id.to_owned();
                tx.read_clipboard(move |s: &mut State, content, _| {
                    let text = content.text();
                    if !text.is_empty() {
                        s.browser_clipboard(&id, "paste", Some(text.to_owned()));
                    }
                });
            }
            ContextAction::CopyAddress => {
                if let Some(url) = self
                    .browser
                    .session
                    .tabs
                    .get(tab_id)
                    .map(|t| t.url.clone())
                    .filter(|u| !u.is_empty())
                {
                    tx.copy(url);
                    self.toasts.push(self.now, ToastKind::Info, "Page address copied", None);
                }
            }
            ContextAction::OpenExternal => {
                if let Some(url) = self
                    .browser
                    .session
                    .tabs
                    .get(tab_id)
                    .map(|t| t.url.clone())
                    .filter(|u| !u.is_empty())
                {
                    self.browser.pending_open_url.push(url);
                }
            }
        }
    }

    /// Called from `view`: collects the browser tab ids visible in the current frame.
    pub(crate) fn sync_browser_panes(&self, _cx: &Cx<State>, phone: bool) {
        *self.browser.phone_now.borrow_mut() = phone;
        let mut visible = BTreeSet::new();
        if self.workspace.record().is_some() {
            let tree = self.workspace.tree();
            let fullscreen = tree.fullscreen_pane_id.as_deref();
            for leaf in model::workspace::leaves(self.workspace.root()) {
                if fullscreen.is_some_and(|id| id != leaf.id) {
                    continue;
                }
                if phone && leaf.id != tree.active_pane_id {
                    continue;
                }
                let active = leaf
                    .tabs
                    .iter()
                    .find(|t| Some(t.id()) == leaf.active_tab_id.as_deref());
                if let Some(Tab::Browser { browser_tab_id, .. }) = active {
                    visible.insert(browser_tab_id.clone());
                }
            }
        }
        *self.browser.visible_now.borrow_mut() = visible;
    }

    /// Pumps browser pane visibility watches, closed-tab cleanup, resizes, and clipboard/URL effects.
    pub(crate) fn pump_browsers(&mut self, tx: &mut Tx) -> bool {
        let mut changed = false;

        for text in std::mem::take(&mut self.browser.pending_copy) {
            tx.copy(text);
            changed = true;
        }
        for url in std::mem::take(&mut self.browser.pending_open_url) {
            if let Err(e) = tx.open_url(&url) {
                log::warn!("[browser] open_url failed: {e}");
            }
            changed = true;
        }

        // Reconcile open workspace browser tabs: ensure entries exist and close removed tabs.
        if self.workspace.record().is_some() {
            let mut open_tabs = BTreeSet::new();
            for leaf in model::workspace::leaves(self.workspace.root()) {
                for tab in &leaf.tabs {
                    if let Tab::Browser { browser_tab_id, .. } = tab {
                        open_tabs.insert(browser_tab_id.clone());
                    }
                }
            }
            for id in &open_tabs {
                if !self.browser.panes.contains_key(id) {
                    self.ensure_browser_tab(id);
                    changed = true;
                }
            }
            // Only close host tabs for the active project when they were closed from its workspace.
            if let Some(active_project) = self.active_project().map(|p| p.id.clone()) {
                let gone: Vec<String> = self
                    .browser
                    .panes
                    .iter()
                    .filter(|(id, rt)| rt.project_id == active_project && !open_tabs.contains(*id))
                    .map(|(id, _)| id.clone())
                    .collect();
                for id in gone {
                    self.sync_browser_watch(&id, false);
                    if let Some(rt) = self.browser.panes.remove(&id)
                        && let Some(host_tab_id) = rt.host_tab_id
                    {
                        let mut payload = json!({ "tabId": host_tab_id });
                        if !rt.project_id.is_empty() {
                            payload["projectId"] = json!(rt.project_id);
                        }
                        if let Some(sid) = rt.session_id {
                            payload["sessionId"] = json!(sid);
                        }
                        self.raw_call(
                            "browser_tab_close",
                            payload,
                            Pending::Browser(BrowserWsPending::Close { host_tab_id }),
                        );
                    }
                    step_session(
                        &mut self.browser.session,
                        SessionEvent::RemoveTab(id.clone()),
                        now_u64(),
                    );
                    if self.browser.focused.as_deref() == Some(id.as_str()) {
                        self.browser.focused = None;
                    }
                    changed = true;
                }
            }
        }

        // Sync screencast watches with visibility changes.
        let visible = self.browser.visible_now.borrow().clone();
        if visible != self.browser.reported_visible {
            let prev = std::mem::replace(&mut self.browser.reported_visible, visible.clone());
            for id in prev.difference(&visible) {
                self.sync_browser_watch(id, false);
                changed = true;
            }
            for id in visible.difference(&prev) {
                self.sync_browser_watch(id, true);
                self.maybe_resize_browser(id);
                changed = true;
            }
        }

        changed
    }

    /// Tab bar title for a `Tab::Browser`.
    pub(crate) fn browser_tab_title(&self, tab: &Tab) -> Option<String> {
        let Tab::Browser { browser_tab_id, .. } = tab else {
            return None;
        };
        let label = self
            .browser
            .session
            .tabs
            .get(browser_tab_id)
            .map(model::browser::BrowserTab::display_title)
            .unwrap_or_else(|| "New Tab".to_owned());
        Some(label)
    }

    /// Renders a `Tab::Browser` body (minimal address bar + live `El::pixels` pane; PR 2 upgrades the toolbar & states).
    pub(crate) fn browser_tab_body(
        &self,
        t: &'static Tokens,
        tab: &Tab,
        phone: bool,
    ) -> Option<El> {
        let Tab::Browser { browser_tab_id, .. } = tab else {
            return None;
        };
        let c = &t.colors;
        let id = browser_tab_id.clone();
        let tab_state = self.browser.session.tabs.get(browser_tab_id);
        let rt = self.browser.panes.get(browser_tab_id);
        let agent_controlled = tab_state
            .and_then(|tb| tb.agent_controlled)
            .unwrap_or(false);
        let crashed = tab_state.is_some_and(|tb| tb.crashed);
        let touch_mode = phone
            || tab_state.is_some_and(|tb| tb.viewport_preset == ViewportPreset::Mobile);

        let url_edit = rt
            .map(|r| &r.url_edit)
            .cloned()
            .unwrap_or_else(|| TextEdit::new(""));
        let id_nav = id.clone();
        let id_edit = id.clone();
        let id_back = id.clone();
        let id_fwd = id.clone();
        let id_reload = id.clone();

        let url_field = text_input(&url_edit, sans(12.0, 400.0, 18.0))
            .single_line()
            .key(format!("browser-url-input-{id}"))
            .role("textbox")
            .label("Address bar")
            .placeholder("Enter URL or localhost port…".to_owned(), c.muted_foreground)
            .color(c.foreground)
            .grow(1.0)
            .px(8.0)
            .py(4.0)
            .radius(6.0)
            .fill(c.secondary.alpha(0.5))
            .on_submit(move |s: &mut State, _tx: &mut Tx| {
                let raw = s
                    .browser
                    .panes
                    .get(&id_nav)
                    .map(|r| r.url_edit.text.clone())
                    .unwrap_or_default();
                s.browser_navigate(&id_nav, &raw);
            })
            .on_edit(move |s: &mut State| {
                s.ensure_browser_tab(&id_edit);
                let rt = s.browser.panes.get_mut(&id_edit).expect("browser pane");
                rt.url_dirty = true;
                &mut rt.url_edit
            });

        let bar = row()
            .key(format!("browser-bar-{id}"))
            .w_full()
            .h(38.0)
            .px(8.0)
            .gap(6.0)
            .items_center()
            .fill(c.background)
            .border_sides([0.0, 0.0, 1.0, 0.0], c.border)
            .child(
                ui::shell::rail_button(t, &i::ARROW_LEFT, "Back", 26.0, 13.0, false)
                    .key(format!("browser-back-{id}"))
                    .on_press(move |s: &mut State| s.browser_history(&id_back, "back")),
            )
            .child(
                ui::shell::rail_button(t, &i::ARROW_RIGHT, "Forward", 26.0, 13.0, false)
                    .key(format!("browser-forward-{id}"))
                    .on_press(move |s: &mut State| s.browser_history(&id_fwd, "forward")),
            )
            .child(
                ui::shell::rail_button(t, &i::ROTATE_CW, "Reload", 26.0, 13.0, false)
                    .key(format!("browser-reload-{id}"))
                    .on_press(move |s: &mut State| s.browser_history(&id_reload, "reload")),
            )
            .child(url_field);

        let viewport = if !crashed
            && let Some(rt) = rt
            && (rt.host_tab_id.is_some() || rt.has_frame)
        {
            surface(
                PaneSurfaceProps {
                    tab_id: &id,
                    pixels: &rt.pixels,
                    touch_mode,
                    read_only: agent_controlled,
                    fixed_size: None,
                },
                t,
                PaneCallbacks {
                    on_focus: |s: &mut State, tid: &str| {
                        s.browser.focused = Some(tid.to_owned());
                    },
                    on_input: |s: &mut State, tid: &str, ev: Value| {
                        s.browser_send_input(tid, ev);
                    },
                    on_key: |s: &mut State, tid: &str, key: &KeyEvent, down: bool, tx: &mut Tx| {
                        s.browser_key_event(tid, key, down, tx)
                    },
                    on_context_menu: |s: &mut State, tid: &str, at: PointerPress, tx: &mut Tx| {
                        s.open_browser_menu(tid, at, tx);
                    },
                    on_layout: |s: &mut State, tid: &str, w: f32, h: f32| {
                        if let Some(r) = s.browser.panes.get_mut(tid) {
                            r.measured = (w, h);
                        }
                        s.maybe_resize_browser(tid);
                    },
                },
            )
        } else {
            let note = if crashed {
                "The browser page stopped unexpectedly. Press Reload to restore it."
            } else {
                rt.and_then(|r| r.status_note.as_deref())
                    .or_else(|| {
                        tab_state
                            .and_then(|tb| tb.load_error.as_ref())
                            .map(|e| e.message.as_str())
                    })
                    .unwrap_or("Enter a URL above to browse inside Troupe.")
            };
            col()
                .key(format!("browser-empty-{id}"))
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .center()
                .gap(8.0)
                .child(icon(&i::GLOBE, 28.0).color(c.muted_foreground))
                .child(
                    text(note.to_owned())
                        .font(sans(13.0, 400.0, 20.0))
                        .color(c.muted_foreground),
                )
        };

        Some(
            col()
                .key(format!("tab-body-{}", tab.id()))
                .w_full()
                .grow(1.0)
                .min_w(0.0)
                .min_h(0.0)
                .fill(c.background)
                .child(bar)
                .child(viewport),
        )
    }

    /// Pushes open browser pane overlays (context menu) onto the app overlay `layer`.
    pub(crate) fn browser_layer(&self, t: &'static Tokens, layer: &mut Vec<El>) {
        let Some((tab_id, at)) = self
            .browser
            .menu
            .clone()
            .filter(|_| self.overlays.is_open(CONTEXT_MENU))
        else {
            return;
        };
        let tab = self.browser.session.tabs.get(&tab_id);
        let props = ContextMenuProps {
            tab_id: &tab_id,
            at,
            can_go_back: tab.is_some_and(|tb| tb.can_go_back),
            can_go_forward: tab.is_some_and(|tb| tb.can_go_forward),
            has_url: tab.is_some_and(|tb| !tb.url.is_empty()),
            agent_controlled: tab.and_then(|tb| tb.agent_controlled).unwrap_or(false),
        };
        build_context_menu(
            props,
            t,
            layer,
            |s: &mut State, tid: &str, action: ContextAction, tx: &mut Tx| {
                s.browser_context_action(tid, action, tx);
            },
            |s: &mut State| {
                s.browser.menu = None;
                s.close(CONTEXT_MENU);
            },
        );
    }
}
