//! The GitHub area's parts (`components/github/github-status.tsx`, `GithubListRow`,
//! `GithubChoiceMenu`'s chip): the state and check looks, label chips with contrast-checked
//! ink, initials avatars, the filter chip and the list row. The app composes them into the
//! panel and the item tab and attaches the handlers.

use model::github::{CheckRollup, CheckStatus, ItemKind, ItemState, Label, ListItem};
use rune::{Color, El, col, el, icon, row, text};
use theme::Tokens;

use crate::git::glyph;
use crate::type_::sans;

/// The ink family of a state or check look (`docs/design/status.md`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Success,
    Destructive,
    Warning,
    Muted,
    Foreground,
}

impl Tone {
    pub fn ink(self, t: &Tokens) -> Color {
        let c = &t.colors;
        match self {
            Tone::Success => c.success,
            Tone::Destructive => c.destructive,
            Tone::Warning => c.warning,
            Tone::Muted => c.muted_foreground,
            Tone::Foreground => c.foreground,
        }
    }

    /// The pill's wash: 20% of the ink, `foreground/10` for neutral, `muted` for muted.
    fn pill(self, t: &Tokens) -> Color {
        match self {
            Tone::Success | Tone::Destructive => self.ink(t).alpha(0.2),
            Tone::Foreground => t.colors.foreground.alpha(0.1),
            Tone::Muted | Tone::Warning => t.colors.muted,
        }
    }
}

/// `ITEM_STATE_LOOK`: the state word, its icon and its tone, per kind.
pub fn item_look(kind: ItemKind, state: ItemState) -> (&'static str, &'static str, Tone) {
    match (kind, state) {
        (ItemKind::Issue, ItemState::Open | ItemState::Draft) => ("Open", "CircleDot", Tone::Success),
        (ItemKind::Issue, _) => ("Closed", "CircleCheck", Tone::Muted),
        (ItemKind::Pr, ItemState::Open) => ("Open", "GitPullRequest", Tone::Success),
        (ItemKind::Pr, ItemState::Draft) => ("Draft", "GitPullRequestDraft", Tone::Muted),
        (ItemKind::Pr, ItemState::Merged) => ("Merged", "GitMerge", Tone::Foreground),
        (ItemKind::Pr, ItemState::Closed) => ("Closed", "GitPullRequestClosed", Tone::Destructive),
    }
}

pub fn kind_noun(kind: ItemKind) -> &'static str {
    match kind {
        ItemKind::Issue => "Issue",
        ItemKind::Pr => "Pull request",
    }
}

/// `ItemStateIcon`: its name says kind and state ("Open pull request").
pub fn item_state_icon(t: &Tokens, kind: ItemKind, state: ItemState, size: f32) -> El {
    let (label, name, tone) = item_look(kind, state);
    el().role("img")
        .label(format!("{label} {}", kind_noun(kind).to_lowercase()))
        .child(icon(glyph(name), size).color(tone.ink(t)))
}

/// `ItemStatePill`, with `label` replacing the state word ("Closed as not planned").
pub fn item_state_pill(t: &Tokens, kind: ItemKind, state: ItemState, label: Option<&str>) -> El {
    let (word, name, tone) = item_look(kind, state);
    let ink = if tone == Tone::Muted && state == ItemState::Closed { t.colors.foreground } else { tone.ink(t) };
    let fill = if kind == ItemKind::Issue && state != ItemState::Open && state != ItemState::Draft {
        Tone::Foreground.pill(t)
    } else {
        tone.pill(t)
    };
    row()
        .h(24.0)
        .px(8.0)
        .gap(4.0)
        .items_center()
        .radius(12.0)
        .fill(fill)
        .child(icon(glyph(name), 14.0).color(ink))
        .child(text(label.unwrap_or(word).to_owned()).font(sans(12.0, 500.0, 16.0)).color(ink).single_line())
}

/// `CHECK_LOOK`: the status word, its icon and its tone.
pub fn check_look(status: CheckStatus) -> (&'static str, &'static str, Tone) {
    match status {
        CheckStatus::Success => ("Passed", "CheckCircle2", Tone::Success),
        CheckStatus::Failure => ("Failed", "XCircle", Tone::Destructive),
        CheckStatus::Pending => ("Pending", "CircleDashed", Tone::Warning),
        CheckStatus::Skipped => ("Skipped", "CircleMinus", Tone::Muted),
        CheckStatus::Neutral => ("Neutral", "CircleMinus", Tone::Muted),
        CheckStatus::Cancelled => ("Cancelled", "CircleMinus", Tone::Muted),
    }
}

/// `ROLLUP_LABEL`.
pub fn rollup_label(rollup: CheckRollup) -> &'static str {
    match rollup {
        CheckRollup::Success => "Checks passed",
        CheckRollup::Failure => "Checks failed",
        CheckRollup::Pending => "Checks running",
    }
}

pub fn rollup_status(rollup: CheckRollup) -> CheckStatus {
    match rollup {
        CheckRollup::Success => CheckStatus::Success,
        CheckRollup::Failure => CheckStatus::Failure,
        CheckRollup::Pending => CheckStatus::Pending,
    }
}

/// `CheckStatusIcon`: the glyph with its status (or `label`) as its name and tooltip.
pub fn check_icon(t: &Tokens, status: CheckStatus, label: Option<&str>, size: f32) -> El {
    let (word, name, tone) = check_look(status);
    let label = label.unwrap_or(word).to_owned();
    el().role("img").label(label.clone()).help(label).child(icon(glyph(name), size).color(tone.ink(t)))
}

/// WCAG contrast of two opaque colours.
fn contrast(a: Color, b: Color) -> f32 {
    let (la, lb) = (a.luminance() + 0.05, b.luminance() + 0.05);
    la.max(lb) / la.min(lb)
}

/// `labelColors`: the ink, wash and edge of a label chip on `surface`. The ink keeps the
/// label's hue and moves in lightness until it reads at 4.5:1 on the washed chip; `None` for
/// a colour that is not six hex digits.
pub fn label_colors(color: &str, surface: Color) -> Option<(Color, Color, Color)> {
    if color.len() != 6 || !color.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let base = rune::color::hex(&format!("#{color}"))?;
    let chip = surface.mix(base, 0.14).alpha(1.0);
    let toward = if chip.luminance() < 0.18 { 1.0 } else { 0.0 };
    let [l, a, b, _] = base.to_oklab();
    let mut ink = base;
    for step in 0..=20 {
        if contrast(ink, chip) >= 4.5 {
            break;
        }
        let k = step as f32 / 20.0;
        ink = Color::from_oklab([l + (toward - l) * k, a * (1.0 - k), b * (1.0 - k), 1.0]);
    }
    Some((ink, base.alpha(0.14), base.alpha(0.45)))
}

/// `LabelChip`: `md` on the item page and in pickers, `sm` in list rows.
pub fn label_chip(t: &Tokens, label: &Label, md: bool) -> El {
    let c = &t.colors;
    let colors = label_colors(&label.color, c.background);
    let (h, px, font) = if md { (24.0, 8.0, sans(12.0, 400.0, 16.0)) } else { (20.0, 6.0, sans(10.0, 400.0, 14.0)) };
    let (ink, fill, edge) = colors.unwrap_or((c.muted_foreground, Color::TRANSPARENT, c.border));
    row()
        .h(h)
        .px(px)
        .gap(4.0)
        .items_center()
        .min_w(0.0)
        .radius(h / 2.0)
        .fill(fill)
        .border(1.0, edge)
        .child(el().w(8.0).h(8.0).radius(4.0).fill(ink).a11y_hidden())
        .child(text(label.name.clone()).font(font).color(ink).single_line().truncate().min_w(0.0))
}

/// `LabelDot`: a label colour as a dot (picker rows).
pub fn label_dot(t: &Tokens, color: &str) -> El {
    let ink = label_colors(color, t.colors.popover).map_or(t.colors.muted_foreground, |(ink, _, _)| ink);
    el().w(10.0).h(10.0).radius(5.0).fill(ink).a11y_hidden()
}

/// `GithubAvatar`: a login's initial in a circle. Avatars are never fetched.
pub fn avatar(t: &Tokens, login: &str, small: bool) -> El {
    let initial = login.chars().find(char::is_ascii_alphanumeric).map_or('?', |ch| ch.to_ascii_uppercase());
    let (side, font) = if small { (20.0, sans(10.0, 500.0, 14.0)) } else { (32.0, sans(12.0, 500.0, 16.0)) };
    row()
        .w(side)
        .h(side)
        .center()
        .radius(side / 2.0)
        .fill(t.colors.foreground.alpha(0.1))
        .a11y_hidden()
        .child(text(initial.to_string()).font(font).color(t.colors.foreground))
}

/// `FilterChip`: a rounded chip with a chevron that opens a menu, a picker or a sheet.
pub fn filter_chip(t: &Tokens, name: &str, label: &str, glyph_name: Option<&str>, active: bool, count: usize, touch: bool) -> El {
    let c = &t.colors;
    let ink = if active { c.foreground } else { c.muted_foreground };
    let mut chip = row()
        .role("button")
        .label(name.to_owned())
        .h(if touch { 36.0 } else { 28.0 })
        .px(if touch { 12.0 } else { 10.0 })
        .gap(6.0)
        .items_center()
        .radius(18.0)
        .border(1.0, if active { c.primary.alpha(0.6) } else { c.border })
        .focusable(true)
        .focus_ring(2.0, 0.0, c.focus_ring);
    chip = if active { chip.fill(c.primary.alpha(0.1)) } else { chip.hover(c.foreground.alpha(0.03)) };
    if let Some(name) = glyph_name {
        chip = chip.child(icon(glyph(name), 12.0).color(ink));
    }
    chip = chip.child(
        text(label.to_owned()).font(sans(if touch { 14.0 } else { 12.0 }, 400.0, 16.0)).color(ink).single_line().truncate().max_w(160.0),
    );
    if count > 0 {
        chip = chip.child(
            row()
                .h(16.0)
                .min_w(16.0)
                .px(4.0)
                .center()
                .radius(8.0)
                .fill(c.primary)
                .child(text(count.to_string()).font(sans(10.0, 500.0, 14.0)).color(c.primary_foreground)),
        );
    }
    chip.child(icon(glyph("ChevronDown"), 12.0).color(ink))
}

/// The labels a row shows; the rest fold into `+N`.
pub const MAX_ROW_LABELS: usize = 3;

/// The row's accessible name: state, kind, number and title, then checks, comments, labels.
pub fn row_name(item: &ListItem) -> String {
    let (state, _, _) = item_look(item.kind, item.state);
    let kind = if item.kind == ItemKind::Pr { "pull request" } else { "issue" };
    let mut parts = vec![format!("{state} {kind} #{}: {}", item.number, item.title)];
    if let Some(rollup) = item.checks {
        parts.push(rollup_label(rollup).to_lowercase());
    }
    if item.comment_count > 0 {
        parts.push(format!("{} comments", item.comment_count));
    }
    if !item.labels.is_empty() {
        let names: Vec<&str> = item.labels.iter().map(|l| l.name.as_str()).collect();
        parts.push(format!("labels {}", names.join(", ")));
    }
    parts.join(", ")
}

/// `GithubListRow`: state glyph, title, then `#n · author · updated` with the comment count
/// and the PR's check rollup, then its labels. `updated` is the relative time already drawn.
pub fn list_row(t: &Tokens, item: &ListItem, active: bool, updated: &str, touch: bool) -> El {
    let c = &t.colors;
    let meta_font = sans(11.0, 400.0, 14.0);
    let dot = || text("\u{b7}").font(meta_font).color(c.muted_foreground).a11y_hidden();
    let mut meta = row()
        .w_full()
        .gap(6.0)
        .items_center()
        .min_w(0.0)
        .child(text(format!("#{}", item.number)).font(meta_font).color(c.muted_foreground).single_line())
        .child(dot())
        .child(text(item.author.clone()).font(meta_font).color(c.muted_foreground).single_line().truncate().min_w(0.0));
    if !updated.is_empty() {
        meta = meta.child(dot()).child(text(updated.to_owned()).font(meta_font).color(c.muted_foreground).single_line());
    }
    let mut trail = row().gap(6.0).items_center();
    if item.comment_count > 0 {
        trail = trail.child(
            row()
                .gap(2.0)
                .items_center()
                .help(format!("{} comments", item.comment_count))
                .child(icon(glyph("Comment"), 12.0).color(c.muted_foreground))
                .child(text(item.comment_count.to_string()).font(meta_font).color(c.muted_foreground)),
        );
    }
    if let Some(rollup) = item.checks {
        trail = trail.child(check_icon(t, rollup_status(rollup), Some(rollup_label(rollup)), 14.0));
    }
    meta = meta.child(el().grow(1.0)).child(trail);
    let mut body = col()
        .grow(1.0)
        .min_w(0.0)
        .gap(4.0)
        .child(
            text(item.title.clone())
                .font(sans(if touch { 14.0 } else { 12.0 }, 500.0, if touch { 20.0 } else { 16.0 }))
                .color(c.foreground)
                .wrap()
                .max_lines(2)
                .w_full(),
        )
        .child(meta);
    if !item.labels.is_empty() {
        let mut chips = row().w_full().gap(4.0).items_center().wrap();
        for label in item.labels.iter().take(MAX_ROW_LABELS) {
            chips = chips.child(label_chip(t, label, false));
        }
        let hidden = item.labels.len().saturating_sub(MAX_ROW_LABELS);
        if hidden > 0 {
            chips = chips.child(text(format!("+{hidden}")).font(sans(10.0, 400.0, 14.0)).color(c.muted_foreground));
        }
        body = body.child(chips);
    }
    let mut r = row()
        .role("button")
        .label(row_name(item))
        .help(item.title.clone())
        .checked(active)
        .w_full()
        .min_w(0.0)
        .px(10.0)
        .py(8.0)
        .gap(8.0)
        .radius(8.0)
        .focusable(true)
        .focus_ring(1.0, 0.0, c.focus_ring)
        .child(el().mt(2.0).child(item_state_icon(t, item.kind, item.state, 14.0)))
        .child(body);
    r = if active { r.fill(c.keycap_fill) } else { r.hover(c.foreground.alpha(0.03)) };
    r
}

#[cfg(test)]
mod tests;
