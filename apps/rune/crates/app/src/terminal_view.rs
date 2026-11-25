//! A terminal tab's body: the theme's terminal surface with React's padding, the screen, and
//! a line under it once the process has exited or the host has lost it.

use model::terminal::Status;
use model::workspace::Tab;
use rune::{El, col, text};
use theme::Tokens;
use ui::terminal::{PAD, Palette, Screen};
use ui::type_::mono;

use crate::State;

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
    let shown = Screen { term: &term, scrolled: view.scrolled, focused: true, blink_on: true };
    let screen = ui::terminal::screen(&format!("terminal-{terminal_id}"), &shown, &palette, &m, &view.cache);
    let (measured, keyed, wheeled) = (terminal_id.clone(), terminal_id.clone(), terminal_id.clone());
    let surface = col()
        .key(format!("terminal-surface-{terminal_id}"))
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .clip_rounded(0.0)
        .focusable(true)
        .on_key(move |s: &mut State, key, _tx| s.terminal_key(&keyed, key))
        .on_wheel(move |s: &mut State, e, _tx| s.terminal_wheel(&wheeled, e.dy))
        .on_layout_tx(move |s: &mut State, [_, _, w, h], tx| s.terminal_measured(&measured, w, h, tx))
        .child(screen);
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
