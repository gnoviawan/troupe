//! The diff tab (`components/git/DiffTab.tsx`): one working-tree file's staged or unstaged
//! diff, drawn by `rune-kit`'s `DiffView` from the patch the server returns.

use model::workspace::Tab;
use rune::{El, col, icon, row, text};
use rune_kit::diff::{DiffState, DiffView};
use theme::Tokens;
use ui::git::{glyph, split_path};
use ui::type_::{MICRO, sans};
use ui::{Button, ButtonSize, ButtonVariant};

use crate::State;

thread_local! {
    static EMPTY: &'static DiffState = Box::leak(Box::new(DiffState::new(Vec::new())));
}

fn view_of<'a>(s: &'a State, id: &str) -> &'a DiffState {
    s.git.diff_views.get(id).map_or_else(|| EMPTY.with(|e| *e), |(_, view)| view)
}

/// The body of a diff tab, or `None` for another kind.
pub fn tab_body(s: &State, t: &'static Tokens, tab: &Tab) -> Option<El> {
    let Tab::Diff { id, cwd, path, staged } = tab else {
        return None;
    };
    let c = &t.colors;
    let (name, dir) = split_path(path);
    let side = if *staged { "Staged" } else { "Unstaged" };
    let header = row()
        .key(format!("diff-header-{id}"))
        .w_full()
        .h(36.0)
        .px(12.0)
        .gap(8.0)
        .items_center()
        .border_sides([0.0, 0.0, 1.0, 0.0], c.border)
        .child(icon(glyph("FileDiff"), 13.0).color(c.muted_foreground))
        .child(text(name.to_owned()).font(sans(12.0, 500.0, 16.0)).color(c.foreground).single_line())
        .child(text(dir.to_owned()).font(sans(MICRO, 400.0, 16.0)).color(c.muted_foreground).single_line().truncate().min_w(0.0).grow(1.0))
        .child(text(side).font(sans(MICRO, 400.0, 16.0)).color(c.muted_foreground));
    let body = diff_body(s, t, id, cwd, path, *staged);
    Some(col().key(format!("tab-body-{id}")).w_full().grow(1.0).min_h(0.0).child(header).child(body))
}

/// The diff of one file side under view key `id`: the view, its read failure, or a note.
pub fn diff_body(s: &State, t: &'static Tokens, id: &str, cwd: &str, path: &str, staged: bool) -> El {
    let c = &t.colors;
    let staged = &staged;
    match (s.git.diff_views.get(id), s.git.cache.diff_error(cwd, path, *staged)) {
        (Some((text_read, _)), _) if text_read.trim().is_empty() => note(t, "No changes to show"),
        (Some(_), _) => {
            let (get, get_mut) = (id.to_owned(), id.to_owned());
            DiffView::new(
                move |s: &State| view_of(s, &get),
                move |s: &mut State| &mut s.git.diff_views.entry(get_mut.clone()).or_insert_with(|| (String::new(), DiffState::new(Vec::new()))).1,
            )
            .key(format!("diff-view-{id}"))
            .build(s, theme::kit(s.theme, s.dark))
            .grow(1.0)
            .min_h(0.0)
        }
        (None, Some(error)) => {
            let (cwd, path, staged) = (cwd.to_owned(), path.to_owned(), *staged);
            col()
                .grow(1.0)
                .w_full()
                .center()
                .gap(12.0)
                .child(text(format!("Couldn't load the diff: {error}")).font(sans(12.0, 400.0, 16.0)).color(c.destructive).wrap())
                .child(
                    Button::new("Retry")
                        .variant(ButtonVariant::Outline)
                        .size(ButtonSize::Sm)
                        .build(t)
                        .key(format!("diff-retry-{id}"))
                        .on_press(move |s: &mut State| s.read_diff(cwd.clone(), path.clone(), staged)),
                )
        }
        (None, None) => note(t, "Loading diff..."),
    }
}

fn note(t: &Tokens, line: &str) -> El {
    col()
        .grow(1.0)
        .w_full()
        .center()
        .child(text(line.to_owned()).font(sans(12.0, 400.0, 16.0)).color(t.colors.muted_foreground))
}
