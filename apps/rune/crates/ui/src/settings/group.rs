//! `SettingsGroup` and its split layout. On a wide page (the settings column at `52rem`, 832 px,
//! or more) a titled group puts its heading, help and actions in a 240 px column on the left and
//! its cards on the right, with a hairline between sections. Narrower, it stacks.
//!
//! React reads the column's width with a container query (`@container/settings`). Here the page
//! body measures it and builds the page inside [`page`], and the parts read it back: the
//! groups sit many calls deep in the pages, and none of those calls has the width.

use std::cell::Cell;

use rune::{El, col, el, row, text};
use theme::Tokens;

use crate::type_::sans;

/// The page column's widest (`SETTINGS_COLUMN_CLASS`, `max-w-[1080px]`).
pub const COLUMN: f32 = 1080.0;
/// The column width from which titled groups split (`@[52rem]/settings`).
pub const SPLIT_FROM: f32 = 832.0;
/// The heading column of a split group (`grid-cols-[240px_minmax(0,1fr)]`).
pub const HEADING: f32 = 240.0;
/// The space between a split group's heading and its content (`gap-x-8`).
pub const SPLIT_GAP: f32 = 32.0;

thread_local! {
    static WIDTH: Cell<f32> = const { Cell::new(0.0) };
    static SECTIONS: Cell<usize> = const { Cell::new(0) };
}

/// Build a page's groups for a settings column `width` wide.
pub fn page<R>(width: f32, build: impl FnOnce() -> R) -> R {
    let was = (WIDTH.replace(width), SECTIONS.replace(0));
    let out = build();
    WIDTH.set(was.0);
    SECTIONS.set(was.1);
    out
}

/// The settings column's width while a page builds, 0 outside one.
pub fn column_width() -> f32 {
    WIDTH.get()
}

/// Whether titled groups split on the page being built.
pub fn split() -> bool {
    column_width() >= SPLIT_FROM
}

/// Note a lead element above the first split section, so that section draws its top hairline
/// (`first:border-t-0` applies only to a section that is its parent's first child).
pub fn after_lead() {
    SECTIONS.set(SECTIONS.get().max(1));
}

/// A group's heading words (`title`, `description`, `meta`).
#[derive(Clone, Copy, Debug, Default)]
pub struct Heading<'a> {
    pub title: &'a str,
    pub description: &'a str,
    /// Quiet text after the title: a count or a summary.
    pub meta: &'a str,
}

/// `SettingsGroupLayout`: `Split` puts the heading on the left on a wide page; `Stacked` keeps
/// full-width content, such as a grid of cards.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    Split,
    Stacked,
}

/// `SettingsGroup`: a 14/20 heading, its actions at the right, then 12 px, then `content`.
pub fn group(t: &Tokens, key: &str, title: &str, actions: Vec<El>, content: Vec<El>) -> El {
    section(t, key, Heading { title, ..Heading::default() }, Layout::Split, actions, content)
}

/// `SettingsGroup` with its `meta`: a 12 px note after the heading, such as a count.
pub fn group_with_meta(t: &Tokens, key: &str, title: &str, meta: &str, actions: Vec<El>, content: Vec<El>) -> El {
    section(t, key, Heading { title, meta, ..Heading::default() }, Layout::Split, actions, content)
}

/// `SettingsGroup` with its `description`: help under the title.
pub fn described(t: &Tokens, key: &str, title: &str, description: &str, actions: Vec<El>, content: Vec<El>) -> El {
    section(t, key, Heading { title, description, ..Heading::default() }, Layout::Split, actions, content)
}

/// `SettingsGroup` with every prop.
pub fn section(t: &Tokens, key: &str, heading: Heading<'_>, layout: Layout, actions: Vec<El>, content: Vec<El>) -> El {
    if layout == Layout::Split && !heading.title.is_empty() && split() {
        let left = split_heading(t, heading, actions);
        let right = col().key("body").grow(1.0).min_w(0.0).gap(12.0).items_stretch().children(content);
        return split_row(t, key, heading.title, left, right);
    }
    stacked(t, key, heading, actions, content)
}

fn title_line(t: &Tokens, heading: Heading<'_>) -> El {
    let c = &t.colors;
    let mut words = row()
        .key("title")
        .min_w(0.0)
        .gap(8.0)
        .items_center()
        .child(text(heading.title).font(sans(14.0, 500.0, 20.0)).color(c.foreground).role("heading"));
    if !heading.meta.is_empty() {
        words = words.child(text(heading.meta).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground).single_line().truncate());
    }
    words
}

/// Help under a title (`text-xs leading-relaxed text-muted-foreground`).
fn help(t: &Tokens, description: &str) -> El {
    text(description).key("description").font(sans(12.0, 400.0, 19.5)).color(t.colors.muted_foreground).wrap()
}

fn stacked(t: &Tokens, key: &str, heading: Heading<'_>, actions: Vec<El>, content: Vec<El>) -> El {
    let mut head = row().key("head").w_full().min_h(28.0).items_center().justify_between().gap(12.0);
    if !heading.title.is_empty() || !heading.description.is_empty() {
        let mut words = col().key("words").min_w(0.0).gap(4.0);
        if !heading.title.is_empty() {
            words = words.child(title_line(t, heading));
        }
        if !heading.description.is_empty() {
            words = words.child(help(t, heading.description));
        }
        head = head.child(words);
    }
    if !actions.is_empty() {
        head = head.child(row().key("actions").gap(8.0).items_center().children(actions));
    }
    col()
        .key(key.to_owned())
        .role("group")
        .label(heading.title.to_owned())
        .w_full()
        .gap(12.0)
        .items_stretch()
        .child(head)
        .children(content)
}

/// A split section's left column: the title and meta, the help, then the actions.
pub fn split_heading(t: &Tokens, heading: Heading<'_>, actions: Vec<El>) -> El {
    let mut left = col().key("head").w(HEADING).pt(4.0).gap(6.0).items_start().child(title_line(t, heading));
    if !heading.description.is_empty() {
        left = left.child(help(t, heading.description));
    }
    if !actions.is_empty() {
        left = left.child(row().key("actions").pt(6.0).gap(8.0).items_center().children(actions));
    }
    left
}

/// `SETTINGS_SPLIT_GROUP_CLASS`: `left` and `right` side by side, under a hairline and 24 px
/// unless it is the page's first section.
pub fn split_row(t: &Tokens, key: &str, label: &str, left: El, right: El) -> El {
    let first = SECTIONS.replace(SECTIONS.get() + 1) == 0;
    let mut out = col().key(key.to_owned()).role("group").label(label.to_owned()).w_full().items_stretch();
    if !first {
        out = out.child(el().key("rule").h(0.5).w_full().fill(t.colors.border).a11y_hidden()).child(el().key("rule-space").h(24.0));
    }
    out.child(row().key("split").w_full().gap(SPLIT_GAP).items_start().child(left).child(right))
}
