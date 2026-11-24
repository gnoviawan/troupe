//! `applyPendingLauncherOptions` (`slices/launch.ts`): the mode, model and
//! option values a launcher or an armed agent switch picked, set on the
//! session that now exists. One set command at a time, each failure logged
//! and skipped, so one bad pick never fails the launch or the switch.
//!
//! The run shares `Book::runs`' ids, so the set commands' outcomes come back
//! through [`State::on_run_command`]. Not here: the `restore` form, which the
//! reopened chat's own model restore uses.

use std::collections::VecDeque;

use serde_json::{Map, Value, json};

use super::controls::{MODEL_CATEGORY, category, config_options, flatten};
use super::import::RequestId;
use super::picks::MODE_CATEGORY;
use super::restore::{READVERTISE_WAIT_MS, RunId, options_key, wire_value};
use super::switch::LauncherPicks;
use super::types::{Session, SessionStatus};
use super::{Effect, State};

const SOURCE: &str = "acp.applyPendingLauncherOptions";

/// Who waits for the options to be applied.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum LauncherCaller {
    /// The app's `applyPendingLauncherOptions`, answered `actionResolved`.
    Action(RequestId),
    /// The switch whose new session takes the armed picks.
    Switch(super::switch_run::SwitchId),
}

#[derive(Clone, Debug, PartialEq)]
enum Step {
    Mode,
    /// `setModel` is out; `before` is what a re-advertise compares to.
    NativeModel {
        before: Option<String>,
    },
    Readvertising {
        before: String,
    },
    /// The model went out through the model config option.
    ConfigModel {
        option: String,
    },
    /// The option value `sent` is out; `rest` waits.
    Values {
        sent: Option<String>,
        rest: VecDeque<(String, Value)>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LauncherRun {
    session_id: String,
    picks: LauncherPicks,
    by: LauncherCaller,
    step: Step,
    /// The model config option once the mode was set.
    model_option: Option<Value>,
    /// The option the model pick already set.
    handled: Option<String>,
    /// The picked model was not applied: its scoped values are skipped.
    missed: bool,
}

fn open(session: Option<&Session>) -> bool {
    session.is_some_and(|session| session.status != SessionStatus::Closed)
}

fn option_id(option: &Value) -> String {
    super::js::string_of(option.get("id"))
}

impl State {
    /// `applyPendingLauncherOptions`.
    pub(crate) fn apply_pending_launcher_options(
        &mut self,
        session_id: String,
        picks: Option<LauncherPicks>,
        by: LauncherCaller,
        fx: &mut Vec<Effect>,
    ) {
        let Some(picks) = picks.filter(|_| open(self.sessions.get(&session_id))) else {
            return self.launcher_done(by, fx);
        };
        let run = self.book.next_run();
        let mode = picks.mode_id.clone().filter(|mode| !mode.is_empty());
        self.book.launchers.insert(
            run,
            LauncherRun {
                session_id: session_id.clone(),
                picks,
                by,
                step: Step::Mode,
                model_option: None,
                handled: None,
                missed: false,
            },
        );
        let current = self.sessions[&session_id]
            .rest
            .get("modes")
            .and_then(|modes| modes.get("currentModeId"))
            .and_then(Value::as_str);
        match mode {
            Some(mode) if current != Some(mode.as_str()) => {
                self.set_mode(session_id, mode, Some(run), fx);
            }
            _ => self.launcher_model(run, fx),
        }
    }

    /// A set command the run sent settled; `failure` is its error message.
    pub(crate) fn on_launcher_command(
        &mut self,
        run: RunId,
        failure: Option<String>,
        fx: &mut Vec<Effect>,
    ) {
        let Some(entry) = self.book.launchers.get(&run).cloned() else {
            return;
        };
        let warn = |label: &str, message: &str, fx: &mut Vec<Effect>| {
            fx.push(Effect::warn(
                SOURCE,
                format!(
                    "Skipping option '{label}' on session {} after apply failure: {message}",
                    entry.session_id
                ),
            ));
        };
        match entry.step {
            Step::Mode => {
                if let Some(message) = &failure {
                    warn("mode", message, fx);
                }
                self.launcher_model(run, fx);
            }
            Step::NativeModel { before } => {
                if failure.is_some() {
                    return self.launcher_model_by_option(run, fx);
                }
                let handled = entry.model_option.as_ref().map(option_id);
                let follow = entry
                    .picks
                    .config_values
                    .iter()
                    .flatten()
                    .any(|(id, _)| Some(id) != handled.as_ref());
                let waits = before.filter(|_| follow).filter(|before| {
                    let session = self.sessions.get(&entry.session_id);
                    session.is_some_and(|s| s.status == SessionStatus::Active)
                        && options_key(session) == *before
                });
                let launcher = self.book.launchers.get_mut(&run).expect("read above");
                launcher.handled = handled;
                match waits {
                    Some(before) => {
                        launcher.step = Step::Readvertising { before };
                        fx.push(Effect::WaitForReadvertisedOptions {
                            session_id: entry.session_id,
                            delay_ms: READVERTISE_WAIT_MS,
                            run,
                        });
                    }
                    None => self.launcher_values(run, fx),
                }
            }
            Step::ConfigModel { option } => {
                match &failure {
                    Some(message) => {
                        warn(&option, message, fx);
                        self.launcher_missed(run, fx);
                    }
                    None => {
                        let launcher = self.book.launchers.get_mut(&run).expect("read above");
                        launcher.handled = Some(option);
                    }
                }
                self.launcher_values(run, fx);
            }
            Step::Values { sent: Some(id), .. } => {
                if let Some(message) = &failure {
                    warn(&id, message, fx);
                }
                self.launcher_next_value(run, fx);
            }
            Step::Values { sent: None, .. } | Step::Readvertising { .. } => {}
        }
    }

    /// The re-advertise wait of `run` is over.
    pub(crate) fn poll_launcher_readvertise(
        &mut self,
        expired: Option<RunId>,
        fx: &mut Vec<Effect>,
    ) {
        let ready: Vec<RunId> = self
            .book
            .launchers
            .iter()
            .filter(|(id, launcher)| match &launcher.step {
                Step::Readvertising { before } => {
                    let session = self.sessions.get(&launcher.session_id);
                    expired == Some(**id)
                        || !session.is_some_and(|s| s.status == SessionStatus::Active)
                        || options_key(session) != *before
                }
                _ => false,
            })
            .map(|(id, _)| *id)
            .collect();
        for run in ready {
            self.launcher_values(run, fx);
        }
    }

    /// After the mode: the model, unless the session already shows it.
    fn launcher_model(&mut self, run: RunId, fx: &mut Vec<Effect>) {
        let Some(entry) = self.book.launchers.get(&run).cloned() else {
            return;
        };
        let session = self.sessions.get(&entry.session_id);
        let model_option = session
            .map(config_options)
            .unwrap_or_default()
            .iter()
            .find(|option| category(option) == Some(MODEL_CATEGORY))
            .cloned();
        let launcher = self.book.launchers.get_mut(&run).expect("read above");
        launcher.model_option.clone_from(&model_option);
        let Some(model_id) = entry.picks.model_id.clone().filter(|m| !m.is_empty()) else {
            return self.launcher_values(run, fx);
        };
        let shown = match &model_option {
            Some(option) => option.get("currentValue").cloned(),
            None => session
                .and_then(|s| s.rest.get("models"))
                .and_then(|models| models.get("currentModelId"))
                .cloned(),
        };
        if shown.as_ref().and_then(Value::as_str) == Some(model_id.as_str()) {
            launcher.handled = model_option.as_ref().map(option_id);
            return self.launcher_values(run, fx);
        }
        if !open(session) {
            self.launcher_missed(run, fx);
            return self.launcher_values(run, fx);
        }
        let native = session
            .and_then(|s| s.rest.get("models"))
            .is_some_and(super::js::truthy);
        if native {
            let before = Some(options_key(session));
            launcher.step = Step::NativeModel { before };
            let session_id = entry.session_id.clone();
            return self.set_model(session_id, model_id, None, Some(run), fx);
        }
        self.launcher_model_by_option(run, fx);
    }

    /// The model through the model config option, when there is one.
    fn launcher_model_by_option(&mut self, run: RunId, fx: &mut Vec<Effect>) {
        let Some(entry) = self.book.launchers.get_mut(&run) else {
            return;
        };
        let (Some(option), Some(model_id)) =
            (entry.model_option.clone(), entry.picks.model_id.clone())
        else {
            self.launcher_missed(run, fx);
            return self.launcher_values(run, fx);
        };
        let id = option_id(&option);
        entry.step = Step::ConfigModel { option: id.clone() };
        let session_id = entry.session_id.clone();
        self.set_config_option(session_id, id, json!(model_id), None, Some(run), fx);
    }

    /// The picked model is not there: the agent's default stays.
    fn launcher_missed(&mut self, run: RunId, fx: &mut Vec<Effect>) {
        if let Some(entry) = self.book.launchers.get_mut(&run) {
            entry.missed = true;
            fx.push(Effect::Toast {
                level: "error".into(),
                message: "Selected model is not available in this session".into(),
            });
        }
    }

    fn launcher_values(&mut self, run: RunId, fx: &mut Vec<Effect>) {
        let Some(entry) = self.book.launchers.get_mut(&run) else {
            return;
        };
        let rest = entry
            .picks
            .config_values
            .clone()
            .unwrap_or_else(Map::new)
            .into_iter()
            .collect();
        entry.step = Step::Values { sent: None, rest };
        self.launcher_next_value(run, fx);
    }

    /// Up to the next set command. Skips the option the model set and a value
    /// already current; logs and skips an option or value the session does
    /// not advertise, and a scoped option after a missed model.
    fn launcher_next_value(&mut self, run: RunId, fx: &mut Vec<Effect>) {
        loop {
            let Some(entry) = self.book.launchers.get_mut(&run) else {
                return;
            };
            let Step::Values { sent, rest } = &mut entry.step else {
                return;
            };
            *sent = None;
            let Some((id, value)) = rest.pop_front() else {
                return self.finish_launcher(run, fx);
            };
            if entry.handled.as_deref() == Some(id.as_str()) {
                continue;
            }
            let session_id = entry.session_id.clone();
            let missed = entry.missed;
            let session = self.sessions.get(&session_id);
            if !open(session) {
                return self.finish_launcher(run, fx);
            }
            let skip = |what: &str, fx: &mut Vec<Effect>| {
                fx.push(Effect::warn(
                    SOURCE,
                    format!(
                        "Skipped option '{id}' on session {session_id}: the {what} is not advertised by this session"
                    ),
                ));
            };
            let Some(option) = session
                .map(config_options)
                .unwrap_or_default()
                .iter()
                .find(|option| option.get("id").and_then(Value::as_str) == Some(id.as_str()))
                .cloned()
            else {
                skip("option", fx);
                continue;
            };
            if missed && !matches!(category(&option), Some(MODEL_CATEGORY | MODE_CATEGORY)) {
                skip("model", fx);
                continue;
            }
            let wire = wire_value(&option, &value);
            let boolean = option.get("type").and_then(Value::as_str) == Some("boolean");
            if !boolean
                && !flatten(&option)
                    .iter()
                    .any(|entry| value.as_str() == Some(entry.value))
            {
                skip("value", fx);
                continue;
            }
            let Some(wire) = wire.filter(|wire| option.get("currentValue") != Some(wire)) else {
                continue;
            };
            if let Some(Step::Values { sent, .. }) = self
                .book
                .launchers
                .get_mut(&run)
                .map(|entry| &mut entry.step)
            {
                *sent = Some(id.clone());
            }
            return self.set_config_option(session_id, id, wire, None, Some(run), fx);
        }
    }

    fn finish_launcher(&mut self, run: RunId, fx: &mut Vec<Effect>) {
        if let Some(entry) = self.book.launchers.remove(&run) {
            self.launcher_done(entry.by, fx);
        }
    }

    fn launcher_done(&mut self, by: LauncherCaller, fx: &mut Vec<Effect>) {
        match by {
            LauncherCaller::Action(request) => fx.push(Effect::ActionResolved {
                request,
                value: Value::Null,
            }),
            LauncherCaller::Switch(id) => self.record_switch_marker(id, fx),
        }
    }
}
