//! What a chat tab draws: the transcript of its session from `State::chat`, with a stand-in
//! for the composer (a later unit) under it, and the full-window table above everything.

use std::rc::Rc;

use model::workspace::Tab;
use rune::{El, col, row, text};
use theme::Tokens;
use ui::chat::{Folds, Lenses};
use ui::markdown::{Hooks, LinkTarget, TABLE_ZOOM};
use ui::type_::sans;

use crate::State;

/// The composer dock's height in the web client (`ChatInputBar` in its tray), held empty
/// until the composer unit draws it.
const COMPOSER_H: f32 = 176.0;

fn lenses() -> Lenses<State> {
    Lenses {
        acp: |s: &State| &s.chat.acp,
        folds: |s: &State| &s.chat.folds,
        folds_mut: |s: &mut State| -> &mut Folds { &mut s.chat.folds },
    }
}

fn hooks() -> Hooks<State> {
    Hooks {
        on_link: Rc::new(|s: &mut State, target: LinkTarget, tx: &mut rune::Tx| match target {
            LinkTarget::File(path) => s.open_file_link(&path),
            LinkTarget::Url(url) => {
                if let Err(e) = tx.open_url(&url) {
                    log::warn!("[chat] could not open {url}: {e}");
                }
            }
            LinkTarget::Fragment(_) => {}
        }),
        on_table_zoom: Rc::new(|s: &mut State, table: String| {
            s.chat.zoomed_table = Some(table);
            s.overlays.open(TABLE_ZOOM);
        }),
        images: Rc::new(|_| None),
    }
}

/// The body of `tab` when the app draws it: a chat's transcript.
pub fn tab_body(s: &State, t: &'static Tokens, tab: &Tab) -> Option<El> {
    let Tab::AgentChat { session_id, .. } = tab else {
        return None;
    };
    let c = &t.colors;
    let body = if s.chat.loaded.contains(session_id) {
        ui::chat::transcript(t, s, session_id, lenses(), hooks())
    } else {
        col().key(format!("chat-loading-{session_id}")).w_full().grow(1.0).min_h(0.0).center().child(
            text("Loading chat…").font(sans(13.0, 400.0, 20.0)).color(c.muted_foreground),
        )
    };
    let composer = row()
        .key(format!("composer-slot-{session_id}"))
        .w_full()
        .h(COMPOSER_H)
        .center()
        .child(text("Composer (R6 composer unit)").font(sans(12.0, 400.0, 16.0)).color(c.disabled_foreground));
    Some(
        col()
            .key(format!("tab-body-{}", tab.id()))
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .child(body)
            .child(composer),
    )
}

/// A chat tab's label: the chat's title as the rail lists it.
pub fn tab_title(s: &State, tab: &Tab) -> Option<String> {
    let Tab::AgentChat { session_id, .. } = tab else {
        return None;
    };
    s.chats.get(session_id).map(|chat| chat.label().to_owned())
}

/// The full-window table, when one is open.
pub fn layer(s: &State, t: &'static Tokens, layer: &mut Vec<El>) {
    let open = s.overlays.is_open(TABLE_ZOOM);
    let Some(table) = s.chat.zoomed_table.as_deref().filter(|_| open) else {
        return;
    };
    ui::markdown::table_zoom(t, table, true, &hooks(), |s: &mut State| {
        s.overlays.close(TABLE_ZOOM);
    }, layer);
}

impl State {
    /// A file link in a reply: open it in an editor tab, relative to the chat's folder.
    pub fn open_file_link(&mut self, path: &str) {
        let (file, _line) = split_line(path);
        log::info!("[chat] open file {file}");
        self.workspace.step(model::workspace::Event::AddEditorTab(self.resolve_path(file), None));
    }
}

impl State {
    /// A relative path from a reply, against the active project's folder.
    fn resolve_path(&self, file: &str) -> String {
        let absolute = file.starts_with('/') || file.starts_with('~') || file.get(1..2) == Some(":");
        match self.active_project().and_then(|p| p.path.as_deref()) {
            Some(root) if !absolute => format!("{}/{}", root.trim_end_matches('/'), file.trim_start_matches("./")),
            _ => file.to_owned(),
        }
    }
}

/// A path without its `:line[:col]` suffix, and the line.
fn split_line(path: &str) -> (&str, Option<u32>) {
    let mut parts = path.rsplitn(3, ':');
    let last = parts.next().unwrap_or(path);
    match (last.parse::<u32>(), parts.next(), parts.next()) {
        (Ok(col_or_line), Some(mid), Some(head)) => match mid.parse::<u32>() {
            Ok(line) => (&path[..head.len()], Some(line)),
            Err(_) => (&path[..head.len() + 1 + mid.len()], Some(col_or_line)),
        },
        (Ok(line), Some(head), None) => (&path[..head.len()], Some(line)),
        _ => (path, None),
    }
}
