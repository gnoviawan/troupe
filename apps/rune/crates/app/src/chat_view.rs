//! What a chat tab draws: the transcript of its session from `State::chat` with the chat's
//! composer under it, and the full-window table above everything.

use std::rc::Rc;

use model::workspace::Tab;
use rune::{El, col, text};
use theme::Tokens;
use ui::chat::{Lenses, Reader};
use ui::markdown::{Hooks, LinkTarget, TABLE_ZOOM};
use ui::type_::sans;

use crate::State;

fn lenses() -> Lenses<State> {
    Lenses {
        acp: |s: &State| &s.chat.acp,
        reader: |s: &State| &s.chat.reader,
        reader_mut: |s: &mut State| -> &mut Reader { &mut s.chat.reader },
    }
}

fn hooks(s: &State) -> Hooks<State> {
    let images = s.chat.images.clone();
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
        images: Rc::new(move |src: &str| images.as_ref().and_then(|store| store.get(src))),
    }
}

/// The body of `tab` when the app draws it: a chat's transcript.
pub fn tab_body(s: &State, t: &'static Tokens, tab: &Tab, place: ui::composer::view::Place) -> Option<El> {
    let Tab::AgentChat { session_id, .. } = tab else {
        return None;
    };
    let c = &t.colors;
    let body = if s.chat.loaded.contains(session_id) {
        ui::chat::transcript(t, s, session_id, lenses(), hooks(s))
    } else {
        col().key(format!("chat-loading-{session_id}")).w_full().grow(1.0).min_h(0.0).center().child(
            text("Loading chat…").font(sans(13.0, 400.0, 20.0)).color(c.muted_foreground),
        )
    };
    let composer = crate::composer_view::chat_dock(s, t, session_id, place);
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
    ui::markdown::table_zoom(t, table, true, &hooks(s), |s: &mut State| {
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
