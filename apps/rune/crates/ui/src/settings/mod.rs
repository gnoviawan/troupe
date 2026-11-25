//! The Settings window's parts (`components/settings/`): the page header, a titled group, the
//! raised card its rows sit in, a row with its label, help and control, the switch row, the
//! stepper and the notice. The window shell and the pages are the app's; these hold the look.

pub mod pages;

use rune::{El, col, el, icon, row, text};
use theme::Tokens;

use crate::Switch;
use crate::paint::shadows;
use crate::type_::sans;

/// The page column's widest (`SETTINGS_COLUMN_CLASS`, `max-w-[640px]`).
pub const COLUMN: f32 = 640.0;
/// The rail's width at `md` and up (`md:w-60`).
pub const RAIL: f32 = 240.0;

/// A page's title and its one line of help (`text-xl` over `text-sm`, 32 px above the groups).
pub fn header(t: &Tokens, title: &str, description: &str) -> El {
    let c = &t.colors;
    col()
        .key("settings-page-header")
        .w_full()
        .gap(6.0)
        .pb(32.0)
        .child(text(title).font(sans(20.0, 500.0, 28.0)).color(c.foreground).role("heading"))
        .child(text(description).font(sans(14.0, 400.0, 20.0)).color(c.muted_foreground).wrap())
}

/// `SettingsGroup`: a 14/20 heading, its actions at the right, then 12 px, then `content`.
pub fn group(t: &Tokens, key: &str, title: &str, actions: Vec<El>, content: Vec<El>) -> El {
    let c = &t.colors;
    let mut head = row().key("head").w_full().min_h(28.0).items_center().justify_between().gap(12.0);
    if !title.is_empty() {
        head = head.child(text(title).font(sans(14.0, 500.0, 20.0)).color(c.foreground).role("heading"));
    }
    if !actions.is_empty() {
        head = head.child(row().key("actions").gap(8.0).items_center().children(actions));
    }
    col()
        .key(key.to_owned())
        .role("group")
        .label(title.to_owned())
        .w_full()
        .gap(12.0)
        .items_stretch()
        .child(head)
        .children(content)
}

/// `SettingsCard`: the card fill, a 0.5 px edge and the control drop, radius 8, its rows split by
/// hairlines.
pub fn card(t: &Tokens, key: &str, rows: Vec<El>) -> El {
    let c = &t.colors;
    let mut body = col()
        .key(key.to_owned())
        .w_full()
        .radius(8.0)
        .fill(c.card)
        .border(1.0, c.border)
        .clip_rounded(8.0)
        .items_stretch();
    for (i, r) in rows.into_iter().enumerate() {
        if i > 0 {
            body = body.child(el().key(format!("rule{i}")).h(0.5).w_full().fill(c.border).a11y_hidden());
        }
        body = body.child(r);
    }
    shadows(body, &t.shadows.shadow_control)
}

/// `SettingsRow`: the label and help on the left, the control on the right.
pub fn setting(t: &Tokens, key: &str, label: &str, description: &str, control: Option<El>) -> El {
    let c = &t.colors;
    let mut words = col()
        .key("words")
        .grow(1.0)
        .min_w(0.0)
        .gap(2.0)
        .child(text(label).font(sans(12.0, 500.0, 16.0)).color(c.foreground));
    if !description.is_empty() {
        words = words.child(text(description).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground).wrap());
    }
    let mut r = row()
        .key(key.to_owned())
        .role("group")
        .label(label.to_owned())
        .w_full()
        .px(16.0)
        .py(12.0)
        .gap(24.0)
        .items_center()
        .child(words);
    if let Some(control) = control {
        r = r.child(row().key("control").gap(8.0).items_center().child(control));
    }
    r
}

/// `SettingsSwitchRow`: a setting whose control is a switch named by its label. The caller adds
/// `on_press` to the switch through `wire`.
pub fn switch_row(t: &Tokens, key: &str, label: &str, description: &str, checked: bool, wire: impl FnOnce(El) -> El) -> El {
    let toggle = wire(Switch::new(checked).label(label).build(t).key(format!("{key}-switch")));
    setting(t, key, label, description, Some(toggle))
}

/// A stepper's parts: the value as shown and whether each button is at its limit.
pub struct Stepper<'a> {
    pub key: &'a str,
    pub label: &'a str,
    pub value: String,
    pub decrement: &'a str,
    pub increment: &'a str,
    pub at_min: bool,
    pub at_max: bool,
    /// The reset button's text, shown when the value is not the reset value.
    pub reset: Option<&'a str>,
}

/// `SettingsStepper`: −, value, + on the field surface, 32 px tall, then an optional Reset. The
/// minus is keyed `{key}-dec`, the plus `{key}-inc`, the reset `{key}-reset`; the caller adds
/// their handlers through `wire(key, el)`.
pub fn stepper(t: &Tokens, s: Stepper<'_>, wire: impl Fn(&str, El) -> El) -> El {
    let c = &t.colors;
    let step = |glyph: &str, label: &str, key: String, disabled: bool| {
        let g = theme::icons::named(glyph).expect("bundled icon");
        let mut b = row()
            .key(key.clone())
            .role("button")
            .label(label.to_owned())
            .w(32.0)
            .h(30.0)
            .items_center()
            .justify_center()
            .child(icon(g, 14.0).color(c.muted_foreground).a11y_hidden())
            .disabled(disabled);
        if disabled {
            b = b.opacity(0.35);
        } else {
            b = wire(&key, b.focusable(true).press(c.foreground.alpha(0.06)));
        }
        b
    };
    let field = row()
        .key(format!("{}-field", s.key))
        .role("group")
        .label(s.label.to_owned())
        .h(32.0)
        .items_center()
        .radius(8.0)
        .fill(c.card)
        .border(1.0, c.border.alpha(0.6))
        .child(step("Minus", s.decrement, format!("{}-dec", s.key), s.at_min))
        .child(
            row()
                .key("value")
                .h_full()
                .min_w(64.0)
                .px(8.0)
                .items_center()
                .justify_center()
                .border_sides([0.0, 1.0, 0.0, 1.0], c.border)
                .child(text(s.value).font(sans(12.0, 500.0, 16.0)).color(c.foreground)),
        )
        .child(step("Plus", s.increment, format!("{}-inc", s.key), s.at_max));
    let mut out = row().key(s.key.to_owned()).gap(8.0).items_center().child(field);
    if let Some(reset) = s.reset {
        let b = row()
            .key(format!("{}-reset", s.key))
            .role("button")
            .label(reset.to_owned())
            .h(28.0)
            .px(8.0)
            .radius(6.0)
            .items_center()
            .focusable(true)
            .hover(c.secondary)
            .child(text(reset).font(sans(12.0, 500.0, 16.0)).color(c.secondary_foreground));
        out = out.child(wire(&format!("{}-reset", s.key), b));
    }
    out
}

/// `SettingsNotice` (info): a bordered note with its icon.
pub fn notice(t: &Tokens, key: &str, body: &str) -> El {
    let c = &t.colors;
    let info = theme::icons::named("Info").expect("bundled icon");
    row()
        .key(key.to_owned())
        .w_full()
        .gap(10.0)
        .px(12.0)
        .py(10.0)
        .radius(8.0)
        .border(1.0, c.border)
        .items_start()
        .child(icon(info, 14.0).color(c.muted_foreground).a11y_hidden())
        .child(
            text(body)
                .font(sans(12.0, 400.0, 19.5))
                .color(c.secondary_foreground)
                .wrap()
                .grow(1.0)
                .min_w(0.0),
        )
}
