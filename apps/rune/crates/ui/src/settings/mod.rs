//! The Settings window's parts (`components/settings/`): the page header, a titled group, the
//! raised card its rows sit in, a row with its label, help and control, the switch row, the
//! stepper, the notice, the empty state, the filter well and the segmented switch. The window
//! shell and the pages are the app's; these hold the look.

mod group;
pub mod pages;

pub use group::{COLUMN, Heading, Layout, SPLIT_FROM, after_lead, column_width, described, group, group_with_meta, page, section, split, split_heading, split_row};

use rune::{El, TextEdit, col, el, icon, row, text, text_input};
use theme::Tokens;

use crate::Switch;
use crate::paint::shadows;
use crate::type_::sans;

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

/// The settings column width below which a row puts its control under its label (`35rem`).
const STACK_BELOW: f32 = 560.0;

/// `SettingsRow`: the label and help on the left, the control on the right. In a settings
/// column narrower than 560 px the control goes on its own line under them.
pub fn setting(t: &Tokens, key: &str, label: &str, description: &str, control: Option<El>) -> El {
    let width = column_width();
    match control {
        Some(control) if width > 0.0 && width < STACK_BELOW => setting_stacked(t, key, label, description, control),
        control => inline_setting(t, key, label, description, control),
    }
}

/// `SettingsRow` `layout="inline"`: the control stays on the label's line at any width.
fn inline_setting(t: &Tokens, key: &str, label: &str, description: &str, control: Option<El>) -> El {
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

/// `SettingsRow` in a narrow column (below 35rem): the label and help, then the control on its
/// own full-width line, 10 px under.
pub fn setting_stacked(t: &Tokens, key: &str, label: &str, description: &str, control: El) -> El {
    let c = &t.colors;
    let mut words = col().key("words").w_full().gap(2.0).child(text(label).font(sans(12.0, 500.0, 16.0)).color(c.foreground));
    if !description.is_empty() {
        words = words.child(text(description).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground).wrap());
    }
    col()
        .key(key.to_owned())
        .role("group")
        .label(label.to_owned())
        .w_full()
        .px(16.0)
        .py(12.0)
        .gap(10.0)
        .items_stretch()
        .child(words)
        .child(row().key("control").w_full().gap(8.0).items_center().child(control))
}

/// `SettingsSwitchRow`: a setting whose control is a switch named by its label. The caller adds
/// `on_press` to the switch through `wire`.
pub fn switch_row(t: &Tokens, key: &str, label: &str, description: &str, checked: bool, wire: impl FnOnce(El) -> El) -> El {
    let toggle = wire(Switch::new(checked).label(label).build(t).key(format!("{key}-switch")));
    inline_setting(t, key, label, description, Some(toggle))
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
    toned_notice(t, key, Tone::Info, body)
}

/// A notice's tone (`NoticeTone`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Info,
    Warning,
    Danger,
}

/// `SettingsNotice` in `tone`: the tone's edge, wash and glyph.
pub fn toned_notice(t: &Tokens, key: &str, tone: Tone, body: &str) -> El {
    let c = &t.colors;
    let (glyph, ink, edge, wash) = match tone {
        Tone::Info => ("Info", c.muted_foreground, c.border, c.foreground.alpha(0.03)),
        Tone::Warning => ("AlertTriangle", c.warning, c.warning.alpha(0.2), c.warning.alpha(0.1)),
        Tone::Danger => ("AlertCircle", c.destructive, c.destructive.alpha(0.2), c.destructive.alpha(0.1)),
    };
    let g = theme::icons::named(glyph).expect("bundled icon");
    row()
        .key(key.to_owned())
        .role(if tone == Tone::Danger { "alert" } else { "status" })
        .w_full()
        .gap(10.0)
        .px(12.0)
        .py(10.0)
        .radius(8.0)
        .border(1.0, edge)
        .fill(wash)
        .items_start()
        .child(icon(g, 14.0).color(ink).mt(2.0).a11y_hidden())
        .child(
            text(body)
                .font(sans(12.0, 400.0, 19.5))
                .color(c.secondary_foreground)
                .wrap()
                .grow(1.0)
                .min_w(0.0),
        )
}

/// `SettingsEmptyState`: an icon, what goes here, one line on what it is for, and the button
/// that adds the first item.
pub fn empty_state(t: &Tokens, key: &str, glyph: &str, title: &str, description: &str, action: Option<El>) -> El {
    let c = &t.colors;
    let g = theme::icons::named(glyph).expect("bundled icon");
    let mut out = col()
        .key(key.to_owned())
        .w_full()
        .gap(6.0)
        .px(24.0)
        .py(32.0)
        .items_center()
        .radius(8.0)
        .fill(c.card)
        .border(1.0, c.border)
        .child(row().key("glyph").mb(4.0).a11y_hidden().child(icon(g, 20.0).color(c.muted_foreground)))
        .child(text(title).font(sans(14.0, 500.0, 20.0)).color(c.foreground))
        .child(text(description).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground).wrap().max_w(384.0).align(rune::Align::Center));
    if let Some(action) = action {
        out = out.child(row().key("action").pt(8.0).gap(8.0).child(action));
    }
    out
}

/// `SettingsFilter`: the 32 px sunk well with a search glyph, and a clear button once it holds
/// text. `wire_input` adds the field's `on_edit`; `wire_clear` the clear button's press.
#[allow(clippy::too_many_arguments)]
pub fn filter(
    t: &Tokens,
    key: &str,
    edit: &TextEdit,
    label: &str,
    placeholder: &str,
    coarse: bool,
    wire_input: impl FnOnce(El) -> El,
    wire_clear: impl FnOnce(El) -> El,
) -> El {
    let c = &t.colors;
    let font = if coarse { sans(16.0, 500.0, 24.0) } else { sans(12.0, 500.0, 16.0) };
    let field = text_input(edit, font)
        .single_line()
        .key("input")
        .label(label.to_owned())
        .placeholder(placeholder.to_owned(), c.muted_foreground)
        .color(c.foreground)
        .w_full()
        .h(if coarse { 44.0 } else { 32.0 })
        .pl(32.0)
        .pr(if coarse { 44.0 } else { 32.0 })
        .radius(6.0)
        .fill(c.muted)
        .focus_ring(2.0, -1.0, c.focus_ring);
    let search = theme::icons::named("Search").expect("bundled icon");
    let mut out = row()
        .key(key.to_owned())
        .min_w(0.0)
        .child(wire_input(field))
        .child(icon(search, 14.0).color(c.muted_foreground).pin(Some(10.0), None, Some(if coarse { 15.0 } else { 9.0 }), None).a11y_hidden());
    if !edit.text.is_empty() {
        let x = theme::icons::named("X").expect("bundled icon");
        let side = if coarse { 44.0 } else { 24.0 };
        let clear = row()
            .key("clear")
            .role("button")
            .label("Clear text")
            .size(side, side)
            .center()
            .radius(6.0)
            .hover(c.foreground.alpha(0.04))
            .child(icon(x, 13.0).color(c.muted_foreground))
            .pin(None, Some(if coarse { 0.0 } else { 4.0 }), Some(if coarse { 0.0 } else { 4.0 }), None);
        out = out.child(wire_clear(clear));
    }
    out
}

/// `SettingsSegmented`: a radio group of options on the sunk well, the chosen one raised.
/// `options` are (label, count); `wire(i, el)` adds each option's press.
pub fn segmented(t: &Tokens, key: &str, label: &str, options: &[(&str, Option<usize>)], chosen: usize, toolbar: bool, wire: impl Fn(usize, El) -> El) -> El {
    let c = &t.colors;
    let mut out = row()
        .key(key.to_owned())
        .role("radiogroup")
        .label(label.to_owned())
        .h(if toolbar { 32.0 } else { 28.0 })
        .padding(2.0)
        .gap(2.0)
        .items_center()
        .radius(6.0)
        .fill(c.background)
        .border(0.5, c.border);
    for (i, (name, count)) in options.iter().enumerate() {
        let on = i == chosen;
        let mut words = row().gap(6.0).items_center().child(text(*name).font(sans(12.0, 500.0, 16.0)).color(if on { c.foreground } else { c.muted_foreground }));
        if let Some(n) = count {
            words = words.child(text(n.to_string()).font(sans(12.0, 500.0, 16.0)).color(c.muted_foreground));
        }
        let name_label = match count {
            Some(n) => format!("{name} {n}"),
            None => (*name).to_owned(),
        };
        let mut b = row()
            .key(format!("option-{i}"))
            .role("radio")
            .label(name_label)
            .checked(on)
            .h_full()
            .px(10.0)
            .items_center()
            .radius(4.0)
            .focusable(true)
            .child(words);
        if on {
            b = shadows(b.fill(c.muted), &t.shadows.shadow_control);
        }
        out = out.child(wire(i, b));
    }
    out
}

/// `StatusWord`'s tone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusTone {
    Success,
    Warning,
    Danger,
    Muted,
}

/// `StatusWord`: a 6 px dot (a spinner while `pending`) and the word, 12 px.
pub fn status_word(t: &Tokens, key: &str, tone: StatusTone, label: &str, pending: bool) -> El {
    let c = &t.colors;
    let (dot, ink) = match tone {
        StatusTone::Success => (c.success, c.muted_foreground),
        StatusTone::Warning => (c.warning, c.warning),
        StatusTone::Danger => (c.destructive, c.destructive),
        StatusTone::Muted => (c.muted_foreground.alpha(0.6), c.muted_foreground),
    };
    let mark = if pending {
        crate::Spinner::new().size(12.0).decorative().build(ink, false).key("spinner")
    } else {
        el().key("dot").size(6.0, 6.0).radius(3.0).fill(dot).a11y_hidden()
    };
    row()
        .key(key.to_owned())
        .role("status")
        .label(label.to_owned())
        .gap(6.0)
        .items_center()
        .child(mark)
        .child(text(label).font(sans(12.0, 400.0, 16.0)).color(ink).single_line())
}

/// `McpCatalogIcon`'s placeholder: the name's first letter on a 32 or 40 px tile.
pub fn letter_tile(t: &Tokens, name: &str, side: f32) -> El {
    let c = &t.colors;
    let letter = name.trim().chars().next().map_or_else(|| "?".to_owned(), |ch| ch.to_uppercase().collect());
    let size = if side >= 40.0 { 16.0 } else { 14.0 };
    row()
        .key("tile")
        .size(side, side)
        .center()
        .radius(6.0)
        .fill(c.foreground.alpha(0.06))
        .a11y_hidden()
        .child(text(letter).font(sans(size, 500.0, size + 8.0)).color(c.secondary_foreground))
}
