//! The floating find and replace widget (`components/find/FindWidget.tsx`) and the go-to-line
//! field (`GoToLineWidget.tsx`) over the editor's top right.

use rune::{El, Tx, row, text};
use theme::Tokens;
use theme::icons as i;

use super::view::field;
use super::{Editors, Field, FindBar};
use crate::type_::mono;

type Lens<S> = fn(&mut S) -> &mut Editors;

fn panel(t: &Tokens, key: String) -> El {
    let c = &t.colors;
    rune::col()
        .key(key)
        .pin(None, Some(16.0), Some(8.0), None)
        .gap(4.0)
        .px(6.0)
        .py(6.0)
        .radius(10.0)
        .fill(c.popover)
        .border(1.0, c.border)
}

fn button<S: 'static>(t: &Tokens, glyph: &'static rune::Icon, label: &str, key: String, on: bool, f: impl Fn(&mut S, &mut Tx) + 'static) -> El {
    let c = &t.colors;
    let mut b = row()
        .key(key)
        .role("button")
        .label(label.to_owned())
        .w(24.0)
        .h(24.0)
        .center()
        .radius(6.0)
        .child(rune::icon(glyph, 14.0).color(if on { c.foreground } else { c.muted_foreground }))
        .on_press_tx(move |s: &mut S, tx| f(s, tx));
    if on {
        b = b.fill(c.foreground.alpha(0.1));
    }
    b
}

fn text_button<S: 'static>(t: &Tokens, label: &str, key: String, f: impl Fn(&mut S) + 'static) -> El {
    let c = &t.colors;
    row()
        .key(key)
        .role("button")
        .label(label.to_owned())
        .h(24.0)
        .px(8.0)
        .center()
        .radius(6.0)
        .child(text(label.to_owned()).font(crate::type_::sans(12.0, 500.0, 16.0)).color(c.foreground))
        .on_press(move |s: &mut S| f(s))
}

/// "n of m", "1000+", "No results", or nothing before a query.
fn count(bar: &FindBar, doc: &model::editor::Doc) -> String {
    if bar.query.text.is_empty() {
        return String::new();
    }
    if !bar.query().is_valid() {
        return "Invalid".to_owned();
    }
    let m = bar.matches(doc);
    if m.ranges.is_empty() {
        return "No results".to_owned();
    }
    let total = if m.capped { format!("{}+", m.ranges.len()) } else { m.ranges.len().to_string() };
    let sel = doc.editor.selection().range();
    match troupe_editor::find::match_index(&m.ranges, &sel) {
        0 => format!("{total} results"),
        n => format!("{n} of {total}"),
    }
}

pub fn bar<S: 'static>(t: &'static Tokens, lens: Lens<S>, path: &str, doc: &model::editor::Doc, bar: &FindBar) -> El {
    let c = &t.colors;
    let p = |suffix: &str| format!("editor-find-{suffix}-{path}");
    let at = path.to_owned();
    let toggle = |flag: fn(&mut FindBar) -> &mut bool| {
        let at = at.clone();
        move |s: &mut S, _tx: &mut Tx| {
            if let Some(bar) = lens(s).panes.get_mut(&at).and_then(|p| p.find.as_mut()) {
                let v = flag(bar);
                *v = !*v;
            }
        }
    };
    let (prev, next, close, expand) = (at.clone(), at.clone(), at.clone(), at.clone());
    let chevron = if bar.replacing { &i::CHEVRON_DOWN } else { &i::CHEVRON_RIGHT };
    let find_row = row()
        .gap(2.0)
        .items_center()
        .child(button(t, chevron, "Toggle replace", p("expand"), false, move |s: &mut S, _| {
            if let Some(bar) = lens(s).panes.get_mut(&expand).and_then(|p| p.find.as_mut()) {
                bar.replacing = !bar.replacing;
            }
        }))
        .child(field(t, lens, path, &bar.query, Field::Query, "Find", 180.0))
        .child(text(count(bar, doc)).font(mono(11.0, 400.0, 16.0)).color(c.muted_foreground).min_w(64.0).px(4.0))
        .child(button(t, &i::CASE_SENSITIVE, "Match case", p("case"), bar.case_sensitive, toggle(|b| &mut b.case_sensitive)))
        .child(button(t, &i::WHOLE_WORD, "Match whole word", p("word"), bar.whole_word, toggle(|b| &mut b.whole_word)))
        .child(button(t, &i::REGEX, "Use regular expression", p("regex"), bar.regex, toggle(|b| &mut b.regex)))
        .child(button(t, &i::CHEVRON_UP, "Previous match", p("prev"), false, move |s: &mut S, _| lens(s).find_step(&prev, false)))
        .child(button(t, &i::CHEVRON_DOWN, "Next match", p("next"), false, move |s: &mut S, _| lens(s).find_step(&next, true)))
        .child(button(t, &i::X, "Close", p("close"), false, move |s: &mut S, tx| {
            lens(s).close_find(&close);
            tx.focus(super::view::surface_key(&close), false);
        }));
    let mut out = panel(t, p("bar")).role("dialog").label("Find".to_owned()).child(find_row);
    if bar.replacing && !doc.read_only() {
        let (one, all) = (at.clone(), at.clone());
        out = out.child(
            row()
                .gap(2.0)
                .items_center()
                .pl(26.0)
                .child(field(t, lens, path, &bar.replace, Field::Replace, "Replace", 180.0))
                .child(text_button(t, "Replace", p("replace"), move |s: &mut S| lens(s).replace_next(&one)))
                .child(text_button(t, "Replace all", p("replace-all"), move |s: &mut S| lens(s).replace_all(&all))),
        );
    }
    out
}

pub fn goto<S: 'static>(t: &'static Tokens, lens: Lens<S>, path: &str, edit: &rune::TextEdit) -> El {
    panel(t, format!("editor-goto-{path}"))
        .role("dialog")
        .label("Go to line".to_owned())
        .child(field(t, lens, path, edit, Field::GoTo, "Line or line:column", 220.0))
}
