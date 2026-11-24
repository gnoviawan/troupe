//! Foundation scenes: the token lists of `design-system/src/previews/foundations.tsx`, read
//! from the bundled theme. The other foundation entries (palettes, type, space, motion, icons)
//! show a note until they are ported.

use rune::{Color, El, col, el, row, text};
use theme::Tokens;
use ui::type_::{MONO_XS, TEXT_XS};

use crate::kit::pending;
use crate::{Ctx, registry};

/// One token row: the CSS variable, the utility that consumes it, and whether the variable holds
/// a whole colour (`raw`) or `L C H` components.
type Spec = (&'static str, &'static str, bool);

const SURFACE: &[Spec] = &[
    ("background", "bg-background", false),
    ("card", "bg-card", false),
    ("popover", "bg-popover", false),
    ("secondary", "bg-secondary", false),
    ("muted", "bg-muted", false),
    ("surface-dark", "bg-surface-dark", false),
    ("surface-darker", "bg-surface-darker", false),
    ("terminal-bg", "bg-terminal-bg", false),
    ("row-hover", "bg-row-hover", true),
    ("row-hover-solid", "bg-row-hover-solid", true),
    ("rail-keycap", ".keycap on the rail", true),
];

const TEXT: &[Spec] = &[
    ("foreground", "text-foreground", false),
    ("card-foreground", "text-card-foreground", false),
    ("popover-foreground", "text-popover-foreground", false),
    ("secondary-foreground", "text-secondary-foreground", false),
    ("muted-foreground", "text-muted-foreground", false),
    ("disabled-foreground", "text-disabled-foreground", false),
    ("terminal-fg", "text-terminal-fg", false),
];

const ACCENT: &[Spec] = &[
    ("primary", "text-primary, border-primary", false),
    ("primary-fill", "bg-primary-fill", false),
    ("primary-foreground", "text-primary-foreground", false),
    ("accent", "bg-accent", false),
    ("accent-foreground", "text-accent-foreground", false),
    ("ring", "ring-ring", false),
    ("search-match", "search highlight", false),
    ("search-match-active", "active search highlight", false),
];

const STATUS: &[Spec] = &[
    ("success", "text-success", false),
    ("success-fill", "bg-success-fill", false),
    ("success-foreground", "text-success-foreground", false),
    ("warning", "text-warning", false),
    ("warning-foreground", "text-warning-foreground", false),
    ("destructive", "text-destructive", false),
    ("destructive-fill", "bg-destructive-fill", false),
    ("destructive-foreground", "text-destructive-foreground", false),
    ("connection", "text-connection", false),
];

const LINE: &[Spec] = &[
    ("border", "border-border", false),
    ("input", "border-input", false),
    ("field-border", "border-field-border", false),
    ("overlay", "bg-overlay/50", false),
];

const DIFF: &[Spec] = &[
    ("diff-added", "bg-diff-added", false),
    ("diff-added-foreground", "text-diff-added-foreground", false),
    ("diff-added-border", "border-diff-added-border", false),
    ("diff-modified", "bg-diff-modified", false),
    ("glow-blue", "shadow-glow-blue", false),
    ("glow-purple", "shadow-glow-purple", false),
    ("glow-green", "shadow-glow-green", false),
];

const SIDEBAR: &[Spec] = &[
    ("sidebar-background", "bg-sidebar", false),
    ("sidebar-foreground", "text-sidebar-foreground", false),
    ("sidebar-primary", "bg-sidebar-primary", false),
    ("sidebar-primary-foreground", "text-sidebar-primary-foreground", false),
    ("sidebar-accent", "bg-sidebar-accent", false),
    ("sidebar-accent-foreground", "text-sidebar-accent-foreground", false),
    ("sidebar-border", "border-sidebar-border", false),
    ("sidebar-ring", "ring-sidebar-ring", false),
];

const PROJECT: &[Spec] = &[
    ("project-blue", "bg-project-blue", false),
    ("project-purple", "bg-project-purple", false),
    ("project-green", "bg-project-green", false),
    ("project-yellow", "bg-project-yellow", false),
    ("project-red", "bg-project-red", false),
    ("project-cyan", "bg-project-cyan", false),
    ("project-pink", "bg-project-pink", false),
    ("project-orange", "bg-project-orange", false),
    ("project-gray", "bg-project-gray", false),
];

const STATUS_BAR: &[Spec] = &[
    ("status-bar", "bg-status-bar", false),
    ("status-bar-blue", "bg-status-bar-blue", false),
    ("status-bar-purple", "bg-status-bar-purple", false),
    ("status-bar-green", "bg-status-bar-green", false),
    ("status-bar-yellow", "bg-status-bar-yellow", false),
    ("status-bar-red", "bg-status-bar-red", false),
    ("status-bar-cyan", "bg-status-bar-cyan", false),
    ("status-bar-pink", "bg-status-bar-pink", false),
    ("status-bar-orange", "bg-status-bar-orange", false),
    ("status-bar-gray", "bg-status-bar-gray", false),
];

/// `oklch(L C H)` with the precision the web client's theme engine writes.
fn oklch(c: Color) -> String {
    let [l, a, b, _] = c.to_oklab();
    let chroma = (a * a + b * b).sqrt();
    let hue = if chroma < 0.0005 { 0.0 } else { b.atan2(a).to_degrees().rem_euclid(360.0) };
    let trim = |v: f32, places: usize| {
        let s = format!("{v:.places$}");
        let s = s.trim_end_matches('0').trim_end_matches('.');
        if s.is_empty() || s == "-0" { "0".to_owned() } else { s.to_owned() }
    };
    format!("oklch({} {} {})", trim(l, 3), trim(chroma, 3), trim(hue, 1))
}

fn token_row(t: &Tokens, (name, utility, raw): Spec, first: bool) -> El {
    let k = &t.colors;
    let color = *k.get(name).unwrap_or_else(|| panic!("theme token {name}"));
    let hex = color.hex()[..7].to_owned();
    let e = row()
        .w_full()
        .items_center()
        .gap(12.0)
        .py(8.0)
        .child(el().w(36.0).h(36.0).radius(6.0).fill(color).border(1.0, k.border).a11y_hidden())
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .child(text(format!("--{name}")).font(MONO_XS).color(k.foreground).truncate())
                .child(text(utility).font(TEXT_XS).color(k.muted_foreground).truncate()),
        )
        .child(
            col()
                .items_end()
                .child(text(if raw { hex.clone() } else { oklch(color) }).font(MONO_XS).color(k.muted_foreground))
                .child(text(if raw { String::new() } else { hex }).font(MONO_XS).color(k.muted_foreground)),
        );
    if first {
        e
    } else {
        e.pt(9.0).border_sides([1.0, 0.0, 0.0, 0.0], k.border.alpha(0.6))
    }
}

fn tokens(c: &Ctx, list: &[Spec]) -> El {
    col()
        .role("list")
        .w_full()
        .px(20.0)
        .py(8.0)
        .children(list.iter().enumerate().map(|(n, spec)| token_row(c.t, *spec, n == 0)))
}

pub fn surface(c: &Ctx) -> El {
    tokens(c, SURFACE)
}
pub fn text_tokens(c: &Ctx) -> El {
    tokens(c, TEXT)
}
pub fn accent(c: &Ctx) -> El {
    tokens(c, ACCENT)
}
pub fn status(c: &Ctx) -> El {
    tokens(c, STATUS)
}
pub fn line(c: &Ctx) -> El {
    tokens(c, LINE)
}
pub fn diff(c: &Ctx) -> El {
    tokens(c, DIFF)
}
pub fn sidebar(c: &Ctx) -> El {
    tokens(c, SIDEBAR)
}
pub fn project(c: &Ctx) -> El {
    tokens(c, PROJECT)
}
pub fn status_bar(c: &Ctx) -> El {
    tokens(c, STATUS_BAR)
}

fn not_yet(c: &Ctx) -> El {
    let name = registry::find(c.s.scene).map_or("", |s| s.name);
    pending(c.t, name, "R4b follow-up", "This foundation specimen is not ported yet; its tokens are in the theme crate.")
}

pub fn brand(c: &Ctx) -> El {
    not_yet(c)
}
pub fn palettes(c: &Ctx) -> El {
    not_yet(c)
}
pub fn syntax(c: &Ctx) -> El {
    not_yet(c)
}
pub fn type_scale(c: &Ctx) -> El {
    not_yet(c)
}
pub fn families(c: &Ctx) -> El {
    not_yet(c)
}
pub fn labels(c: &Ctx) -> El {
    not_yet(c)
}
pub fn spacing(c: &Ctx) -> El {
    not_yet(c)
}
pub fn radius(c: &Ctx) -> El {
    not_yet(c)
}
pub fn shadow(c: &Ctx) -> El {
    not_yet(c)
}
pub fn elevation(c: &Ctx) -> El {
    not_yet(c)
}
pub fn durations(c: &Ctx) -> El {
    not_yet(c)
}
pub fn easings(c: &Ctx) -> El {
    not_yet(c)
}
pub fn motion_tokens(c: &Ctx) -> El {
    not_yet(c)
}
pub fn icons(c: &Ctx) -> El {
    not_yet(c)
}
