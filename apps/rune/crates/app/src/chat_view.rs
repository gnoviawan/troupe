//! What a chat tab draws: the transcript of its session from `State::chat` with the chat's
//! composer under it, and the full-window table above everything.

use std::rc::Rc;

use model::workspace::Tab;
use rune::{El, col, row, text};
use theme::Tokens;
use ui::chat::{Lenses, Reader};
use ui::markdown::{Hooks, LinkTarget, TABLE_ZOOM};
use ui::type_::sans;

use crate::State;

pub(crate) fn lenses() -> Lenses<State> {
    Lenses {
        acp: |s: &State| &s.chat.acp,
        reader: |s: &State| &s.chat.reader,
        reader_mut: |s: &mut State| -> &mut Reader { &mut s.chat.reader },
        step: |s: &mut State, e| s.acp_step(e),
        stop_subagent: |s: &mut State, session, id| s.stop_subagent(session, id),
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
    let composer = match pending_question(s, session_id) {
        Some(q) => question_dock(s, t, q, place),
        None => crate::composer_view::chat_dock(s, t, session_id, place),
    };
    let chat = col().w_full().grow(1.0).min_h(0.0).child(body).child(composer);
    let phone = place == ui::composer::view::Place::Phone;
    let frame = row().key(format!("tab-body-{}", tab.id())).w_full().grow(1.0).min_h(0.0);
    Some(match subagent_panel(s, t, session_id) {
        Some(panel) if phone => frame.child(panel.w_full()),
        Some(panel) => frame
            .child(chat.min_w(0.0))
            .child(panel.w_frac(0.38).min_w(320.0).max_w(480.0).border_sides([0.0, 0.0, 0.0, 1.0], c.border)),
        None => frame.child(chat),
    })
}

/// The open subagent's panel (`SubagentSurface`), docked beside the chat.
fn subagent_panel(s: &State, t: &'static Tokens, session_id: &str) -> Option<El> {
    let open = s.chat.reader.subagent_panel(session_id)?;
    let tools = s.chat.acp.tool_calls.get(session_id)?;
    let dispatches: Vec<_> = tools.iter().filter(|t| t.get("subagent").is_some_and(|v| !v.is_null())).collect();
    let index = dispatches.iter().position(|t| t.get("toolCallId").and_then(serde_json::Value::as_str) == Some(open))?;
    let tool = dispatches[index];
    let turn_active = ui::chat::turn_active(&s.chat.acp, session_id);
    let status = ui::chat::subagents::status(tool, turn_active);
    let stopping = status == ui::chat::subagents::Status::Running
        && ui::chat::subagents::subagent_id(tool).is_some_and(|id| s.chat.reader.is_stopping(&id));
    let agent = s.shown_agent_name(&crate::composer::Host::Chat(session_id.to_owned()));
    let spec = ui::chat::subagents::PanelSpec {
        session: session_id,
        tool,
        index,
        total: dispatches.len(),
        turn_active,
        stopping,
        agent_name: agent.as_deref().unwrap_or("The agent"),
    };
    Some(ui::chat::subagents::panel(t, spec, lenses()))
}

/// The oldest question waiting in the chat (`pendingQuestion`).
fn pending_question<'s>(s: &'s State, session_id: &str) -> Option<&'s model::acp::approvals::PendingQuestion> {
    s.chat.acp.pending_questions.values().find(|q| q.session_id == session_id)
}

/// The chat's dock while a question waits: the composer turned into the question card, in the
/// transcript's column, 12 points above and 24 below.
fn question_dock(s: &State, t: &'static Tokens, q: &model::acp::approvals::PendingQuestion, place: ui::composer::view::Place) -> El {
    let phone = place == ui::composer::view::Place::Phone;
    let picked = s.chat.reader.picks.of(&q.question_id);
    col()
        .key(format!("question-dock-{}", q.session_id))
        .w_full()
        .items_center()
        .px(if phone { 16.0 } else { 20.0 })
        .pt(12.0)
        .pb(24.0)
        .child(
            col()
                .w_full()
                .max_w(ui::chat::COLUMN_MAX_W)
                .child(ui::chat::approvals::question_card(t, q, picked, phone, lenses())),
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
