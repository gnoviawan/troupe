//! A terminal tab's body: the theme's terminal surface with React's padding, the screen, and
//! a line under it once the process has exited or the host has lost it.

use model::terminal::Status;
use model::workspace::Tab;
use rune::{Cursor, El, Key, col, icon, row, text, text_input};
use theme::Tokens;
use ui::terminal::{PAD, Palette, Screen};
use ui::type_::mono;

use crate::State;
use crate::term_select::{self, Find, find_field_key};

/// The tab's name (`Terminal <n>`), or None for a tab this client did not open.
pub fn tab_title(s: &State, tab: &Tab) -> Option<String> {
    let Tab::Terminal { terminal_id, .. } = tab else { return None };
    s.terms.names.get(terminal_id).cloned()
}

pub fn tab_body(s: &State, t: &'static Tokens, tab: &Tab) -> Option<El> {
    let Tab::Terminal { terminal_id, .. } = tab else { return None };
    let palette = Palette::new(t);
    let m = s.terms.metrics();
    let body = col()
        .key(format!("tab-body-{}", tab.id()))
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .fill(palette.bg)
        .pl(PAD[0])
        .pr(PAD[0])
        .pt(PAD[1])
        .pb(PAD[2]);
    let note = |message: String| text(message).font(mono(13.0, 400.0, 18.0)).color(t.colors.muted_foreground);
    let Some(view) = s.terms.views.get(terminal_id) else {
        return Some(body.child(note("This terminal is not running in this window.".into())));
    };
    let Ok(term) = view.cells.lock() else {
        return Some(body.child(note("This terminal's screen is unavailable.".into())));
    };
    let focused = s.terms.focused.as_deref() == Some(terminal_id.as_str());
    let marks = term_select::marks(view, &term, &palette, [t.colors.search_match, t.colors.search_match_active], focused);
    let shown = Screen { term: &term, scrolled: view.scrolled, focused: true, blink_on: true, marks: &marks };
    let id = terminal_id.clone();
    let screen = ui::terminal::screen(&format!("terminal-{terminal_id}"), &shown, &palette, &m, &view.cache)
        .cursor(if view.hover.is_some() { Cursor::Pointer } else { Cursor::Text })
        .on_drag({
            let id = id.clone();
            move |s: &mut State, d, _tx| {
                s.terminal_drag(&id, d);
            }
        })
        .on_pointer_move_local({
            let id = id.clone();
            move |s: &mut State, p, _tx| {
                s.terminal_hover(&id, p.x, p.y);
            }
        });
    let (measured, keyed, wheeled) = (id.clone(), id.clone(), id.clone());
    let mut surface = col()
        .key(format!("terminal-surface-{terminal_id}"))
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .clip_rounded(0.0)
        .focusable(true)
        .on_key(move |s: &mut State, key, tx| s.terminal_key(&keyed, key, tx))
        .on_wheel(move |s: &mut State, e, _tx| s.terminal_wheel(&wheeled, e.dy))
        .on_layout_tx(move |s: &mut State, [_, _, w, h], tx| s.terminal_measured(&measured, w, h, tx))
        .child(screen);
    if let Some(find) = &view.find {
        surface = surface.child(find_bar(t, &id, find));
    }
    let mut body = body.child(surface);
    match s.terms.sessions.get(terminal_id).map(|session| &session.status) {
        Some(Status::Exited { code }) => {
            let code = code.map_or_else(|| "unknown".to_owned(), |c| c.to_string());
            body = body.child(note(format!("Process exited with code {code}")));
        }
        Some(Status::Lost { reason }) => body = body.child(note(format!("Disconnected from this terminal: {reason}"))),
        _ => {}
    }
    Some(body)
}

type FindAction = dyn Fn(&mut State, &mut rune::Tx);

/// The find bar over the screen's top right: the query, "n of m", previous, next and close.
/// Return goes to the next match, Shift+Return to the previous one, Escape closes it.
fn find_bar(t: &'static Tokens, id: &str, find: &Find) -> El {
    let c = &t.colors;
    let button = |glyph: &'static str, label: &str, key: &str, f: Box<FindAction>| {
        row()
            .key(format!("terminal-find-{key}-{id}"))
            .role("button")
            .label(label.to_owned())
            .w(24.0)
            .h(24.0)
            .center()
            .radius(4.0)
            .child(icon(theme::icons::named(glyph).unwrap_or(&theme::icons::X), 14.0).color(c.muted_foreground))
            .on_press_tx(move |s: &mut State, tx| f(s, tx))
    };
    let (next, prev, close, submit, keyed) = (id.to_owned(), id.to_owned(), id.to_owned(), id.to_owned(), id.to_owned());
    let edited = id.to_owned();
    let field = text_input(&find.edit, mono(13.0, 400.0, 18.0))
        .key(find_field_key(id))
        .single_line()
        .label("Find in terminal".to_owned())
        .placeholder("Find".to_owned(), c.muted_foreground)
        .color(c.foreground)
        .w(200.0)
        .px(8.0)
        .py(3.0)
        .on_edit(move |s: &mut State| match s.terms.views.get_mut(&edited).and_then(|v| v.find.as_mut()) {
            Some(find) => &mut find.edit,
            None => &mut s.terms.spare_edit,
        })
        .on_submit(move |s: &mut State, _tx| s.terminal_find_step(&submit, true))
        .on_key(move |s: &mut State, key, tx| match key.key {
            Key::Escape => {
                s.close_terminal_find(&keyed, tx);
                true
            }
            Key::Enter if key.mods.shift => {
                s.terminal_find_step(&keyed, false);
                true
            }
            _ => false,
        });
    let count = if find.error.is_some() { "Invalid search".to_owned() } else { find.count() };
    row()
        .key(format!("terminal-find-bar-{id}"))
        .pin(None, Some(4.0), Some(8.0), None)
        .gap(2.0)
        .items_center()
        .px(4.0)
        .py(4.0)
        .radius(6.0)
        .fill(c.popover)
        .border(1.0, c.border)
        .child(field)
        .child(text(count).font(mono(12.0, 400.0, 16.0)).color(c.muted_foreground).min_w(64.0).px(4.0))
        .child(button("ChevronUp", "Previous match", "prev", Box::new(move |s, _| s.terminal_find_step(&prev, false))))
        .child(button("ChevronDown", "Next match", "next", Box::new(move |s, _| s.terminal_find_step(&next, true))))
        .child(button("X", "Close find", "close", Box::new(move |s, tx| s.close_terminal_find(&close, tx))))
}
