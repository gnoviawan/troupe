//! A switched chat's reopen (`slices/session.ts` `redirectSwitchedReopen`,
//! `respliceLiveSwitchTarget`). A chat whose last switch marker names
//! another session reopens as that session, with every hop's transcript
//! spliced in front of it and its tab remapped. A chat that is itself a
//! switch target gets its sources' transcripts spliced back in front of its
//! own. Each source payload is a fresh read (`loadFreshSourcePayload`): the
//! host's re-read first, then the cached copy.

use serde_json::{Value, json};

use super::history::{OpenId, OpenWaiter};
use super::history_install::project_transcript;
use super::host_error::HostError;
use super::splice::{Band, SPLICE_PREFIX, splice_switch_transcript, with_switch_marker};
use super::transcript::HistoryPayload;
use super::types::SessionStatus;
use super::window::{PayloadRead, PayloadReadOutcome, Reload};
use super::{Effect, State, js};

const REDIRECT: &str = "acp.openHistorySession.switchRedirect";
const SPLICE: &str = "acp.switchAgent.splice";
/// The redirect follows at most this many markers.
const MAX_HOPS: usize = 8;

/// The marker chain one open walks.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Walk {
    Redirect(Redirect),
    Resplice(Resplice),
}

/// `redirectSwitchedReopen`'s walk to the session the conversation ended in.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Redirect {
    /// Each hop's session and transcript, the reopened chat first.
    chain: Vec<(String, Band)>,
    /// The intermediate hops' payloads, whose watermarks count once the
    /// splice applies.
    payloads: Vec<(String, Value)>,
    target: String,
    hops: usize,
}

/// `respliceLiveSwitchTarget`'s walk back through the chat's sources.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Resplice {
    /// Each source's session, payload and band, newest first.
    hops: Vec<(String, Value, Band)>,
    visited: Vec<String>,
    successor: String,
    /// The source being read, and the marker its live link holds.
    source: String,
    live_marker: Option<Value>,
}

/// `resolveSwitchRedirect`: the last marker's new session, when it names one
/// and its config.
fn redirect_target(switches: &[Value]) -> Option<String> {
    let last = switches.last()?;
    let named = |key: &str| {
        last.get(key)
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
    };
    named("toConfigId")?;
    named("newSessionId").map(str::to_owned)
}

/// `restoredSwitches`.
fn restored_switches(payload: &Value) -> Vec<Value> {
    payload
        .get("switches")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|record| {
            js::finite(record.get("seq")).is_some()
                && record
                    .get("id")
                    .and_then(Value::as_str)
                    .is_some_and(|id| !id.is_empty())
        })
        .cloned()
        .collect()
}

fn continued_from(payload: &Value) -> Option<String> {
    payload
        .pointer("/metadata/continuedFrom")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

impl State {
    /// After the install: redirect a chat whose last marker names its
    /// continuation, else re-splice its sources, else go on to its agent.
    pub(crate) fn redirect_switched_reopen(
        &mut self,
        open: OpenId,
        installed: Band,
        fx: &mut Vec<Effect>,
    ) {
        let Some(session_id) = self.open_of(open).map(|o| o.session_id.clone()) else {
            return;
        };
        match redirect_target(&installed.switches).filter(|target| *target != session_id) {
            Some(target) => {
                let walk = Redirect {
                    chain: vec![(session_id, installed)],
                    payloads: Vec::new(),
                    target: target.clone(),
                    hops: 0,
                };
                self.book.history.walks.insert(open, Walk::Redirect(walk));
                self.read_fresh(open, target, fx);
            }
            None => self.resplice_live_switch_target(open, fx),
        }
    }

    /// `loadFreshSourcePayload`: the host's re-read, then the cached copy.
    fn read_fresh(&mut self, open: OpenId, session_id: String, fx: &mut Vec<Effect>) {
        fx.push(Effect::ReloadSessionPayload {
            session_id,
            read: Reload::Walk(open),
        });
    }

    /// The host's re-read answered: its payload, else the cached copy.
    pub(crate) fn on_walk_reload(
        &mut self,
        open: OpenId,
        session_id: &str,
        outcome: PayloadReadOutcome,
        fx: &mut Vec<Effect>,
    ) {
        match outcome {
            Ok(Some(payload)) => self.on_walk_payload(open, Some(payload), fx),
            failed => {
                if let Err(message) = failed {
                    fx.push(Effect::warn(
                        SPLICE,
                        format!(
                            "Host re-read of switch source {session_id} failed; using the cached copy if there is one: {message}"
                        ),
                    ));
                }
                fx.push(Effect::ReadSessionPayload {
                    session_id: session_id.to_owned(),
                    read: PayloadRead::Walk(open),
                });
            }
        }
    }

    /// One source payload of the walk, `None` when it could not be read.
    pub(crate) fn on_walk_payload(
        &mut self,
        open: OpenId,
        payload: Option<Value>,
        fx: &mut Vec<Effect>,
    ) {
        match self.book.history.walks.remove(&open) {
            Some(Walk::Redirect(walk)) => self.redirect_hop(open, walk, payload, fx),
            Some(Walk::Resplice(walk)) => self.resplice_hop(open, walk, payload, fx),
            None => {}
        }
    }

    /// A hop of the redirect: follow its marker, or stop at the session the
    /// conversation ended in.
    fn redirect_hop(
        &mut self,
        open: OpenId,
        mut walk: Redirect,
        payload: Option<Value>,
        fx: &mut Vec<Effect>,
    ) {
        let opening = |state: &Self, id: &str| state.book.history.opening(id);
        let resolved = match payload {
            None => false,
            Some(payload) => {
                let next = redirect_target(&restored_switches(&payload));
                match next {
                    Some(next) if next != walk.target && !opening(self, &next) => {
                        let band = project_transcript(self, &payload).unwrap_or_default();
                        let hop = std::mem::replace(&mut walk.target, next.clone());
                        walk.chain.push((hop.clone(), band));
                        walk.payloads.push((hop, payload));
                        walk.hops += 1;
                        if walk.hops < MAX_HOPS {
                            self.book.history.walks.insert(open, Walk::Redirect(walk));
                            return self.read_fresh(open, next, fx);
                        }
                        false
                    }
                    next => !next.is_some_and(|next| opening(self, &next)),
                }
            }
        };
        let id = walk.chain[0].0.clone();
        if !resolved && opening(self, &walk.target) {
            fx.push(Effect::warn(
                REDIRECT,
                format!(
                    "Switch marker chain for session {id} resolves to in-flight session {} whose own chain cannot be verified; reopening on the original session",
                    walk.target
                ),
            ));
            return self.resplice_live_switch_target(open, fx);
        }
        let target = walk.target.clone();
        self.book.history.walks.insert(open, Walk::Redirect(walk));
        let live = self.sessions.get(&target).is_some_and(|session| {
            !matches!(session.status, SessionStatus::Closed | SessionStatus::Error)
        });
        if live {
            return self.on_redirect_opened(open, &None, fx);
        }
        self.open_history_for(target, None, OpenWaiter::Redirect(open), fx);
    }

    /// The continuation's own open ended: splice the chain under it, or on
    /// failure reopen the original chat as it is.
    pub(crate) fn on_redirect_opened(
        &mut self,
        open: OpenId,
        error: &Option<HostError>,
        fx: &mut Vec<Effect>,
    ) {
        let Some(Walk::Redirect(walk)) = self.book.history.walks.remove(&open) else {
            return;
        };
        let id = walk.chain[0].0.clone();
        let target = walk.target;
        if let Some(error) = error {
            fx.push(Effect::warn(
                REDIRECT,
                format!(
                    "Redirected reopen of session {id} to new session {target} failed; reopening on the original session: {}",
                    super::effect::error_text(error)
                ),
            ));
            return self.resplice_live_switch_target(open, fx);
        }
        if !self.open_of(open).is_some_and(|o| self.open_current(o)) {
            return self.finish_open(open, None, fx);
        }
        for (session_id, payload) in &walk.payloads {
            if let Ok(history) = serde_json::from_value::<HistoryPayload>(payload.clone()) {
                self.note_history_watermark(session_id, &history);
            }
        }
        let moved = self.prompt_queues.get(&id).map_or(0, Vec::len);
        let not_spliced = |id: &str| !id.starts_with(SPLICE_PREFIX);
        let mut band = Band {
            messages: self.messages.remove(&target).unwrap_or_default(),
            tool_calls: self.tool_calls.remove(&target).unwrap_or_default(),
            switches: self.agent_switches.remove(&target).unwrap_or_default(),
        };
        // An active continuation skipped its open, so it may still hold the
        // live splice under other ids; the durable chain replaces it.
        band.messages.retain(|m| not_spliced(&m.id));
        band.tool_calls
            .retain(|call| not_spliced(&js::string_of(call.get("toolCallId"))));
        band.switches
            .retain(|record| not_spliced(&js::string_of(record.get("id"))));
        for (source, hop) in walk.chain.iter().rev() {
            band = splice_switch_transcript(source, hop, band);
        }
        self.install_spliced(&target, band, fx);
        if let Some(queue) = self.prompt_queues.remove(&id).filter(|q| !q.is_empty()) {
            self.prompt_queues
                .entry(target.clone())
                .or_default()
                .extend(queue);
        }
        if self.book.open_chat_tabs.remove(&id) {
            self.book.open_chat_tabs.insert(target.clone());
            fx.push(Effect::RemapChatTab {
                from: id.clone(),
                to: target.clone(),
            });
        }
        if moved > 0 {
            fx.push(Effect::info(
                REDIRECT,
                format!("Moved {moved} queued prompt(s) from switched session {id} to {target}"),
            ));
        }
        self.book.history.flush.push(target);
        self.book.history.redirected.push(open);
    }

    fn install_spliced(&mut self, target: &str, band: Band, fx: &mut Vec<Effect>) {
        self.messages.insert(target.to_owned(), band.messages);
        let mut calls = band.tool_calls;
        super::window::trim_live_tool_calls(&mut calls);
        self.tool_calls.insert(target.to_owned(), calls);
        self.agent_switches.insert(target.to_owned(), band.switches);
        self.trim_live_window(target, fx);
    }

    /// `respliceLiveSwitchTarget`: walk back through the chat's sources, by
    /// the live link first and then the durable `continuedFrom` edge.
    fn resplice_live_switch_target(&mut self, open: OpenId, fx: &mut Vec<Effect>) {
        let Some(o) = self.open_of(open) else {
            return;
        };
        let session_id = o.session_id.clone();
        let edge = continued_from(&o.payload);
        let walk = Resplice {
            hops: Vec::new(),
            visited: vec![session_id.clone()],
            successor: session_id,
            source: String::new(),
            live_marker: None,
        };
        self.resplice_next(open, walk, edge, fx);
    }

    fn resplice_next(
        &mut self,
        open: OpenId,
        mut walk: Resplice,
        edge: Option<String>,
        fx: &mut Vec<Effect>,
    ) {
        let live = self.book.switch.live_sources.get(&walk.successor).cloned();
        let source = live.as_ref().map(|(source, _)| source.clone()).or(edge);
        match source.filter(|source| !walk.visited.contains(source)) {
            Some(source) => {
                walk.source = source.clone();
                walk.live_marker = live.map(|(_, marker)| marker);
                self.book.history.walks.insert(open, Walk::Resplice(walk));
                self.read_fresh(open, source, fx);
            }
            None => self.finish_resplice(open, walk, fx),
        }
    }

    fn resplice_hop(
        &mut self,
        open: OpenId,
        mut walk: Resplice,
        payload: Option<Value>,
        fx: &mut Vec<Effect>,
    ) {
        let target = walk.visited[0].clone();
        let source = walk.source.clone();
        let Some(payload) = payload else {
            fx.push(Effect::info(
                SPLICE,
                format!(
                    "Re-splice for switch target {target} stopped: source session {source} payload unavailable"
                ),
            ));
            return self.finish_resplice(open, walk, fx);
        };
        let installed = project_transcript(self, &payload).unwrap_or_default();
        let successor = walk.successor.clone();
        let marker = walk.live_marker.take().or_else(|| {
            installed
                .switches
                .iter()
                .find(|sw| sw.get("newSessionId").and_then(Value::as_str) == Some(&successor))
                .cloned()
        });
        let marker =
            marker.unwrap_or_else(|| self.fabricated_marker(&source, &successor, &payload));
        let (band, _) = with_switch_marker(installed, &marker);
        let edge = continued_from(&payload);
        walk.hops.push((source.clone(), payload, band));
        walk.visited.push(source.clone());
        walk.successor = source;
        self.resplice_next(open, walk, edge, fx);
    }

    /// A `continuedFrom` edge with no readable marker still splices, under a
    /// boundary made from the index's config ids.
    fn fabricated_marker(&self, source: &str, successor: &str, payload: &Value) -> Value {
        let entry = |id: &str| {
            self.session_index
                .iter()
                .find(|e| e.get("id").and_then(Value::as_str) == Some(id))
        };
        let config = |id: &str| {
            entry(id)
                .and_then(|e| e.get("agentConfigId"))
                .filter(|v| !v.is_null())
                .cloned()
                .unwrap_or(json!(""))
        };
        let mut marker = json!({
            "fromConfigId": config(source),
            "toConfigId": config(successor),
            "newSessionId": successor,
            "summaryText": "",
        });
        let timestamp = entry(successor)
            .and_then(|e| e.get("createdAt"))
            .filter(|v| !v.is_null())
            .or_else(|| payload.pointer("/metadata/lastActivityAt"))
            .filter(|v| !v.is_null());
        if let Some(timestamp) = timestamp {
            marker["timestamp"] = timestamp.clone();
        }
        marker
    }

    fn finish_resplice(&mut self, open: OpenId, walk: Resplice, fx: &mut Vec<Effect>) {
        let current = self.open_of(open).is_some_and(|o| self.open_current(o));
        if !walk.hops.is_empty() && current {
            let target = walk.visited[0].clone();
            for (source, payload, _) in &walk.hops {
                if let Ok(history) = serde_json::from_value::<HistoryPayload>(payload.clone()) {
                    self.note_history_watermark(source, &history);
                }
            }
            let mut band = Band {
                messages: self.messages.remove(&target).unwrap_or_default(),
                tool_calls: self.tool_calls.remove(&target).unwrap_or_default(),
                switches: self.agent_switches.remove(&target).unwrap_or_default(),
            };
            for (source, _, hop) in &walk.hops {
                band = splice_switch_transcript(source, hop, band);
            }
            self.install_spliced(&target, band, fx);
        }
        self.find_history_agent(open, fx);
    }
}
