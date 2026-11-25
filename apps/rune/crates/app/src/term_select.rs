//! Selection, copy, search and links on a terminal tab, as the React terminal (xterm.js with
//! its search and web-links addons) has them. A drag selects cells, a double click words and a
//! triple click lines; the platform modifier with C copies a selection (and sends ^C without
//! one), with A selects everything. The Terminal Search shortcut opens a find bar whose matches
//! the screen fills. A click on a URL in the output asks before opening it.

use rune::{Drag, Phase, TextEdit, Tx};
use troupe_term::Term;
use troupe_term::links::{Link, link_at};
use troupe_term::search::{Match, Query, compile, find_all, next_after, previous_before};
use troupe_term::selection::{Selection, Unit};
use troupe_term::text::Point;
use ui::terminal::{Mark, Metrics, Palette};

use crate::State;
use crate::terminal::TermView;

/// The search addon's wait after output before it reads the matches again (seconds).
const FIND_DEBOUNCE: f64 = 0.2;
/// `DEFAULT_HIGHLIGHT_LIMIT`: the most matches a find bar marks and counts.
pub const HIGHLIGHT_LIMIT: usize = 1000;

/// A selection and where the buffer stood when it was made, so dropped scrollback and a switch
/// to the other screen move or clear it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Held {
    pub sel: Selection,
    dropped: usize,
    alt: bool,
}

impl Held {
    fn new(sel: Selection, term: &Term) -> Held {
        Held { sel, dropped: term.dropped(), alt: term.alt_screen() }
    }

    /// The selection in today's buffer, or None once its lines are gone.
    pub fn current(&self, term: &Term) -> Option<Selection> {
        if term.alt_screen() != self.alt {
            return None;
        }
        self.sel.shift(term.dropped().checked_sub(self.dropped)?)
    }
}

/// The find bar of one terminal.
#[derive(Clone, Debug, Default)]
pub struct Find {
    pub edit: TextEdit,
    pub matches: Vec<Match>,
    pub active: Option<usize>,
    pub error: Option<String>,
    /// The query the matches are for, and the output they were read from.
    searched: Option<String>,
    stale: bool,
    /// When output last changed under an open bar: the matches are read again
    /// `FIND_DEBOUNCE` after the last change, as the search addon's `_updateMatches` waits.
    output_at: Option<f64>,
}

impl Find {
    /// "n of m", "No results", or nothing before a query.
    pub fn count(&self) -> String {
        match (self.edit.text.is_empty(), self.active, self.matches.len()) {
            (true, _, _) => String::new(),
            (false, _, 0) => "No results".into(),
            (false, Some(i), n) => format!("{} of {n}", i + 1),
            (false, None, n) => format!("{n} results"),
        }
    }
}

/// The key of a terminal's find field.
pub fn find_field_key(id: &str) -> String {
    format!("terminal-find-{id}")
}

/// The cell under `(x, y)` points of the screen. A cell selection snaps to the nearest column
/// edge, as xterm.js's does; words, lines and links take the column under the pointer.
fn point_at(term: &Term, top: usize, m: &Metrics, x: f32, y: f32, edge: bool) -> Point {
    let col = if edge { (x / m.cell_w).round() } else { (x / m.cell_w).floor() };
    let row = (y / m.cell_h).floor().clamp(0.0, (term.rows() - 1) as f32) as usize;
    Point::new(top + row, col.clamp(0.0, term.cols() as f32) as usize)
}

fn top(term: &Term, view: &TermView) -> usize {
    term.screen_start().saturating_sub(view.scrolled)
}

impl State {
    /// A drag on terminal `id`'s screen: a press starts a selection by the unit its click count
    /// names, a move extends it, and a click that selected nothing clears it and opens the link
    /// under it, if any.
    pub(crate) fn terminal_drag(&mut self, id: &str, d: &Drag) -> bool {
        let m = self.terms.metrics();
        self.terms.focused = Some(id.to_owned());
        let Some(view) = self.terms.views.get_mut(id) else { return false };
        let cells = view.cells.clone();
        let Ok(term) = cells.lock() else { return false };
        let top = top(&term, view);
        let unit = Unit::of_clicks(d.clicks);
        let at = point_at(&term, top, &m, d.x, d.y, unit == Unit::Cell);
        match d.phase {
            Phase::Down => view.selection = Some(Held::new(Selection::new(at, unit), &term)),
            Phase::Move => {
                if let Some(held) = &mut view.selection {
                    held.sel.head = at;
                }
            }
            Phase::Up => {
                let empty = view.selection.and_then(|h| h.current(&term)).is_none_or(|s| s.range(term.lines()).is_none());
                if empty {
                    view.selection = None;
                    if d.clicks <= 1 && !d.cancelled() {
                        let under = point_at(&term, top, &m, d.x, d.y, false);
                        if let Some(link) = link_at(term.lines(), under) {
                            drop(term);
                            log::info!("[term] link clicked in {id}");
                            self.request_link_open(&link.url);
                        }
                    }
                }
            }
        }
        true
    }

    /// The pointer moved over terminal `id`'s screen: underline the link under it.
    pub(crate) fn terminal_hover(&mut self, id: &str, x: f32, y: f32) -> bool {
        let m = self.terms.metrics();
        let Some(view) = self.terms.views.get_mut(id) else { return false };
        let link = match view.cells.lock() {
            Ok(term) => link_at(term.lines(), point_at(&term, top(&term, view), &m, x, y, false)),
            Err(_) => None,
        };
        if link == view.hover {
            return false;
        }
        view.hover = link;
        true
    }

    /// Copies terminal `id`'s selection: true when there was one.
    pub(crate) fn terminal_copy(&mut self, id: &str, tx: &mut Tx) -> bool {
        let Some(view) = self.terms.views.get(id) else { return false };
        let Ok(term) = view.cells.lock() else { return false };
        let text = view.selection.and_then(|h| h.current(&term)).map(|s| s.text(term.lines())).unwrap_or_default();
        if text.is_empty() {
            return false;
        }
        log::info!("[term] copied {} characters from {id}", text.chars().count());
        tx.copy(text);
        true
    }

    pub(crate) fn terminal_select_all(&mut self, id: &str) {
        let Some(view) = self.terms.views.get_mut(id) else { return };
        let Ok(term) = view.cells.lock() else { return };
        view.selection = Some(Held::new(Selection::all(term.lines()), &term));
    }

    /// The Terminal Search shortcut: open the focused terminal's find bar, or focus it again.
    pub(crate) fn open_terminal_find(&mut self, tx: &mut Tx) -> bool {
        let Some(id) = self.terms.focused.clone() else { return false };
        let Some(view) = self.terms.views.get_mut(&id) else { return false };
        let find = view.find.get_or_insert_with(Find::default);
        find.stale = true;
        log::info!("[term] find opened in {id}");
        tx.focus(find_field_key(&id), true);
        true
    }

    /// Escape outside the terminal (in its find field): close the find bar of the terminal
    /// that had focus last. A focused terminal sends its own Escape to the program.
    pub(crate) fn close_focused_find(&mut self, tx: &mut Tx) -> bool {
        let Some(id) = self.terms.focused.clone() else { return false };
        if self.terms.views.get(&id).is_none_or(|v| v.find.is_none()) {
            return false;
        }
        self.close_terminal_find(&id, tx);
        true
    }

    pub(crate) fn close_terminal_find(&mut self, id: &str, tx: &mut Tx) {
        if let Some(view) = self.terms.views.get_mut(id) {
            view.find = None;
        }
        tx.focus(format!("terminal-surface-{id}"), false);
    }

    /// Next (or previous) match: scroll it into view.
    pub(crate) fn terminal_find_step(&mut self, id: &str, forward: bool) {
        let Some(view) = self.terms.views.get_mut(id) else { return };
        let cells = view.cells.clone();
        let Ok(term) = cells.lock() else { return };
        let shown_top = top(&term, view);
        let Some(find) = &mut view.find else { return };
        let from = match (find.active.and_then(|i| find.matches.get(i)), forward) {
            (Some(m), true) => Point::new(m.start.line, m.start.col + 1),
            (Some(m), false) => m.start,
            (None, _) => Point::new(term.screen_start() + term.cursor().row, 0),
        };
        find.active = if forward { next_after(&find.matches, from) } else { previous_before(&find.matches, from) };
        let line = find.active.and_then(|i| find.matches.get(i)).map(|m| m.start.line);
        if let Some(line) = line
            && !(shown_top..shown_top + term.rows()).contains(&line)
        {
            let start = term.screen_start();
            view.scrolled = start.saturating_sub(line.min(start));
        }
    }

    /// Re-reads the matches of every open find bar whose query or output changed.
    pub(crate) fn refresh_terminal_finds(&mut self, changed: &[String], now: f64) -> bool {
        let mut redraw = false;
        for (id, view) in &mut self.terms.views {
            let scrolled = view.scrolled;
            let Some(find) = &mut view.find else { continue };
            if changed.contains(id) {
                find.output_at = Some(now);
            }
            let query = find.edit.text.clone();
            let settled = find.output_at.is_some_and(|at| now >= at + FIND_DEBOUNCE);
            if !find.stale && !settled && find.searched.as_deref() == Some(query.as_str()) {
                continue;
            }
            let Ok(term) = view.cells.lock() else { continue };
            let new_query = find.searched.as_deref() != Some(query.as_str());
            find.stale = false;
            find.output_at = None;
            find.searched = Some(query.clone());
            redraw = true;
            if query.is_empty() {
                (find.matches, find.active, find.error) = (Vec::new(), None, None);
                continue;
            }
            match compile(&Query { text: query, ..Query::default() }) {
                Ok(re) => {
                    find.matches = find_all(term.lines(), &re, HIGHLIGHT_LIMIT);
                    find.error = None;
                    // A new query lands on the first match from the top of the view, as
                    // `findNext` starts from the viewport.
                    if new_query || find.active.is_some_and(|i| i >= find.matches.len()) {
                        let view_top = term.screen_start().saturating_sub(scrolled);
                        find.active = next_after(&find.matches, Point::new(view_top, 0));
                    }
                }
                Err(e) => (find.matches, find.active, find.error) = (Vec::new(), None, Some(e)),
            }
        }
        redraw
    }

    /// When an open find bar next reads its matches again after output, if one waits.
    pub(crate) fn next_find_refresh(&self) -> Option<f64> {
        self.terms.views.values().filter_map(|v| v.find.as_ref()?.output_at).map(|at| at + FIND_DEBOUNCE).reduce(f64::min)
    }
}

/// The fills and underlines terminal `view` draws on its visible rows: search matches, the
/// selection, and the hovered link.
pub fn marks(view: &TermView, term: &Term, palette: &Palette, search: [rune::Color; 2], focused: bool) -> Vec<Mark> {
    let top = top(term, view);
    let (rows, cols) = (term.rows(), term.cols());
    let mut out = Vec::new();
    let mut add = |start: Point, end: Point, fill: Option<rune::Color>, underline: Option<rune::Color>| {
        for line in start.line.max(top)..=end.line.min(top + rows - 1) {
            let from = if line == start.line { start.col } else { 0 };
            let to = if line == end.line { end.col } else { cols };
            if to > from {
                out.push(Mark { row: line - top, start: from, end: to.min(cols), fill, underline });
            }
        }
    };
    if let Some(find) = &view.find {
        for (i, m) in find.matches.iter().enumerate() {
            if m.end.line < top || m.start.line >= top + rows {
                continue;
            }
            let color = if find.active == Some(i) { search[1] } else { search[0] };
            add(m.start, m.end, Some(color), None);
        }
    }
    if let Some((a, b)) = view.selection.and_then(|h| h.current(term)).and_then(|s| s.range(term.lines())) {
        let color = if focused { palette.selection } else { palette.selection_inactive };
        add(a, b, Some(color), None);
    }
    if let Some(Link { at, .. }) = &view.hover {
        add(at.start, at.end, None, Some(palette.fg));
    }
    out
}

#[cfg(test)]
#[path = "term_select_tests.rs"]
mod tests;
