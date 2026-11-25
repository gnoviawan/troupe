//! The git panel's parts (`components/git/rows.tsx`): the file row with its status letter,
//! the section header, the row and section action buttons, and the filter field. The app
//! composes them into the Changes panel and attaches the handlers.

use rune::{El, Icon, TextEdit, col, icon, row, text};
use theme::Tokens;

use crate::type_::{MICRO, mono, sans};

/// A bundled icon by its `components/icons` name.
pub fn glyph(name: &str) -> &'static Icon {
    theme::icons::named(name).unwrap_or(&theme::icons::X)
}

/// `splitGitPath`: the file name and its parent folder (`""` at the root).
pub fn split_path(path: &str) -> (&str, &str) {
    match path.rfind('/') {
        None => (path, ""),
        Some(slash) => {
            let name = &path[slash + 1..];
            (if name.is_empty() { path } else { name }, &path[..slash])
        }
    }
}

/// `GIT_STATUS_LETTER` and `GIT_STATUS_LABEL`; the legacy `staged` reads as modified.
pub fn status_letter(status: &str) -> (&'static str, &'static str) {
    match status {
        "added" => ("A", "Added"),
        "untracked" => ("U", "Untracked"),
        "deleted" => ("D", "Deleted"),
        "renamed" => ("R", "Renamed"),
        _ => ("M", "Modified"),
    }
}

/// `GitStatusLetter`: a 14 px mono letter in the status colour.
pub fn status_mark(t: &Tokens, status: &str) -> El {
    let c = &t.colors;
    let (letter, label) = status_letter(status);
    let ink = match status {
        "added" | "untracked" => c.diff_added,
        "deleted" => c.destructive,
        _ => c.diff_modified,
    };
    row()
        .w(14.0)
        .justify_center()
        .role("img")
        .label(label)
        .help(label)
        .child(text(letter).font(mono(MICRO, 400.0, 16.0)).color(ink))
}

/// A row or section action: a quiet 24 px icon button, red on hover when `danger`.
pub fn action(t: &Tokens, name: &str, label: &str, size: f32, disabled: bool, danger: bool) -> El {
    let c = &t.colors;
    let mut b = row()
        .role("button")
        .label(label.to_owned())
        .help(label.to_owned())
        .w(24.0)
        .h(24.0)
        .center()
        .radius(6.0)
        .focusable(!disabled)
        .focus_ring(2.0, 0.0, c.focus_ring);
    let ink = icon(glyph(name), size).color(c.muted_foreground);
    if disabled {
        return b.opacity(0.4).child(ink);
    }
    b = if danger {
        b.hover(c.destructive.alpha(0.1)).hover_group().child(ink.group_color(c.destructive))
    } else {
        b.hover(c.foreground.alpha(0.03)).hover_group().child(ink.group_color(c.foreground))
    };
    b
}

/// How a file row is lit: the open diff's row, a multi-selected row, or neither.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowState {
    Active,
    Selected,
    Idle,
}

/// `FileItem`: the file glyph, the name and its folder, the hover actions and the letter.
pub fn file_row(t: &Tokens, path: &str, status: &str, state: RowState, touch: bool, actions: Vec<El>) -> El {
    let c = &t.colors;
    let (name, dir) = split_path(path);
    let mut names = row()
        .grow(1.0)
        .min_w(0.0)
        .gap(8.0)
        .items_center()
        .child(text(name.to_owned()).font(sans(12.0, 400.0, 16.0)).color(ink(t, state)).single_line().truncate());
    if !dir.is_empty() {
        let size = if touch { 12.0 } else { MICRO };
        names = names.child(
            text(dir.to_owned())
                .font(sans(size, 400.0, 16.0))
                .color(c.muted_foreground)
                .single_line()
                .truncate()
                .min_w(0.0),
        );
    }
    let tools = row().gap(2.0).items_center().group_opacity(if touch { 1.0 } else { 0.0 }, 1.0).children(actions);
    let mut r = row()
        .role("option")
        .label(path.to_owned())
        .w_full()
        .min_w(0.0)
        .gap(8.0)
        .px(8.0)
        .items_center()
        .radius(6.0)
        .focusable(true)
        .focus_ring(2.0, 0.0, c.focus_ring)
        .hover_group()
        .child(icon(glyph("File"), 14.0).color(c.project_blue))
        .child(names)
        .child(tools)
        .child(status_mark(t, status));
    r = if touch { r.min_h(44.0).py(4.0) } else { r.h(28.0) };
    match state {
        RowState::Active => r.fill(c.row_hover_solid),
        RowState::Selected => r.fill(c.foreground.alpha(0.06)),
        RowState::Idle => r.hover(c.foreground.alpha(0.03)),
    }
}

fn ink(t: &Tokens, state: RowState) -> rune::Color {
    match state {
        RowState::Idle => t.colors.secondary_foreground,
        _ => t.colors.foreground,
    }
}

/// `SectionHeader`: a chevron, the label, the count, `· N selected`, then its actions.
pub fn section_header(t: &Tokens, label: &str, count: usize, selected: usize, actions: Vec<El>) -> El {
    let c = &t.colors;
    let mut r = row()
        .h(32.0)
        .w_full()
        .pl(8.0)
        .pr(4.0)
        .gap(6.0)
        .items_center()
        .child(icon(glyph("ChevronDown"), 11.0).color(c.muted_foreground))
        .child(text(label.to_owned()).font(sans(MICRO, 500.0, 16.0)).color(c.muted_foreground).single_line())
        .child(text(count.to_string()).font(sans(MICRO, 400.0, 16.0)).color(c.muted_foreground));
    if selected > 1 {
        r = r.child(text(format!("\u{b7} {selected} selected")).font(sans(MICRO, 400.0, 16.0)).color(c.muted_foreground));
    }
    r.child(row().grow(1.0)).child(row().gap(2.0).items_center().children(actions))
}

/// `ChangesFilter`: the search glyph over the panel field.
pub fn filter_field(t: &Tokens, edit: &TextEdit, touch: bool) -> El {
    let field = crate::base::panel_styles::panel_field(t, edit, "Filter changes", touch)
        .w_full()
        .h(if touch { 44.0 } else { 28.0 })
        .pl(28.0);
    col().w_full().child(field).child(
        row()
            .pin(Some(10.0), Some(0.0), None, Some(0.0))
            .items_center()
            .child(icon(glyph("Search"), 13.0).color(t.colors.muted_foreground))
            .a11y_hidden(),
    )
}

/// The empty Changes section: a clean tree, a plain folder, or the status read's failure.
pub fn empty_changes(t: &Tokens, status_error: Option<&str>) -> El {
    let c = &t.colors;
    let (line, ink) = match status_error {
        Some(error) if !error.contains("not a git repository") => (format!("Couldn't read changes: {error}"), c.destructive),
        Some(_) => ("This folder is not a git repository.".to_owned(), c.muted_foreground),
        None => ("No changes detected".to_owned(), c.muted_foreground),
    };
    col()
        .w_full()
        .px(16.0)
        .py(32.0)
        .items_center()
        .child(text(line).font(sans(12.0, 400.0, 16.0)).color(ink).wrap().w_full().align(rune::Align::Center))
}

/// The native checkbox the commit box and the stash dialog use (`size-3.5 accent-foreground`).
pub fn checkbox(t: &Tokens, checked: bool, disabled: bool) -> El {
    let c = &t.colors;
    let mut b = row().w(14.0).h(14.0).radius(3.0).center();
    b = if checked {
        b.fill(c.foreground).child(icon(glyph("Check"), 11.0).color(c.background))
    } else {
        b.border(1.0, c.muted_foreground)
    };
    if disabled { b.opacity(0.5) } else { b }
}

#[cfg(test)]
mod tests;
