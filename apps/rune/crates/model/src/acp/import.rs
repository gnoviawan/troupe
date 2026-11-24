//! `listImportableSessions` and `importAgentSessions`
//! (`slices/import-sessions.ts`): the agent's own sessions in a folder that
//! the chat history lacks, and loading the ones the user picked, one at a
//! time.
//!
//! A listing reads `session/list` pages and an import sends one
//! `importAgentSession` per session, both through the sign-in retry. The
//! listing answers its caller with `actionResolved`; the import reports each
//! session as `importProgress` and answers with the outcomes once the index
//! it reloads arrives.

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;
use serde_json::{Value, json};

use super::discovery::SessionList;
use super::ensure::{Caller, EnsureOptions};
use super::host_error::HostError;
use super::reuse_key::agent_reuse_key;
use super::spawn_env::resolve_spawn_project;
use super::{Effect, State};

/// Names one listing or import.
pub type ImportId = u64;

const LIST_SOURCE: &str = "acp.importSessions.list";
const IMPORT_SOURCE: &str = "acp.importSessions.import";

/// `MAX_LIST_PAGES`: a repeated cursor also ends the listing, so the cap
/// only stops a runaway agent.
const MAX_LIST_PAGES: usize = 200;

/// A `session/list` answered with no page. The TypeScript fails with the
/// engine's `TypeError` text, which differs between engines.
const NO_PAGE: &str = "session/list answered with no page";

/// What `importAgentSessions` is asked to load.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportRequest {
    pub agent_id: String,
    pub cwd: String,
    #[serde(default)]
    pub project_id: Option<String>,
    /// The `SessionInfo` records the listing returned.
    pub sessions: Vec<Value>,
}

#[derive(Clone, Debug, PartialEq)]
struct Listing {
    agent_id: String,
    cwd: String,
    project_id: Option<String>,
    known: BTreeSet<String>,
    seen: BTreeSet<String>,
    sessions: Vec<Value>,
    cursors: BTreeSet<String>,
    pages: usize,
}

#[derive(Clone, Debug, PartialEq)]
struct Import {
    request: ImportRequest,
    outcomes: Vec<Value>,
    stop: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ImportBook {
    seq: ImportId,
    listings: BTreeMap<ImportId, Listing>,
    imports: BTreeMap<ImportId, Import>,
    /// Finished imports waiting on their index reload, with their outcomes.
    reloading: BTreeMap<ImportId, Vec<Value>>,
}

impl ImportBook {
    fn next(&mut self) -> ImportId {
        self.seq += 1;
        self.seq
    }
}

fn unavailable() -> Value {
    json!({ "status": "unavailable" })
}

impl State {
    /// `listImportableSessions`: a live agent of this launch lists the
    /// folder's sessions as it is; otherwise `ensureLiveAgent` gets one.
    pub(crate) fn list_importable_sessions(
        &mut self,
        config_id: &str,
        cwd: &str,
        project_id: Option<String>,
        fx: &mut Vec<Effect>,
    ) {
        let cwd = crate::js_trim(cwd).to_owned();
        let project = resolve_spawn_project(&self.book.projects, &cwd, project_id.as_deref());
        let scope = project
            .as_ref()
            .and_then(|project| project.get("id"))
            .map(|id| super::js::string_of(Some(id)));
        let key = agent_reuse_key(config_id, &cwd, scope.as_deref(), None);
        let id = self.book.imports.next();
        self.book.imports.listings.insert(
            id,
            Listing {
                agent_id: String::new(),
                cwd: cwd.clone(),
                project_id: project_id.clone(),
                known: BTreeSet::new(),
                seen: BTreeSet::new(),
                sessions: Vec::new(),
                cursors: BTreeSet::new(),
                pages: 0,
            },
        );
        let live = self
            .config_to_live_agent
            .get(&key)
            .and_then(Value::as_str)
            .filter(|live| !live.is_empty())
            .map(str::to_owned);
        if let Some(live) = live
            && self.agent_status.get(&live) == Some(&json!("connected"))
            && self.runs_current_launch(&live, config_id, &cwd, project_id.as_deref())
        {
            return self.on_import_agent(id, Some(live), fx);
        }
        let options = EnsureOptions {
            project_id,
            ..EnsureOptions::default()
        };
        self.ensure_live_agent(config_id, &cwd, options, Caller::ImportList(id), fx);
    }

    /// The listing has its agent, or none.
    pub(crate) fn on_import_agent(
        &mut self,
        id: ImportId,
        agent_id: Option<String>,
        fx: &mut Vec<Effect>,
    ) {
        let Some(agent_id) = agent_id else {
            self.book.imports.listings.remove(&id);
            fx.push(Effect::warn(
                LIST_SOURCE,
                "Agent unavailable: no sessions listed for import".into(),
            ));
            return fx.push(Effect::ActionResolved {
                value: unavailable(),
            });
        };
        let capabilities = self
            .agents
            .get(&agent_id)
            .and_then(|agent| agent.get("capabilities"));
        let capable = |pointer: &str| {
            capabilities
                .and_then(|caps| caps.pointer(pointer))
                .is_some_and(super::js::truthy)
        };
        if !capable("/loadSession") || !capable("/sessionCapabilities/list") {
            self.book.imports.listings.remove(&id);
            fx.push(Effect::info(
                LIST_SOURCE,
                "Agent cannot list and load sessions: import is not offered".into(),
            ));
            return fx.push(Effect::ActionResolved {
                value: json!({ "status": "unsupported" }),
            });
        }
        let known = self
            .session_index
            .iter()
            .map(|entry| super::js::string_of(entry.get("id")))
            .collect();
        let Some(listing) = self.book.imports.listings.get_mut(&id) else {
            return;
        };
        listing.agent_id = agent_id;
        listing.known = known;
        fx.push(list_page(id, listing, None));
    }

    /// A page came back: keep the sessions the history lacks, then read
    /// the next page or answer.
    pub(crate) fn on_import_list_page(
        &mut self,
        id: ImportId,
        reply: &Value,
        fx: &mut Vec<Effect>,
    ) {
        // Reading a page that is not there throws, which fails the listing.
        if reply.is_null() {
            let error = HostError::new(NO_PAGE.into(), None);
            return self.on_import_list_failed(id, &error, fx);
        }
        let Some(listing) = self.book.imports.listings.get_mut(&id) else {
            return;
        };
        for info in reply
            .get("sessions")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let session_id = super::js::string_of(info.get("sessionId"));
            if listing.seen.insert(session_id.clone()) && !listing.known.contains(&session_id) {
                listing.sessions.push(info.clone());
            }
        }
        listing.pages += 1;
        let truncated = match reply.get("nextCursor").filter(|cursor| !cursor.is_null()) {
            None => None,
            // A repeated cursor would loop forever.
            Some(cursor) if !listing.cursors.insert(cursor.to_string()) => {
                Some("the agent repeated a cursor".to_owned())
            }
            Some(_) if listing.pages == MAX_LIST_PAGES => {
                Some(format!("reached the {MAX_LIST_PAGES}-page cap"))
            }
            Some(cursor) => {
                let cursor = super::js::string_of(Some(cursor));
                return fx.push(list_page(id, listing, Some(cursor)));
            }
        };
        let listing = self.book.imports.listings.remove(&id).expect("found above");
        if let Some(reason) = truncated {
            fx.push(Effect::warn(
                LIST_SOURCE,
                format!(
                    "Stopped listing agent sessions after {}: {reason}",
                    listing.seen.len()
                ),
            ));
        }
        if self.agent_status.get(&listing.agent_id) != Some(&json!("connected")) {
            fx.push(Effect::warn(
                LIST_SOURCE,
                "Agent disconnected while listing sessions for import".into(),
            ));
            return fx.push(Effect::ActionResolved {
                value: unavailable(),
            });
        }
        fx.push(Effect::info(
            LIST_SOURCE,
            format!(
                "Listed {} agent session(s), {} not yet in the chat history",
                listing.seen.len(),
                listing.sessions.len()
            ),
        ));
        fx.push(Effect::ActionResolved {
            value: json!({
                "status": "ready",
                "agentId": listing.agent_id,
                "sessions": listing.sessions,
            }),
        });
    }

    /// The listing's page or agent failed for good: the caller sees the
    /// error.
    pub(crate) fn on_import_list_failed(
        &mut self,
        id: ImportId,
        error: &HostError,
        fx: &mut Vec<Effect>,
    ) {
        if self.book.imports.listings.remove(&id).is_some() {
            fx.push(Effect::rejected_by(error));
        }
    }

    /// `importAgentSessions`.
    pub(crate) fn import_agent_sessions(&mut self, request: ImportRequest, fx: &mut Vec<Effect>) {
        let id = self.book.imports.next();
        self.book.imports.imports.insert(
            id,
            Import {
                request,
                outcomes: Vec::new(),
                stop: false,
            },
        );
        self.import_next(id, fx);
    }

    /// `shouldStop`: the running imports start no other session.
    pub(crate) fn stop_imports(&mut self) {
        for import in self.book.imports.imports.values_mut() {
            import.stop = true;
        }
    }

    pub(crate) fn on_session_imported(
        &mut self,
        id: ImportId,
        session_id: &str,
        fx: &mut Vec<Effect>,
    ) {
        let outcome = json!({ "sessionId": session_id, "status": "imported" });
        self.import_settled(id, outcome, fx);
    }

    /// A failed import leaves nothing behind; one the history already has
    /// is a duplicate.
    pub(crate) fn on_import_session_failed(
        &mut self,
        id: ImportId,
        session_id: String,
        error: HostError,
        fx: &mut Vec<Effect>,
    ) {
        let outcome = if error.code.as_deref() == Some("duplicate") {
            json!({ "sessionId": session_id, "status": "duplicate" })
        } else {
            json!({ "sessionId": session_id, "status": "failed", "message": error.message })
        };
        self.import_settled(id, outcome, fx);
    }

    fn import_settled(&mut self, id: ImportId, outcome: Value, fx: &mut Vec<Effect>) {
        let Some(import) = self.book.imports.imports.get_mut(&id) else {
            return;
        };
        import.outcomes.push(outcome.clone());
        fx.push(Effect::ImportProgress {
            done: import.outcomes.len(),
            outcome,
        });
        self.import_next(id, fx);
    }

    /// Send the next session, or finish: log, then reload the index.
    fn import_next(&mut self, id: ImportId, fx: &mut Vec<Effect>) {
        let Some(import) = self.book.imports.imports.get(&id) else {
            return;
        };
        let request = &import.request;
        let next = request.sessions.get(import.outcomes.len());
        if let Some(info) = next.filter(|_| !import.stop) {
            let cwd = info
                .get("cwd")
                .filter(|cwd| super::js::truthy(cwd))
                .map_or_else(
                    || request.cwd.clone(),
                    |cwd| super::js::string_of(Some(cwd)),
                );
            let updated_at = info
                .get("updatedAt")
                .filter(|at| super::js::truthy(at))
                .and_then(Value::as_str)
                .and_then(date_parse);
            return fx.push(Effect::ImportAgentSession {
                agent_id: request.agent_id.clone(),
                session_id: super::js::string_of(info.get("sessionId")),
                cwd,
                title: info.get("title").cloned(),
                updated_at,
                project_id: request.project_id.clone().filter(|id| !id.is_empty()),
                import: id,
                retried: false,
            });
        }
        let import = self.book.imports.imports.remove(&id).expect("found above");
        let total = import.request.sessions.len();
        let count = |status: &str| {
            import
                .outcomes
                .iter()
                .filter(|outcome| outcome["status"] == status)
                .count()
        };
        let (imported, failed) = (count("imported"), count("failed"));
        let stopped = if import.outcomes.len() < total {
            format!("; stopped, {} not started", total - import.outcomes.len())
        } else {
            String::new()
        };
        let message =
            format!("Imported {imported} of {total} agent session(s); {failed} failed{stopped}");
        fx.push(if failed > 0 {
            Effect::warn(IMPORT_SOURCE, message)
        } else {
            Effect::info(IMPORT_SOURCE, message)
        });
        // Desktop gets no `chat_history` watch delta: reread the index so
        // the imported chats show up.
        fx.push(Effect::RefreshSessionIndex { import: Some(id) });
        self.book.imports.reloading.insert(id, import.outcomes);
    }

    /// The import's own index reload is over, read or failed: the import
    /// answers with its outcomes.
    pub(crate) fn on_import_reloaded(
        &mut self,
        reload: &Effect,
        failure: Option<&str>,
        fx: &mut Vec<Effect>,
    ) {
        let Effect::RefreshSessionIndex { import: Some(id) } = reload else {
            return;
        };
        let Some(outcomes) = self.book.imports.reloading.remove(id) else {
            return;
        };
        if let Some(message) = failure {
            fx.push(Effect::warn(
                "acp.importSessions.refresh",
                format!("Chat history refresh after import failed: Error: {message}"),
            ));
        }
        fx.push(Effect::ActionResolved {
            value: Value::Array(outcomes),
        });
    }
}

fn list_page(id: ImportId, listing: &Listing, cursor: Option<String>) -> Effect {
    Effect::ListSessions {
        agent_id: listing.agent_id.clone(),
        cwd: Some(listing.cwd.clone()),
        cursor,
        project_id: listing.project_id.clone().filter(|id| !id.is_empty()),
        list: SessionList::Import(id),
        retried: false,
    }
}

/// `Date.parse` for the ISO 8601 forms an agent sends: a date (UTC), or a
/// date and time (`T` or a space between) with `Z` or an offset. A time without an offset, which
/// JavaScript reads in the local zone, is not parsed.
pub(crate) fn date_parse(text: &str) -> Option<i64> {
    let number = |range: std::ops::Range<usize>| -> Option<i64> {
        let part = text.get(range)?;
        part.bytes()
            .all(|b| b.is_ascii_digit())
            .then(|| part.parse().ok())
            .flatten()
    };
    let bytes = text.as_bytes();
    if bytes.len() < 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let (year, month, day) = (number(0..4)?, number(5..7)?, number(8..10)?);
    // A day past the month's end rolls over, as the engines do.
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let days = days_from_civil(year, month, day);
    if bytes.len() == 10 {
        return Some(days * 86_400_000);
    }
    if !matches!(bytes[10], b'T' | b't' | b' ') || bytes.len() < 16 || bytes[13] != b':' {
        return None;
    }
    let (hour, minute) = (number(11..13)?, number(14..16)?);
    let mut at = 16;
    let mut second = 0;
    let mut millis = 0;
    if bytes.get(at) == Some(&b':') {
        second = number(at + 1..at + 3)?;
        at += 3;
        if bytes.get(at) == Some(&b'.') {
            let digits = text[at + 1..]
                .bytes()
                .take_while(u8::is_ascii_digit)
                .count();
            if digits == 0 {
                return None;
            }
            let fraction = &text[at + 1..at + 1 + digits.min(3)];
            millis = fraction.parse::<i64>().ok()? * 10_i64.pow(3 - fraction.len() as u32);
            at += 1 + digits;
        }
    }
    if hour > 24 || minute > 59 || second > 59 || (hour == 24 && minute + second + millis > 0) {
        return None;
    }
    let offset = match &text[at..] {
        "Z" => 0,
        zone if zone.len() == 6 && zone.as_bytes()[3] == b':' => {
            let sign = match zone.as_bytes()[0] {
                b'+' => 1,
                b'-' => -1,
                _ => return None,
            };
            let (hours, minutes) = (number(at + 1..at + 3)?, number(at + 4..at + 6)?);
            if hours > 23 || minutes > 59 {
                return None;
            }
            sign * (hours * 60 + minutes)
        }
        _ => return None,
    };
    let seconds = days * 86_400 + hour * 3600 + minute * 60 + second - offset * 60;
    Some(seconds * 1000 + millis)
}

/// Days since 1970-01-01 (Howard Hinnant's `days_from_civil`).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_index = (month + 9) % 12;
    let day_of_year = (153 * month_index + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}
