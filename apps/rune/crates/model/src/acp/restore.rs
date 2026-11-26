//! Option restores that span several steps: the composer's model pick
//! (`pickModel` in `slices/prompt.ts`) and the heal from the user's saved
//! picks (`reapplyPersistedComposerOptions` and `applyOptionValues` in
//! `helpers.ts`, `savedModelValues` and `savedAgentWideValues` in
//! `composer-options.ts`).
//!
//! The TypeScript functions await a read and then one set command at a time.
//! Here each is a [`Run`] in `Book::runs` that sends one command and waits.
//! The saved record (`Event::ComposerOptionsRead`), a set command's
//! acknowledgement (`Event::ControlSet`) or failure (`Event::ControlFailed`),
//! and the end of the re-advertise wait move it on.

use std::collections::{BTreeMap, VecDeque};

use serde_json::{Map, Value, json};

use super::controls::{MODEL_CATEGORY, category, config_options, flatten, option_count};
use super::picks::{MODE_CATEGORY, PickOptions, unknown_session};
use super::types::{Session, SessionStatus};
use super::{Effect, State};

/// Names one run; set and read effects carry it back.
pub type RunId = u64;

/// `OptionValues`: option id to the saved value, in the record's order.
type Values = Map<String, Value>;

/// Each option's value when a restore looked (`optionValueBaseline`); a later
/// mismatch means something else moved it, and that change wins.
type Baseline = BTreeMap<String, Option<Value>>;

/// How long a native model switch may take to re-advertise the options.
pub(super) const READVERTISE_WAIT_MS: f64 = 1000.0;

#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    session_id: String,
    kind: Kind,
    phase: Phase,
}

#[derive(Clone, Debug, PartialEq)]
enum Kind {
    /// The user picked `model_id`; `restore` is the favorite's snapshot or,
    /// once read, the values saved for that model.
    Pick { model_id: String, restore: Values },
    /// The heal from the saved record, started by an options event.
    Heal {
        config_id: String,
        /// The values when the read started.
        baseline: Baseline,
        model_at_read: Option<String>,
        saved: Value,
        agent_wide: Values,
    },
}

#[derive(Clone, Debug, PartialEq)]
enum Phase {
    /// Waiting for the saved record.
    Reading,
    /// The model command is out. With `before`, the run then waits for the
    /// agent to re-advertise options that differ from it.
    SettingModel {
        before: Option<String>,
    },
    Readvertising {
        before: String,
    },
    /// Setting the values one at a time; `sent` is the option whose command
    /// is out.
    Applying {
        sent: Option<String>,
        rest: VecDeque<(String, Value)>,
        baseline: Baseline,
        model_id: Option<String>,
    },
}

impl Kind {
    fn source(&self) -> &'static str {
        match self {
            Self::Pick { .. } => "acp.pickModel",
            Self::Heal { .. } => "acp.reapplyComposerOptions",
        }
    }

    /// A heal is a restore: it never writes the saved record.
    fn set_options(&self) -> Option<PickOptions> {
        match self {
            Self::Pick { .. } => None,
            Self::Heal { .. } => Some(PickOptions {
                persist: Some(false),
            }),
        }
    }
}

fn current_model(session: Option<&Session>) -> Option<String> {
    session
        .and_then(super::controls::model_snapshot)
        .map(|(model_id, _)| model_id)
}

fn baseline(session: Option<&Session>) -> Baseline {
    session
        .map(config_options)
        .unwrap_or_default()
        .iter()
        .map(|option| {
            (
                super::js::string_of(option.get("id")),
                option.get("currentValue").cloned(),
            )
        })
        .collect()
}

/// `nonModelOptionsKey`: the non-model options as data.
pub(super) fn options_key(session: Option<&Session>) -> String {
    let options: Vec<&Value> = session
        .map(config_options)
        .unwrap_or_default()
        .iter()
        .filter(|option| category(option) != Some(MODEL_CATEGORY))
        .collect();
    serde_json::to_string(&options).unwrap_or_default()
}

fn is_active(session: Option<&Session>) -> bool {
    session.is_some_and(|session| session.status == SessionStatus::Active)
}

fn option_id(option: &Value) -> Option<&str> {
    option.get("id").and_then(Value::as_str)
}

/// `isUsableConfigOption`.
fn usable(option: &Value) -> bool {
    option.get("type").and_then(Value::as_str) == Some("boolean") || option_count(option) > 0
}

/// `wireConfigValue`: a stored boolean back to the type the agent expects.
pub(super) fn wire_value(option: &Value, stored: &Value) -> Option<Value> {
    if option.get("type").and_then(Value::as_str) != Some("boolean") {
        return Some(stored.clone());
    }
    match stored.as_str() {
        Some("true") => Some(json!(true)),
        Some("false") => Some(json!(false)),
        _ => None,
    }
}

fn values_of(value: Option<&Value>) -> Values {
    value
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

/// `savedModelValues`.
fn saved_model_values(saved: &Value, model_id: Option<&str>) -> Values {
    let Some(model_id) = model_id.filter(|id| !id.is_empty()) else {
        return Values::new();
    };
    values_of(
        saved
            .get("modelValues")
            .and_then(|values| values.get(model_id)),
    )
}

/// `savedAgentWideValues`: the saved values that do not follow the model.
/// With no option known yet, the kinds are guessed from the record.
fn saved_agent_wide_values(saved: &Value, options: &[Value]) -> Values {
    let model_values = values_of(saved.get("modelValues"));
    let scoped = |id: &str| {
        model_values
            .values()
            .any(|values| values.as_object().is_some_and(|v| v.contains_key(id)))
    };
    let mut values = Values::new();
    for (id, value) in values_of(saved.get("configValues")) {
        if options.is_empty() {
            if Some(&value) != saved.get("modelId") && !scoped(&id) {
                values.insert(id, value);
            }
            continue;
        }
        let Some(option) = options.iter().find(|o| option_id(o) == Some(id.as_str())) else {
            continue;
        };
        let kind = category(option);
        if kind == Some(MODEL_CATEGORY) {
            continue;
        }
        if super::js::truthy(saved.get("modelValues").unwrap_or(&Value::Null))
            && kind != Some(MODE_CATEGORY)
        {
            continue;
        }
        values.insert(id, value);
    }
    values
}

impl State {
    /// `pickModel`: the model first (the agent re-advertises effort choices
    /// on a switch), then `values` or the values saved for that model. A
    /// newer pick on the chat stops this one at its next step.
    pub(crate) fn pick_model(
        &mut self,
        session_id: String,
        model_id: String,
        values: Option<Values>,
        fx: &mut Vec<Effect>,
    ) {
        let Some(session) = self.sessions.get(&session_id) else {
            fx.push(unknown_session(&session_id));
            return;
        };
        let config_id = self.config_id_for_agent(&session.agent_id);
        let run = self.book.next_run();
        self.book.latest_model_pick.insert(session_id.clone(), run);
        let read = config_id.filter(|_| values.is_none());
        self.book.runs.insert(
            run,
            Run {
                session_id,
                kind: Kind::Pick {
                    model_id,
                    restore: values.unwrap_or_default(),
                },
                phase: Phase::Reading,
            },
        );
        match read {
            Some(config_id) => fx.push(Effect::ReadComposerOptions { config_id, run }),
            None => self.pick_set_model(run, fx),
        }
    }

    /// `reapplyPersistedComposerOptions`: heal a live chat from the saved
    /// record. Skipped while a model pick runs on the chat; a trigger while
    /// a heal runs asks for one more pass after it.
    pub(crate) fn heal_composer_options(
        &mut self,
        session_id: &str,
        config_id: String,
        fx: &mut Vec<Effect>,
    ) {
        if self.book.latest_model_pick.contains_key(session_id) {
            return;
        }
        let healing = self
            .book
            .runs
            .values()
            .any(|run| run.session_id == session_id && matches!(run.kind, Kind::Heal { .. }));
        if healing {
            self.book.heal_rerun.insert(session_id.to_owned());
            return;
        }
        let session = self.sessions.get(session_id);
        let advertises = session.is_some_and(|session| {
            !config_options(session).is_empty()
                || session
                    .rest
                    .get("models")
                    .and_then(|models| models.get("availableModels"))
                    .and_then(Value::as_array)
                    .is_some_and(|models| !models.is_empty())
        });
        if !is_active(session) || !advertises {
            return;
        }
        let run = self.book.next_run();
        self.book.runs.insert(
            run,
            Run {
                session_id: session_id.to_owned(),
                kind: Kind::Heal {
                    config_id: config_id.clone(),
                    baseline: baseline(session),
                    model_at_read: current_model(session),
                    saved: Value::Null,
                    agent_wide: Values::new(),
                },
                phase: Phase::Reading,
            },
        );
        fx.push(Effect::ReadComposerOptions { config_id, run });
    }

    /// The saved record a `ReadComposerOptions` asked for (null when none).
    pub(crate) fn on_composer_options_read(
        &mut self,
        run: RunId,
        saved: Value,
        fx: &mut Vec<Effect>,
    ) {
        let Some(entry) = self.book.runs.get_mut(&run) else {
            return;
        };
        if entry.phase != Phase::Reading {
            return;
        }
        match &mut entry.kind {
            Kind::Pick { model_id, restore } => {
                *restore = saved_model_values(&saved, Some(model_id));
                self.pick_set_model(run, fx);
            }
            Kind::Heal { .. } => self.heal_after_read(run, saved, fx),
        }
    }

    /// A set command a run sent settled: `failure` is the error message it
    /// failed with.
    pub(crate) fn on_run_command(
        &mut self,
        run: RunId,
        failure: Option<String>,
        fx: &mut Vec<Effect>,
    ) {
        if self.book.launchers.contains_key(&run) {
            return self.on_launcher_command(run, failure, fx);
        }
        let Some(entry) = self.book.runs.get(&run) else {
            return;
        };
        let session_id = entry.session_id.clone();
        let source = entry.kind.source();
        let picking = matches!(entry.kind, Kind::Pick { .. });
        match entry.phase.clone() {
            Phase::SettingModel { before } => {
                if let Some(message) = failure {
                    fx.push(if picking {
                        Effect::rejected(message)
                    } else {
                        Effect::warn(
                            source,
                            "failed to re-apply persisted model (details withheld)".into(),
                        )
                    });
                    return self.finish_run(run, fx);
                }
                match before {
                    Some(before) if !self.readvertised(&session_id, &before) => {
                        self.set_phase(run, Phase::Readvertising { before });
                        fx.push(Effect::WaitForReadvertisedOptions {
                            session_id,
                            delay_ms: READVERTISE_WAIT_MS,
                            run,
                        });
                    }
                    _ => self.after_model(run, fx),
                }
            }
            Phase::Applying {
                sent: Some(option),
                rest,
                model_id,
                ..
            } => {
                fx.push(match failure {
                    None => Effect::info(
                        source,
                        format!("restored saved value for '{option}' on session {session_id}"),
                    ),
                    Some(_) => Effect::warn(
                        source,
                        format!(
                            "the agent rejected the saved value for '{option}' on session {session_id}; kept its value (details withheld)"
                        ),
                    ),
                });
                // The set may re-advertise other options; compare later
                // changes to that.
                let baseline = baseline(self.sessions.get(&session_id));
                self.set_phase(
                    run,
                    Phase::Applying {
                        sent: None,
                        rest,
                        baseline,
                        model_id,
                    },
                );
                self.apply_next(run, fx);
            }
            _ => {}
        }
    }

    /// The re-advertise wait ran out (`expired`), or an event may have
    /// re-advertised the options a waiting run compares against.
    pub(crate) fn poll_readvertise(&mut self, expired: Option<RunId>, fx: &mut Vec<Effect>) {
        self.poll_launcher_readvertise(expired, fx);
        let ready: Vec<RunId> = self
            .book
            .runs
            .iter()
            .filter_map(|(id, run)| match &run.phase {
                Phase::Readvertising { before }
                    if expired == Some(*id) || self.readvertised(&run.session_id, before) =>
                {
                    Some(*id)
                }
                _ => None,
            })
            .collect();
        for run in ready {
            self.after_model(run, fx);
        }
    }

    /// `waitForReadvertisedOptions` is over for a chat that left `active` or
    /// whose non-model options changed.
    fn readvertised(&self, session_id: &str, before: &str) -> bool {
        let session = self.sessions.get(session_id);
        !is_active(session) || options_key(session) != before
    }

    fn pick_set_model(&mut self, run: RunId, fx: &mut Vec<Effect>) {
        let Some(Run {
            session_id,
            kind: Kind::Pick { model_id, restore },
            ..
        }) = self.book.runs.get(&run).cloned()
        else {
            return;
        };
        if !self.is_latest_pick(&session_id, run) {
            return self.finish_run(run, fx);
        }
        let Some(live) = self.sessions.get(&session_id) else {
            return self.finish_run(run, fx);
        };
        if current_model(Some(live)).as_deref() == Some(model_id.as_str()) {
            return self.pick_apply(run, fx);
        }
        let model_option = config_options(live)
            .iter()
            .find(|option| category(option) == Some(MODEL_CATEGORY) && usable(option))
            .map(|option| super::js::string_of(option.get("id")));
        let before =
            (model_option.is_none() && !restore.is_empty()).then(|| options_key(Some(live)));
        self.set_phase(run, Phase::SettingModel { before });
        match model_option {
            Some(option) => {
                self.set_config_option(session_id, option, json!(model_id), None, Some(run), fx);
            }
            None => self.set_model(session_id, model_id, None, Some(run), fx),
        }
    }

    fn pick_apply(&mut self, run: RunId, fx: &mut Vec<Effect>) {
        let Some(Run {
            session_id,
            kind: Kind::Pick { model_id, restore },
            ..
        }) = self.book.runs.get(&run).cloned()
        else {
            return;
        };
        if !self.is_latest_pick(&session_id, run) {
            return self.finish_run(run, fx);
        }
        let baseline = baseline(self.sessions.get(&session_id));
        self.start_apply(run, restore, baseline, Some(model_id), fx);
    }

    fn heal_after_read(&mut self, run: RunId, saved: Value, fx: &mut Vec<Effect>) {
        let Some(Run {
            session_id,
            kind:
                Kind::Heal {
                    config_id,
                    baseline,
                    model_at_read,
                    ..
                },
            ..
        }) = self.book.runs.get(&run).cloned()
        else {
            return;
        };
        if !super::js::truthy(&saved) || self.book.latest_model_pick.contains_key(&session_id) {
            return self.finish_run(run, fx);
        }
        let live = self.sessions.get(&session_id);
        let current = current_model(live);
        if current != model_at_read {
            return self.finish_run(run, fx);
        }
        let options = live.map(config_options).unwrap_or_default();
        let agent_wide = saved_agent_wide_values(&saved, options);
        let target = saved
            .get("modelId")
            .and_then(Value::as_str)
            .filter(|target| !target.is_empty() && Some(*target) != current.as_deref())
            .map(str::to_owned);
        let Some(target) = target else {
            let values = merged(agent_wide, saved_model_values(&saved, current.as_deref()));
            return self.start_apply(run, values, baseline, current, fx);
        };
        let model_option = options
            .iter()
            .find(|option| category(option) == Some(MODEL_CATEGORY));
        let native = live.and_then(|session| session.rest.get("models"));
        let advertised = match model_option {
            Some(option) => flatten(option).iter().any(|value| value.value == target),
            None => native
                .and_then(|models| models.get("availableModels"))
                .and_then(Value::as_array)
                .is_some_and(|models| {
                    models
                        .iter()
                        .any(|model| model.get("modelId").and_then(Value::as_str) == Some(&target))
                }),
        };
        if !advertised {
            self.warn_once(
                &session_id,
                &format!("model\0{target}"),
                "acp.reapplyComposerOptions",
                format!(
                    "saved model is not advertised on session {session_id}; kept the session's model and its values"
                ),
                fx,
            );
            return self.start_apply(run, agent_wide, baseline, current, fx);
        }
        // The native model state when the session has one, else the option.
        let via_option = model_option
            .filter(|_| !native.is_some_and(super::js::truthy))
            .map(|option| super::js::string_of(option.get("id")));
        let before = (via_option.is_none()
            && !saved_model_values(&saved, Some(&target)).is_empty())
        .then(|| options_key(live));
        if let Some(entry) = self.book.runs.get_mut(&run) {
            entry.kind = Kind::Heal {
                config_id,
                baseline,
                model_at_read,
                saved,
                agent_wide,
            };
            entry.phase = Phase::SettingModel { before };
        }
        let quiet = Some(PickOptions {
            persist: Some(false),
        });
        match via_option {
            Some(option) => {
                self.set_config_option(session_id, option, json!(target), quiet, Some(run), fx);
            }
            None => self.set_model(session_id, target, quiet, Some(run), fx),
        }
    }

    /// The model is in place (and the options re-advertised, or the wait ran
    /// out).
    fn after_model(&mut self, run: RunId, fx: &mut Vec<Effect>) {
        let Some(entry) = self.book.runs.get(&run).cloned() else {
            return;
        };
        let Kind::Heal {
            saved, agent_wide, ..
        } = entry.kind
        else {
            return self.pick_apply(run, fx);
        };
        let session_id = entry.session_id;
        fx.push(Effect::info(
            "acp.reapplyComposerOptions",
            format!("re-applied persisted model on session {session_id} (#1486/#1479)"),
        ));
        let live = self.sessions.get(&session_id);
        let baseline = baseline(live);
        let model = current_model(live);
        let values = merged(agent_wide, saved_model_values(&saved, model.as_deref()));
        self.start_apply(run, values, baseline, model, fx);
    }

    fn start_apply(
        &mut self,
        run: RunId,
        values: Values,
        baseline: Baseline,
        model_id: Option<String>,
        fx: &mut Vec<Effect>,
    ) {
        self.set_phase(
            run,
            Phase::Applying {
                sent: None,
                rest: values.into_iter().collect(),
                baseline,
                model_id,
            },
        );
        self.apply_next(run, fx);
    }

    /// `applyOptionValues`, up to the next set command: skips the model
    /// option, an option or value the session does not advertise (warned
    /// once), a value already current, and an option that moved off the
    /// baseline. Stops once the chat leaves `model_id`.
    fn apply_next(&mut self, run: RunId, fx: &mut Vec<Effect>) {
        loop {
            let Some(entry) = self.book.runs.get_mut(&run) else {
                return;
            };
            let source = entry.kind.source();
            let set_options = entry.kind.set_options();
            let session_id = entry.session_id.clone();
            let Phase::Applying {
                sent,
                rest,
                baseline,
                model_id,
            } = &mut entry.phase
            else {
                return;
            };
            let Some((id, stored)) = rest.pop_front() else {
                return self.finish_run(run, fx);
            };
            let session = self.sessions.get(&session_id);
            let off_model = model_id.as_deref().is_some_and(|model| {
                !model.is_empty() && current_model(session).as_deref() != Some(model)
            });
            if !is_active(session) || off_model {
                return self.finish_run(run, fx);
            }
            let option = session
                .map(config_options)
                .unwrap_or_default()
                .iter()
                .find(|option| option_id(option) == Some(id.as_str()));
            if option.is_some_and(|option| category(option) == Some(MODEL_CATEGORY)) {
                continue;
            }
            if let (Some(option), Some(seen)) = (option, baseline.get(&id))
                && *seen != option.get("currentValue").cloned()
            {
                continue;
            }
            let wire = option.and_then(|option| wire_value(option, &stored));
            let advertised = match (option, &wire) {
                (Some(option), Some(wire)) => {
                    option.get("type").and_then(Value::as_str) == Some("boolean")
                        || flatten(option)
                            .iter()
                            .any(|value| wire.as_str() == Some(value.value))
                }
                _ => false,
            };
            let (Some(option), Some(wire), true) = (option, wire, advertised) else {
                let shown = super::js::string_of(Some(&stored));
                self.warn_once(
                    &session_id,
                    &format!("{id}\0{shown}"),
                    source,
                    format!(
                        "skipped saved value for '{id}' on session {session_id}: the session does not advertise it"
                    ),
                    fx,
                );
                continue;
            };
            if option.get("currentValue") == Some(&wire) {
                continue;
            }
            *sent = Some(id.clone());
            self.set_config_option(session_id, id, wire, set_options, Some(run), fx);
            return;
        }
    }

    fn finish_run(&mut self, run: RunId, fx: &mut Vec<Effect>) {
        let Some(entry) = self.book.runs.remove(&run) else {
            return;
        };
        match entry.kind {
            Kind::Pick { .. } => {
                if self.is_latest_pick(&entry.session_id, run) {
                    self.book.latest_model_pick.remove(&entry.session_id);
                }
            }
            Kind::Heal { config_id, .. } => {
                if self.book.heal_rerun.remove(&entry.session_id) {
                    self.heal_composer_options(&entry.session_id, config_id, fx);
                }
            }
        }
    }

    fn is_latest_pick(&self, session_id: &str, run: RunId) -> bool {
        self.book.latest_model_pick.get(session_id) == Some(&run)
    }

    fn set_phase(&mut self, run: RunId, phase: Phase) {
        if let Some(entry) = self.book.runs.get_mut(&run) {
            entry.phase = phase;
        }
    }

    /// `warnOnce`: a warn the chat has not logged under `key` yet.
    fn warn_once(
        &mut self,
        session_id: &str,
        key: &str,
        source: &str,
        message: String,
        fx: &mut Vec<Effect>,
    ) {
        if self.book.warned_once.insert(format!("{session_id}\0{key}")) {
            fx.push(Effect::warn(source, message));
        }
    }

    /// `forgetComposerSession`: a removed chat's pick mark and warn keys. A
    /// pick in flight stops at its next step.
    pub(crate) fn forget_composer_session(&mut self, session_id: &str) {
        self.book.latest_model_pick.remove(session_id);
        let prefix = format!("{session_id}\0");
        self.book
            .warned_once
            .retain(|key| !key.starts_with(&prefix));
    }
}

/// `{ ...first, ...second }`: a key in both keeps its place and takes the
/// second value.
fn merged(mut first: Values, second: Values) -> Values {
    first.extend(second);
    first
}
