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
        open_file: |s: &mut State, path| s.open_file_link(&path),
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
    let elicitation = s.chat.acp.pending_elicitations.values().find(|e| e.session_id == *session_id);
    let composer = match (elicitation, pending_question(s, session_id)) {
        (Some(e), _) => elicitation_dock(s, t, e, place),
        (None, Some(q)) => question_dock(s, t, q, place),
        (None, None) => crate::composer_view::chat_dock(s, t, session_id, place),
    };
    let phone = place == ui::composer::view::Place::Phone;
    let mut chat = col().w_full().grow(1.0).min_h(0.0);
    if let Some(plan) = plan_panel(s, t, session_id, phone) {
        chat = chat.child(plan);
    }
    chat = chat.child(body);
    if let Some(consent) = consent_card(s, t, session_id, phone) {
        chat = chat.child(consent);
    }
    let chat = chat.child(composer);
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

/// `PlanPanel` over the transcript, when the agent shared a plan.
fn plan_panel(s: &State, t: &'static Tokens, session_id: &str, phone: bool) -> Option<El> {
    let entries = s.chat.acp.plans.get(session_id).filter(|e| !e.is_empty())?;
    let reader = &s.chat.reader;
    let collapsed = reader.plan_collapsed(session_id, ui::chat::plan::is_settled(entries), phone);
    let opened = |n: usize| reader.is_plan_entry_open(session_id, n);
    Some(ui::chat::plan::panel(t, session_id, entries, collapsed, &opened, phone, lenses()))
}

/// `ChatErrorNotice` for the chat's last error, unless the reader dismissed it.
pub(crate) fn error_notice(s: &State, t: &'static Tokens, session_id: &str) -> Option<El> {
    let session = s.chat.acp.sessions.get(session_id)?;
    let raw = session.last_error.clone().flatten().filter(|e| !e.is_empty())?;
    if s.chat.reader.dismissed_error.get(session_id) == Some(&raw) {
        return None;
    }
    let closed = session.status == model::acp::types::SessionStatus::Closed;
    let busy = session.turn_busy();
    let retry = (!busy && !closed && session.status != model::acp::types::SessionStatus::Error && s.last_user_blocks(session_id).is_some()).then_some("Retry");
    let (details, retried, dismissed) = (session_id.to_owned(), session_id.to_owned(), session_id.to_owned());
    let error = raw.clone();
    let spec = ui::chat::notice::Spec {
        key: format!("chat-error-{session_id}"),
        title: if closed { "This chat stopped." } else { "The last turn did not finish." },
        message: ui::chat::notice::format_agent_error(&raw),
        code: session.last_error_code.flatten(),
        data: session.last_error_data.as_ref().and_then(Option::as_ref),
        details_open: s.chat.reader.error_details.contains(session_id),
        retry,
    };
    let actions = ui::chat::notice::Actions {
        details: Box::new(move |s: &mut State| {
            let open = &mut s.chat.reader.error_details;
            if !open.remove(&details) {
                open.insert(details.clone());
            }
        }),
        retry: Box::new(move |s: &mut State| s.retry_last_turn(&retried)),
        dismiss: Box::new(move |s: &mut State| {
            s.chat.reader.dismissed_error.insert(dismissed.clone(), error.clone());
        }),
    };
    Some(ui::chat::notice::error_notice(t, spec, actions))
}

impl State {
    /// The last user message's blocks, when one has something to send again
    /// (`canRetryLastUserTurn`) and no composer token (whose wire form the composer rebuilds).
    fn last_user_blocks(&self, session_id: &str) -> Option<Vec<serde_json::Value>> {
        let messages = self.chat.acp.messages.get(session_id)?;
        let last = messages.iter().rev().find(|m| m.role == model::acp::types::Role::User)?;
        let text = |b: &serde_json::Value| b.get("text").and_then(serde_json::Value::as_str).unwrap_or("").to_owned();
        let sendable = last.blocks.iter().any(|b| b.get("type").and_then(serde_json::Value::as_str) != Some("text") || !text(b).trim().is_empty());
        let tokens = last.blocks.iter().any(|b| text(b).contains(|c: char| ('\u{E000}'..='\u{E00F}').contains(&c)));
        (sendable && !tokens).then(|| last.blocks.clone())
    }

    /// The notice's Retry for a live chat: send the last user turn again.
    pub(crate) fn retry_last_turn(&mut self, session_id: &str) {
        let Some(blocks) = self.last_user_blocks(session_id) else { return };
        if let Some(error) = self.chat.acp.sessions.get(session_id).and_then(|s| s.last_error.clone().flatten()) {
            self.chat.reader.dismissed_error.insert(session_id.to_owned(), error);
        }
        log::info!("[chat] retry the last turn of {session_id}");
        self.acp_step(model::acp::Event::SendPromptBlocks(session_id.to_owned(), blocks.clone(), Some(model::acp::prompt::SendOptions { display_blocks: Some(blocks) })));
    }
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
                .child(ui::chat::approvals::question_card(t, q, picked, s.chat.reader.picks.other_of(&q.question_id), phone, lenses())),
        )
}

/// `BrowserConsentCard` over the composer while a host ticket for this chat waits (the web
/// client has no browser pane, so the card always hosts it).
fn consent_card(s: &State, t: &'static Tokens, session_id: &str, phone: bool) -> Option<El> {
    let closed = s.chat.acp.sessions.get(session_id).is_none_or(|x| x.status == model::acp::types::SessionStatus::Closed);
    let ticket = s.chat.acp.pending_browser_consents.values().find(|v| v.get("sessionId").and_then(serde_json::Value::as_str) == Some(session_id)).filter(|_| !closed)?;
    let agent = s.shown_agent_name(&crate::composer::Host::Chat(session_id.to_owned()));
    let card = ui::chat::consent::card(t, ticket, agent.as_deref().unwrap_or("The agent"), phone, |s: &mut State, request, option| s.respond_browser_consent(request, option));
    Some(
        col()
            .key(format!("browser-consent-dock-{session_id}"))
            .w_full()
            .items_center()
            .px(if phone { 16.0 } else { 20.0 })
            .pt(12.0)
            .pb(8.0)
            .child(col().w_full().max_w(ui::chat::COLUMN_MAX_W).child(card)),
    )
}

/// The chat's dock while an elicitation waits: the composer turned into the agent's form.
fn elicitation_dock(s: &State, t: &'static Tokens, e: &model::acp::approvals::PendingElicitation, place: ui::composer::view::Place) -> El {
    let phone = place == ui::composer::view::Place::Phone;
    let empty = ui::chat::elicitation::Form::default();
    let form = s.chat.reader.form_of(&e.request_id).unwrap_or(&empty);
    let hooks = ui::chat::elicitation::Hooks {
        toast: |s: &mut State, why: &str| s.toast_error(why, ""),
        open_url: |_s: &mut State, url: &str, tx: &mut rune::Tx| {
            log::info!("[chat] open the elicitation link");
            if let Err(e) = tx.open_url(url) {
                log::warn!("[chat] could not open the elicitation link: {e}");
            }
        },
        form: |s: &mut State, id: &str| s.chat.reader.form(id),
    };
    col()
        .key(format!("elicitation-dock-{}", e.session_id))
        .w_full()
        .items_center()
        .px(if phone { 16.0 } else { 20.0 })
        .pt(12.0)
        .pb(24.0)
        .child(col().w_full().max_w(ui::chat::COLUMN_MAX_W).child(ui::chat::elicitation::prompt(t, e, form, phone, lenses(), hooks)))
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
