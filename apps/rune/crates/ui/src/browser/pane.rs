//! The interactive screencast viewport (`El::pixels`) for a hosted Chromium tab.
//! Forwards pointer, drag, wheel, key, key-up, secondary-press, and touch events
//! back to the host via callbacks.

use rune::{
    Drag, El, KeyEvent, Phase, Pixels, PointerButton, PointerEvent, PointerPress, Tx, WheelEvent,
    el,
};
use serde_json::Value;
use theme::Tokens;

use super::input::{
    cdp_modifiers, key_event, pointer_button, pointer_event, touch_event, wheel_event,
};

/// Element key for a browser pane's interactive pixel surface.
#[must_use]
pub fn surface_key(tab_id: &str) -> String {
    format!("browser-surface-{tab_id}")
}

/// How the app wants a key press on the browser surface handled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyDisposition {
    /// Already handled by the host (e.g. clipboard, find, zoom); stop propagation.
    Handled,
    /// Forward to Chromium via `browser_tab_input` and stop propagation.
    Forward,
    /// Do not forward to Chromium; let the event bubble to `AppSpec::on_key`.
    Bubble,
}

/// Props describing one rendered browser viewport.
pub struct PaneSurfaceProps<'a> {
    pub tab_id: &'a str,
    pub pixels: &'a Pixels,
    /// When true, drags emit CDP touch events instead of mouse events (phone shell / mobile preset).
    pub touch_mode: bool,
    /// When true, interactive input is suppressed (e.g. read-only agent-controlled tab).
    pub read_only: bool,
    /// Optional explicit width/height in points (responsive viewport mode).
    pub fixed_size: Option<(f32, f32)>,
}

/// Host callbacks invoked by the viewport element.
pub struct PaneCallbacks<S> {
    /// Focuses the pane on pointer interaction.
    pub on_focus: fn(&mut S, &str),
    /// Sends one `PaneInput` JSON payload to `browser_tab_input`.
    pub on_input: fn(&mut S, &str, Value),
    /// Decides whether a key event is handled locally, forwarded to Chromium, or bubbled.
    pub on_key: fn(&mut S, &str, &KeyEvent, bool, &mut Tx) -> KeyDisposition,
    /// Opens the custom Rune context menu on right-click / secondary press.
    pub on_context_menu: fn(&mut S, &str, PointerPress, &mut Tx),
    /// Reports the laid-out viewport dimensions `[width, height]` in points.
    pub on_layout: fn(&mut S, &str, f32, f32),
}

/// Renders the `El::pixels` viewport with full input forwarding and IME cursor anchor.
pub fn surface<S: 'static>(
    props: PaneSurfaceProps<'_>,
    t: &'static Tokens,
    cb: PaneCallbacks<S>,
) -> El {
    let key = surface_key(props.tab_id);
    let tab_move = props.tab_id.to_owned();
    let tab_drag = props.tab_id.to_owned();
    let tab_wheel = props.tab_id.to_owned();
    let tab_key = props.tab_id.to_owned();
    let tab_key_up = props.tab_id.to_owned();
    let tab_ctx = props.tab_id.to_owned();
    let tab_layout = props.tab_id.to_owned();
    let focus_drag = key.clone();
    let focus_ctx = key.clone();
    let touch_mode = props.touch_mode;
    let read_only = props.read_only;

    let mut view = el()
        .key(key)
        .role("region")
        .label("Browser viewport")
        .pixels(props.pixels)
        .fill(t.colors.background)
        .focusable(true)
        .pointer_buttons([
            PointerButton::Middle,
            PointerButton::Back,
            PointerButton::Forward,
        ]);

    view = match props.fixed_size {
        Some((w, h)) => view.size(w.max(1.0), h.max(1.0)),
        None => view.w_full().grow(1.0).min_w(0.0).min_h(0.0),
    };

    let on_focus = cb.on_focus;
    let on_input = cb.on_input;
    let on_key = cb.on_key;
    let on_context_menu = cb.on_context_menu;
    let on_layout = cb.on_layout;

    view.on_layout_tx(move |s: &mut S, [_, _, w, h], _tx: &mut Tx| {
        on_layout(s, &tab_layout, w, h);
    })
    .on_pointer_move_local(move |s: &mut S, e: PointerEvent, tx: &mut Tx| {
        if read_only || touch_mode {
            return;
        }
        let mods = cdp_modifiers(tx.mods());
        on_input(
            s,
            &tab_move,
            pointer_event("move", e.x, e.y, "none", 0, 0, mods),
        );
    })
    .on_drag(move |s: &mut S, d: &mut Drag, tx: &mut Tx| {
        if d.phase == Phase::Down {
            tx.focus(&focus_drag, false);
            on_focus(s, &tab_drag);
        }
        if read_only {
            return;
        }
        let mods = cdp_modifiers(tx.mods());
        if touch_mode {
            let action = match d.phase {
                Phase::Down => "start",
                Phase::Move => "move",
                Phase::Up => {
                    if d.cancelled() {
                        "cancel"
                    } else {
                        "end"
                    }
                }
            };
            on_input(s, &tab_drag, touch_event(action, d.x, d.y, 1, mods));
            return;
        }
        let (btn_name, btn_mask) = pointer_button(tx.button());
        let ev = match d.phase {
            Phase::Down => {
                pointer_event("down", d.x, d.y, btn_name, btn_mask, d.clicks.max(1), mods)
            }
            Phase::Move => pointer_event("move", d.x, d.y, btn_name, btn_mask, 0, mods),
            Phase::Up => pointer_event("up", d.x, d.y, btn_name, 0, d.clicks.max(1), mods),
        };
        on_input(s, &tab_drag, ev);
    })
    .on_secondary_press(move |s: &mut S, at: PointerPress, tx: &mut Tx| {
        tx.focus(&focus_ctx, false);
        on_focus(s, &tab_ctx);
        on_context_menu(s, &tab_ctx, at, tx);
    })
    .on_wheel(move |s: &mut S, e: WheelEvent, tx: &mut Tx| {
        if read_only {
            return false;
        }
        let mods = cdp_modifiers(tx.mods());
        on_input(s, &tab_wheel, wheel_event(&e, mods));
        true
    })
    .on_key(move |s: &mut S, key: &KeyEvent, tx: &mut Tx| {
        match on_key(s, &tab_key, key, true, tx) {
            KeyDisposition::Handled => true,
            KeyDisposition::Bubble => false,
            KeyDisposition::Forward => {
                if read_only {
                    return false;
                }
                if let Some(ev) = key_event(true, key) {
                    on_input(s, &tab_key, ev);
                    true
                } else {
                    false
                }
            }
        }
    })
    .on_key_up(move |s: &mut S, key: &KeyEvent, tx: &mut Tx| {
        match on_key(s, &tab_key_up, key, false, tx) {
            KeyDisposition::Handled => true,
            KeyDisposition::Bubble => false,
            KeyDisposition::Forward => {
                if read_only {
                    return false;
                }
                if let Some(ev) = key_event(false, key) {
                    on_input(s, &tab_key_up, ev);
                    true
                } else {
                    false
                }
            }
        }
    })
    .child(
        el().key(format!("browser-ime-caret-{}", props.tab_id))
            .absolute(24.0, 24.0)
            .size(1.0, 20.0)
            .ime_cursor()
            .a11y_hidden(),
    )
}

/// Overlay key for the browser pane context menu.
pub const CONTEXT_MENU: &str = "browser-context-menu";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextAction {
    Back,
    Forward,
    Reload,
    Cut,
    Copy,
    Paste,
    SelectAll,
    CopyAddress,
    OpenExternal,
}

pub struct ContextMenuProps<'a> {
    pub tab_id: &'a str,
    pub at: PointerPress,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub has_url: bool,
    pub agent_controlled: bool,
}

fn shortcut_label(letter: char) -> String {
    let mod_key = if rune::KeyPlatform::current() == rune::KeyPlatform::Mac {
        "⌘"
    } else {
        "Ctrl"
    };
    format!("{mod_key}+{letter}")
}

/// Pushes the browser pane's custom Rune context menu onto `layer`.
pub fn build_context_menu<S: 'static>(
    props: ContextMenuProps<'_>,
    t: &'static Tokens,
    layer: &mut Vec<El>,
    on_action: fn(&mut S, &str, ContextAction, &mut Tx),
    on_close: fn(&mut S),
) {
    use crate::overlay::{Menu, MenuItem};
    use theme::icons as i;

    let tab_id = props.tab_id.to_owned();
    let item = |label: &str, action: ContextAction| {
        let id = tab_id.clone();
        MenuItem::new(label.to_owned()).on_select_tx(move |s: &mut S, tx: &mut Tx| {
            on_close(s);
            on_action(s, &id, action, tx);
            tx.instant();
        })
    };
    let editable = !props.agent_controlled;
    let menu = Menu::new(CONTEXT_MENU)
        .width(216.0)
        .at(Some(props.at))
        .item(
            item("Back", ContextAction::Back)
                .icon(&i::ARROW_LEFT)
                .disabled(!props.can_go_back || !editable),
        )
        .item(
            item("Forward", ContextAction::Forward)
                .icon(&i::ARROW_RIGHT)
                .disabled(!props.can_go_forward || !editable),
        )
        .item(
            item("Reload", ContextAction::Reload)
                .icon(&i::ROTATE_CW)
                .disabled(!props.has_url || !editable),
        )
        .separator()
        .item(
            item("Cut", ContextAction::Cut)
                .shortcut(shortcut_label('X'))
                .disabled(!editable),
        )
        .item(item("Copy", ContextAction::Copy).shortcut(shortcut_label('C')))
        .item(
            item("Paste", ContextAction::Paste)
                .shortcut(shortcut_label('V'))
                .disabled(!editable),
        )
        .item(item("Select All", ContextAction::SelectAll).shortcut(shortcut_label('A')))
        .separator()
        .item(
            item("Copy page address", ContextAction::CopyAddress)
                .icon(&i::COPY)
                .disabled(!props.has_url),
        )
        .item(
            item("Open in default browser", ContextAction::OpenExternal)
                .icon(&i::EXTERNAL_LINK)
                .disabled(!props.has_url),
        )
        .on_close(move |s: &mut S| on_close(s));
    let _ = menu.build(t, layer, rune::el());
}
