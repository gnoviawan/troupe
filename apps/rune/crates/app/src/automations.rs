//! The automations driver (`AutomationsPanel.tsx`, `use-automations.ts`): while the active
//! project has an Automations tab it reads the project's automations, the selected one's
//! detail and runs, and an open run's steps, on the `/api/automations` routes. Each read keeps
//! only its latest reply and keeps the last good data when a refresh fails. It re-reads on
//! `automation_changed`, and polls an open run's steps while the run works. The actions run
//! the facade's calls with React's toasts.

use std::collections::BTreeMap;

use client::http::{HttpError, HttpRequest, Method};
use model::automations::routes::{Call, Method as RouteMethod};
use model::automations::{Automation, Changed, Detail, Run, RunStatus, Step};
use model::workspace::Tab;
use serde::de::DeserializeOwned;
use serde_json::Value;
use troupe_flow::canvas::FlowState;

use crate::State;
use crate::host::Fetch;

pub const DELETE_CONFIRM: &str = "automation-delete-confirm";
/// `RUNS_PAGE`.
const RUNS_PAGE: u32 = 25;
/// `RUNNING_POLL_MS`, in seconds.
const RUNNING_POLL: f64 = 2.0;

/// What a reply is for. Reads carry the request they answer, so a superseded reply is dropped.
#[derive(Clone, Debug, PartialEq)]
pub enum AutoFetch {
    List { project: String, request: u64 },
    Detail { id: String, request: u64 },
    Runs { id: String, request: u64 },
    Steps { run_id: String, request: u64 },
    Act(Act),
}

/// A button's call, as `AutomationDetail`'s `act` names it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Act {
    Enable,
    RunNow,
    Delete,
    Approve,
    Cancel,
    Seen,
}

impl Act {
    /// The toast's lead when the call fails.
    fn failure(&self) -> &'static str {
        match self {
            Act::Enable => "Could not change the automation",
            Act::RunNow => "Could not start the run",
            Act::Delete => "Could not delete the automation",
            Act::Approve => "Could not approve",
            Act::Cancel => "Could not cancel the run",
            Act::Seen => "Could not mark the failures seen",
        }
    }
}

/// One read: its key, the data for that key, the last error, and the request in flight.
#[derive(Debug)]
pub struct Query<T> {
    pub key: Option<String>,
    pub data: Option<T>,
    pub error: Option<String>,
    request: u64,
    pub pending: bool,
}

impl<T> Default for Query<T> {
    fn default() -> Self {
        Query {
            key: None,
            data: None,
            error: None,
            request: 0,
            pending: false,
        }
    }
}

impl<T> Query<T> {
    /// The data for `key`, else none.
    pub fn of(&self, key: &str) -> Option<&T> {
        (self.key.as_deref() == Some(key))
            .then_some(self.data.as_ref())
            .flatten()
    }

    fn start(&mut self, key: &str, request: u64) {
        if self.key.as_deref() != Some(key) {
            self.data = None;
            self.error = None;
        }
        self.key = Some(key.to_owned());
        self.request = request;
        self.pending = true;
    }

    /// Lands a reply; a failed refresh keeps the last good data on screen.
    fn land(&mut self, request: u64, result: Result<T, String>) -> bool {
        if request != self.request {
            return false;
        }
        self.pending = false;
        match result {
            Ok(data) => {
                self.data = Some(data);
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
        true
    }
}

#[derive(Debug, Default)]
pub struct AutomationsUi {
    /// The project whose automations are read.
    pub project: Option<String>,
    pub list: Query<Vec<Automation>>,
    /// The automation the user picked; none shows the first on desktop, the list on a phone.
    pub chosen: Option<String>,
    pub detail: Query<Detail>,
    pub runs: Query<Vec<Run>>,
    /// The run that fills the pane.
    pub run_id: Option<String>,
    pub steps: Query<Vec<Step>>,
    pub busy: Option<Act>,
    /// The automation the delete confirm names, `(id, name)`, captured when it opened: the
    /// selection may move to another automation before the user confirms.
    pub deleting: Option<(String, String)>,
    pub cancelling: bool,
    /// The detail page's read-only diagram.
    pub diagram: FlowState,
    /// The run view's canvas, its inspected step and whether the step list is open.
    pub run_flow: FlowState,
    pub inspected: Option<String>,
    pub steps_closed: bool,
    /// Loop iterations the run view shows, by for_each id.
    pub picks: BTreeMap<String, u64>,
    /// The detail whose unseen failures were last marked seen, with the count.
    seen: Option<(String, u64)>,
    next_poll: f64,
    requests: u64,
    /// Calls made with no server link (a headless run or a test), oldest first.
    pub(crate) unsent: Vec<(AutoFetch, HttpRequest)>,
}

impl AutomationsUi {
    /// The automation the pane shows: the user's pick while it exists, else the first.
    pub fn selected(&self) -> Option<&str> {
        let list = self.list.data.as_deref();
        match (&self.chosen, list) {
            (Some(id), Some(list)) if list.iter().any(|a| a.id == *id) => Some(id),
            (Some(id), None) => Some(id),
            (_, Some(list)) => list.first().map(|a| a.id.as_str()),
            _ => None,
        }
    }

    pub fn open_run(&self) -> Option<&Run> {
        let id = self.run_id.as_deref()?;
        self.runs.data.as_ref()?.iter().find(|r| r.id == id)
    }

    fn next(&mut self) -> u64 {
        self.requests += 1;
        self.requests
    }
}

fn message(error: HttpError) -> String {
    match error {
        HttpError::Refused { message, .. } => message,
        HttpError::Network(detail) => detail,
        HttpError::InProgress => "the first run of this command is still going".to_owned(),
    }
}

/// A reply parsed at the boundary: a shape the contract does not name is a failure.
pub fn decode<T: DeserializeOwned>(
    operation: &str,
    result: Result<Value, HttpError>,
) -> Result<T, String> {
    let data = result.map_err(|e| {
        let text = message(e);
        log::warn!("[automations] {operation} failed: {text}");
        text
    })?;
    serde_json::from_value(data).map_err(|e| {
        log::warn!("[automations] {operation} returned an unexpected shape: {e}");
        format!("unexpected reply: {e}")
    })
}

impl State {
    /// The project of an Automations tab in the visible workspace, when it is the active one.
    fn automations_project(&self) -> Option<String> {
        let active = self.active_project()?.id.clone();
        self.workspace.record()?;
        let open = model::workspace::leaves(self.workspace.root())
            .into_iter()
            .flat_map(|leaf| leaf.tabs.iter())
            .any(|tab| matches!(tab, Tab::Automations { project_id, .. } if *project_id == active));
        open.then_some(active)
    }

    pub(crate) fn automation_send(&mut self, call: Call, tag: AutoFetch) {
        let route = call.route();
        let request = HttpRequest {
            method: match route.method {
                RouteMethod::Get => Method::Get,
                RouteMethod::Post => Method::Post,
                RouteMethod::Put => Method::Put,
                RouteMethod::Delete => Method::Delete,
            },
            path: route.path,
            body: route.body,
            command_id: None,
        };
        match &mut self.host {
            Some(host) => host.request(request, Fetch::Automation(tag)),
            None => self.automations.unsent.push((tag, request)),
        }
    }

    fn load_list(&mut self, project: &str) {
        let request = self.automations.next();
        self.automations.list.start(project, request);
        self.automation_send(
            Call::List {
                project_id: project.to_owned(),
            },
            AutoFetch::List {
                project: project.to_owned(),
                request,
            },
        );
    }

    fn load_detail(&mut self, id: &str) {
        let request = self.automations.next();
        self.automations.detail.start(id, request);
        self.automation_send(
            Call::Get { id: id.to_owned() },
            AutoFetch::Detail {
                id: id.to_owned(),
                request,
            },
        );
        self.load_runs(id);
    }

    fn load_runs(&mut self, id: &str) {
        let request = self.automations.next();
        self.automations.runs.start(id, request);
        self.automation_send(
            Call::ListRuns {
                id: id.to_owned(),
                limit: RUNS_PAGE,
            },
            AutoFetch::Runs {
                id: id.to_owned(),
                request,
            },
        );
    }

    fn load_steps(&mut self, run_id: &str) {
        let request = self.automations.next();
        self.automations.steps.start(run_id, request);
        self.automation_send(
            Call::RunSteps {
                run_id: run_id.to_owned(),
            },
            AutoFetch::Steps {
                run_id: run_id.to_owned(),
                request,
            },
        );
    }

    /// `reloadAll`: the list, the detail and its runs.
    pub(crate) fn automations_reload(&mut self) {
        if let Some(project) = self.automations.project.clone() {
            self.load_list(&project);
        }
        if let Some(id) = self.automations.detail.key.clone() {
            self.load_detail(&id);
        }
    }

    /// Keeps the reads current; the time of the next poll, if one is due.
    pub(crate) fn automations_tick(&mut self, now: f64) -> Option<f64> {
        let project = self.automations_project();
        if project != self.automations.project {
            let ui = &mut self.automations;
            ui.project = project.clone();
            ui.chosen = None;
            ui.run_id = None;
            ui.list = Query::default();
            ui.detail = Query::default();
            ui.runs = Query::default();
            ui.steps = Query::default();
            ui.diagram = FlowState::default();
            if let Some(project) = &project {
                log::info!("[automations] reading the automations of {project}");
                self.load_list(project);
            }
        }
        project.as_ref()?;
        let selected = self.automations.selected().map(str::to_owned);
        if selected != self.automations.detail.key {
            self.automations.run_id = None;
            self.automations.diagram = FlowState::default();
            match &selected {
                Some(id) => self.load_detail(id),
                None => {
                    self.automations.detail = Query::default();
                    self.automations.runs = Query::default();
                }
            }
        }
        if let Some(run_id) = self.automations.run_id.clone()
            && self.automations.steps.key.as_deref() != Some(&run_id)
        {
            self.automations.run_flow = FlowState::default();
            self.load_steps(&run_id);
            self.automations.next_poll = now + RUNNING_POLL;
        }
        self.mark_seen();
        let working = self
            .automations
            .open_run()
            .is_some_and(|r| r.status == RunStatus::Running && r.parked.is_none());
        if !working {
            return None;
        }
        if now >= self.automations.next_poll && !self.automations.steps.pending {
            // The run's status rides on the runs list, which the host's change event would
            // refresh; read it with the steps so a run that ends shows it.
            if let (Some(run_id), Some(id)) = (
                self.automations.run_id.clone(),
                self.automations.detail.key.clone(),
            ) {
                self.load_steps(&run_id);
                self.load_runs(&id);
            }
            self.automations.next_poll = now + RUNNING_POLL;
        }
        Some(self.automations.next_poll)
    }

    /// The runs list is on screen: its failures count as seen.
    fn mark_seen(&mut self) {
        let ui = &self.automations;
        if ui.run_id.is_some() {
            return;
        }
        let Some(detail) = ui.detail.data.as_ref() else {
            return;
        };
        let (id, unseen) = (
            detail.automation.id.clone(),
            detail.automation.unseen_failures,
        );
        if unseen == 0 || ui.seen == Some((id.clone(), unseen)) {
            return;
        }
        self.automations.seen = Some((id.clone(), unseen));
        self.automation_send(Call::MarkSeen { id }, AutoFetch::Act(Act::Seen));
    }

    /// `automation_changed`; none means a change may have been missed, so everything shown
    /// is read again.
    pub(crate) fn automation_changed(&mut self, change: Option<Changed>) {
        let ui = &self.automations;
        let project_hit = change
            .as_ref()
            .is_none_or(|c| ui.project.as_deref() == Some(c.project_id.as_str()));
        let detail_hit = change
            .as_ref()
            .is_none_or(|c| c.automation_id.is_some() && c.automation_id == ui.detail.key);
        if project_hit && let Some(project) = ui.project.clone() {
            self.load_list(&project);
        }
        if detail_hit && let Some(id) = self.automations.detail.key.clone() {
            self.load_detail(&id);
            if let Some(run_id) = self.automations.run_id.clone() {
                self.load_steps(&run_id);
            }
        }
    }

    /// A reply from the automation routes.
    pub(crate) fn automation_replied(&mut self, tag: AutoFetch, result: Result<Value, HttpError>) {
        let ui = &mut self.automations;
        match tag {
            AutoFetch::List { project, request } => {
                if ui.list.key.as_deref() == Some(&project) {
                    ui.list.land(request, decode("list", result));
                }
            }
            AutoFetch::Detail { id, request } => {
                if ui.detail.key.as_deref() == Some(&id) {
                    ui.detail.land(request, decode("get", result));
                }
            }
            AutoFetch::Runs { id, request } => {
                if ui.runs.key.as_deref() == Some(&id) {
                    ui.runs.land(request, decode("list_runs", result));
                }
            }
            AutoFetch::Steps { run_id, request } => {
                if ui.steps.key.as_deref() == Some(&run_id) {
                    ui.steps.land(request, decode("run_steps", result));
                }
            }
            AutoFetch::Act(act) => self.automation_acted(act, result),
        }
    }

    fn automation_acted(&mut self, act: Act, result: Result<Value, HttpError>) {
        if act == Act::Cancel {
            self.automations.cancelling = false;
        } else if self.automations.busy.as_ref() == Some(&act) {
            self.automations.busy = None;
        }
        match (&act, result) {
            (Act::Seen, Err(e)) => log::warn!("[automations] mark_seen failed: {}", message(e)),
            (Act::Seen, Ok(_)) => {}
            (_, Err(e)) => {
                let text = message(e);
                log::warn!("[automations] {act:?} failed: {text}");
                self.toast_error(&format!("{}: {text}", act.failure()), "");
            }
            (Act::RunNow, Ok(run)) => match serde_json::from_value::<Run>(run) {
                Ok(run) => {
                    log::info!(
                        "[automations] run now automation={} run={}",
                        run.automation_id,
                        run.id
                    );
                    self.automations.run_id = Some(run.id);
                }
                Err(e) => {
                    self.toast_error(&format!("{}: unexpected reply: {e}", act.failure()), "")
                }
            },
            (Act::Delete, Ok(_)) => {
                log::info!(
                    "[automations] deleted automation={:?}",
                    self.automations.detail.key
                );
                self.automations.chosen = None;
                self.automations.run_id = None;
            }
            (Act::Approve, Ok(_)) => log::info!(
                "[automations] approved automation={:?}",
                self.automations.detail.key
            ),
            _ => {}
        }
        if act == Act::Cancel {
            if let Some(id) = self.automations.detail.key.clone() {
                self.load_runs(&id);
            }
            return;
        }
        if act != Act::Seen {
            self.automations_reload();
        }
    }

    /// The rail's and the phone drawer's Automations: the active project's tab.
    pub fn open_automations(&mut self) {
        let Some(id) = self.active_project().map(|p| p.id.clone()) else {
            return;
        };
        self.workspace
            .step(model::workspace::Event::AddAutomationsTab(id, None));
    }

    // The detail's and the run view's actions.

    pub fn automation_select(&mut self, id: Option<String>) {
        self.automations.chosen = id;
        self.automations.run_id = None;
    }

    pub fn automation_open_run(&mut self, run_id: String) {
        self.automations.inspected = None;
        self.automations.picks.clear();
        self.automations.steps_closed = false;
        self.automations.run_id = Some(run_id);
    }

    pub fn automation_close_run(&mut self) {
        self.automations.run_id = None;
    }

    fn act(&mut self, act: Act, call: Call) {
        self.automations.busy = Some(act.clone());
        self.automation_send(call, AutoFetch::Act(act));
    }

    pub fn automation_set_enabled(&mut self, enabled: bool) {
        if let Some(id) = self.automations.detail.key.clone() {
            self.act(Act::Enable, Call::SetEnabled { id, enabled });
        }
    }

    pub fn automation_run_now(&mut self) {
        if let Some(id) = self.automations.detail.key.clone() {
            self.act(Act::RunNow, Call::RunNow { id });
        }
    }

    pub fn automation_approve(&mut self) {
        let Some(detail) = self.automations.detail.data.as_ref() else {
            return;
        };
        let (id, revision) = (detail.automation.id.clone(), detail.automation.revision);
        self.act(Act::Approve, Call::Approve { id, revision });
    }

    pub fn automation_ask_delete(&mut self) {
        let Some(detail) = self.automations.detail.data.as_ref() else {
            return;
        };
        self.automations.deleting =
            Some((detail.automation.id.clone(), detail.automation.name.clone()));
        self.overlays.open(DELETE_CONFIRM);
    }

    pub fn automation_cancel_delete(&mut self) {
        self.automations.deleting = None;
        self.close(DELETE_CONFIRM);
    }

    pub fn automation_confirm_delete(&mut self) {
        self.close(DELETE_CONFIRM);
        if let Some((id, _)) = self.automations.deleting.take() {
            self.act(Act::Delete, Call::Remove { id });
        }
    }

    pub fn automation_cancel_run(&mut self) {
        let Some(run_id) = self.automations.run_id.clone() else {
            return;
        };
        self.automations.cancelling = true;
        self.automation_send(Call::CancelRun { run_id }, AutoFetch::Act(Act::Cancel));
    }
}
