//! Installing a server from the MCP catalog (`use-mcp-catalog-install.ts`): describe the form,
//! plan the launch, show it for consent, commit it, save the result into the registry the way
//! `saveCatalogInstall` does, then probe it. The rules and words are `model::settings::mcp_install`;
//! the dialogs are `mcp_install_view`.

use std::collections::BTreeMap;

use client::http::{HttpError, HttpRequest, Method};
use model::acp::Event;
use model::settings::mcp_install::{Failure, Filling, Flow, Probe, Target, missing, split_values};
use rune::TextEdit;
use serde_json::{Value, json};

use crate::State;
use crate::mcp_catalog::CatalogFetch;
use crate::settings_mcp::Action;

/// The overlay key of the install dialogs.
pub const INSTALL: &str = "settings-mcp-install";
const LOG: &str = "[settings.McpCatalogInstall]";

/// What an install request is for, with the flow generation that sent it.
#[derive(Clone, Debug, PartialEq)]
pub enum InstallFetch {
    Describe(u64),
    Plan(u64),
    Commit,
    Cancel(String),
}

#[derive(Default)]
pub struct InstallUi {
    pub flow: Flow,
    /// Bumped by each describe and each dismiss; an answer for an older one is dropped.
    generation: u64,
    /// The text of each text-like field, by field id.
    pub edits: BTreeMap<String, TextEdit>,
    /// The launch digest of the commit being saved, to find the saved entry.
    saving: Option<String>,
    /// The choice field whose menu is open.
    pub choice_open: Option<String>,
}

fn code_of(e: &HttpError) -> String {
    match e {
        HttpError::Refused { code, .. } => code.clone(),
        HttpError::Network(_) => "NETWORK_ERROR".into(),
        HttpError::InProgress => "IN_PROGRESS".into(),
    }
}

fn post(path: &str, body: Value) -> HttpRequest {
    HttpRequest { method: Method::Post, path: path.into(), body: Some(body), command_id: None }
}

impl State {
    fn client_id(&self) -> String {
        self.host.as_ref().map(|h| h.client_id().to_owned()).unwrap_or_default()
    }

    fn install_send(&mut self, request: HttpRequest, tag: InstallFetch) {
        self.catalog_send(request, CatalogFetch::Install(tag));
    }

    fn install_set(&mut self, flow: Flow) {
        let open = !matches!(flow, Flow::Idle);
        self.mcp_ui.install.flow = flow;
        if open && !self.overlays.is_open(INSTALL) {
            self.overlays.open(INSTALL);
        } else if !open && self.overlays.is_open(INSTALL) {
            self.close(INSTALL);
        }
    }

    fn install_fail(&mut self, target: Target, stage: &str, failure: Failure) {
        log::warn!("{LOG} {stage} failed for '{}' {} ({})", target.name, target.version, failure.code());
        self.install_set(Flow::Failed { target, failure });
    }

    fn install_cancel(&mut self, plan_id: &str) {
        let body = json!({ "planId": plan_id, "clientInstanceId": self.client_id() });
        self.install_send(post("/mcp-catalog/install/cancel", body), InstallFetch::Cancel(plan_id.to_owned()));
    }

    /// `start`: Install on a catalog card.
    pub(crate) fn install_start(&mut self, name: &str) {
        if self.mcp_ui.install.flow != Flow::Idle {
            return;
        }
        let row = self.mcp_ui.catalog.search.shown().and_then(|r| r.rows.iter().find(|r| r.name == name)).cloned();
        let Some(target) = row.as_ref().and_then(Target::of) else { return };
        self.install_describe(target);
    }

    fn install_describe(&mut self, target: Target) {
        log::info!("{LOG} describe '{}' {}", target.name, target.version);
        let ui = &mut self.mcp_ui.install;
        ui.generation += 1;
        let body = json!({ "source": { "kind": "catalog", "name": target.name, "version": target.version }, "choice": target.choice });
        let generation = ui.generation;
        self.install_set(Flow::Describing(target));
        self.install_send(post("/mcp-catalog/install/form", body), InstallFetch::Describe(generation));
    }

    fn install_plan(&mut self, filling: Filling) {
        let generation = self.mcp_ui.install.generation;
        let mut body = filling.plan_request();
        body["clientInstanceId"] = json!(self.client_id());
        self.install_set(Flow::Planning(filling));
        self.install_send(post("/mcp-catalog/install/plan", body), InstallFetch::Plan(generation));
    }

    fn install_form(&mut self, filling: Filling, error: Option<String>) {
        self.mcp_ui.install.edits = filling.values.iter().map(|(id, v)| (id.clone(), TextEdit::new(v))).collect();
        self.install_set(Flow::Form { filling, error, field_errors: BTreeMap::new() });
    }

    /// `change` for a switch or a choice.
    pub(crate) fn install_pick(&mut self, id: &str, value: &str) {
        if let Flow::Form { filling, field_errors, .. } = &mut self.mcp_ui.install.flow {
            filling.values.insert(id.to_owned(), value.to_owned());
            field_errors.remove(id);
        }
        if let Some(edit) = self.mcp_ui.install.edits.get_mut(id) {
            edit.replace_text(value);
        }
    }

    /// `change` for a typed field: its error clears.
    pub(crate) fn install_typed(&mut self, id: &str) {
        if let Flow::Form { field_errors, .. } = &mut self.mcp_ui.install.flow {
            field_errors.remove(id);
        }
    }

    /// `submit`: required blanks show their errors; otherwise plan.
    pub(crate) fn install_submit(&mut self) {
        let ui = &mut self.mcp_ui.install;
        let Flow::Form { filling, error, field_errors } = &mut ui.flow else { return };
        for f in &filling.fields {
            if let Some(edit) = ui.edits.get(&f.id) {
                filling.values.insert(f.id.clone(), edit.text.clone());
            }
        }
        let blanks = missing(&filling.fields, &filling.values);
        if !blanks.is_empty() {
            *error = None;
            *field_errors = blanks;
            return;
        }
        let filling = filling.clone();
        self.install_plan(filling);
    }

    /// `approve`: commit the reviewed plan with the secrets.
    pub(crate) fn install_approve(&mut self) {
        let Flow::Consent { filling, plan } = self.mcp_ui.install.flow.clone() else { return };
        log::info!("{LOG} approved '{}' {}", filling.target.name, filling.target.version);
        let (_, secrets) = split_values(&filling.fields, &filling.values);
        let body = json!({ "planId": plan["planId"], "launchDigest": plan["launchDigest"], "secretValues": secrets, "clientInstanceId": self.client_id() });
        self.install_set(Flow::Committing { filling, plan });
        self.install_send(post("/mcp-catalog/install/commit", body), InstallFetch::Commit);
    }

    /// The flow's state as the dialog closes; a plan under review is cancelled on the host.
    fn install_reset(&mut self) {
        let ui = &mut self.mcp_ui.install;
        ui.generation += 1;
        let flow = std::mem::take(&mut ui.flow);
        ui.edits.clear();
        ui.choice_open = None;
        if let Flow::Consent { plan, .. } = flow {
            self.install_cancel(plan["planId"].as_str().unwrap_or_default());
        }
    }

    /// `dismiss`: nothing while the commit is out.
    pub(crate) fn install_dismiss(&mut self) {
        if matches!(self.mcp_ui.install.flow, Flow::Committing { .. }) {
            return;
        }
        self.install_reset();
        if self.overlays.is_open(INSTALL) {
            self.close(INSTALL);
        }
    }

    /// The overlay was popped by Escape or a back press.
    pub(crate) fn install_closed(&mut self) {
        self.install_reset();
    }

    /// Whether Escape must leave the dialog up: the commit is out.
    pub(crate) fn install_busy(&self) -> bool {
        matches!(self.mcp_ui.install.flow, Flow::Committing { .. })
    }

    pub(crate) fn install_start_over(&mut self) {
        let Flow::Failed { target, failure } = self.mcp_ui.install.flow.clone() else { return };
        if failure.can_start_over() {
            self.install_describe(target);
        }
    }

    /// `signIn` on the done step.
    pub(crate) fn install_sign_in(&mut self) {
        let Flow::Done { server, .. } = &self.mcp_ui.install.flow else { return };
        let id = server["id"].as_str().unwrap_or_default().to_owned();
        let project = self.active_project().map(|p| p.id.clone());
        self.acp_step(Event::ConnectMcpOAuth(project, id));
    }

    /// An install answer.
    pub(crate) fn install_fetched(&mut self, tag: InstallFetch, result: Result<Value, HttpError>) {
        let generation = self.mcp_ui.install.generation;
        match tag {
            InstallFetch::Cancel(plan_id) => {
                if let Err(e) = result {
                    log::warn!("{LOG} cancel failed for plan {} ({})", plan_id.chars().take(8).collect::<String>(), code_of(&e));
                }
            }
            InstallFetch::Describe(sent) => {
                let Flow::Describing(target) = self.mcp_ui.install.flow.clone() else { return };
                if sent != generation {
                    return;
                }
                match result {
                    Err(e) => self.install_fail(target, "describe", Failure::Host(code_of(&e))),
                    Ok(reply) if reply["outcome"] == json!("unsupported") => self.install_fail(target, "describe", Failure::Unsupported(reply["reason"].clone())),
                    Ok(reply) => match Filling::new(target.clone(), reply["form"].clone()) {
                        Some(filling) if filling.fields.is_empty() => self.install_plan(filling),
                        Some(filling) => self.install_form(filling, None),
                        None => self.install_fail(target, "describe", Failure::Host("unknown".into())),
                    },
                }
            }
            InstallFetch::Plan(sent) => {
                let current = match &self.mcp_ui.install.flow {
                    Flow::Planning(filling) if sent == generation => Some(filling.clone()),
                    _ => None,
                };
                let Some(filling) = current else {
                    if let Some(plan_id) = result.ok().filter(|r| r["outcome"] == json!("plan")).and_then(|r| r["plan"]["planId"].as_str().map(str::to_owned)) {
                        self.install_cancel(&plan_id);
                    }
                    return;
                };
                match result {
                    Err(HttpError::Refused { code, message, .. }) if code == "VALIDATION_ERROR" => self.install_form_error(filling, message),
                    Err(e) => self.install_fail(filling.target, "plan", Failure::Host(code_of(&e))),
                    Ok(reply) if reply["outcome"] == json!("unsupported") => self.install_fail(filling.target, "plan", Failure::Unsupported(reply["reason"].clone())),
                    Ok(reply) => {
                        log::info!("{LOG} plan ready for '{}'", filling.target.name);
                        self.install_set(Flow::Consent { filling, plan: reply["plan"].clone() });
                    }
                }
            }
            InstallFetch::Commit => {
                let Flow::Committing { filling, plan } = self.mcp_ui.install.flow.clone() else { return };
                let committed = match result {
                    Ok(committed) => committed,
                    Err(e) => {
                        self.install_cancel(plan["planId"].as_str().unwrap_or_default());
                        return match e {
                            HttpError::Refused { code, message, .. } if code == "VALIDATION_ERROR" => self.install_form_error(filling, message),
                            e => self.install_fail(filling.target, "commit", Failure::Host(code_of(&e))),
                        };
                    }
                };
                self.mcp_ui.install.saving = committed["launchDigest"].as_str().map(str::to_owned);
                self.mcp_write(Action::Install, Event::SaveCatalogInstall(committed));
            }
        }
    }

    fn install_form_error(&mut self, filling: Filling, message: String) {
        let error = (!message.is_empty()).then_some(message);
        let values = filling.values.clone();
        self.install_form(filling, error);
        for (id, v) in values {
            self.mcp_ui.install.edits.insert(id, TextEdit::new(v));
        }
    }

    /// The registry write of the commit answered.
    pub(crate) fn install_saved(&mut self, ok: bool) {
        let digest = self.mcp_ui.install.saving.take();
        let Flow::Committing { filling, .. } = self.mcp_ui.install.flow.clone() else { return };
        let server = digest.and_then(|d| self.chat.acp.mcp_servers.iter().rev().find(|s| s.get("catalog").and_then(|c| c.get("launchDigest")).and_then(Value::as_str) == Some(d.as_str())).cloned());
        let Some(server) = server.filter(|_| ok) else {
            return self.install_fail(filling.target, "save", Failure::Save);
        };
        let id = server["id"].as_str().unwrap_or_default().to_owned();
        log::info!("{LOG} saved '{}' as {id}; probing", filling.target.name);
        self.install_set(Flow::Probing { target: filling.target, server: Value::Object(server) });
        self.acp_step(Event::ProbeMcpServer(id));
        self.install_follow();
    }

    /// `probeInstalled`: once the probe settles, the done step reads its result.
    pub(crate) fn install_follow(&mut self) {
        let Flow::Probing { target, server } = &self.mcp_ui.install.flow else { return };
        let id = server["id"].as_str().unwrap_or_default();
        let acp = &self.chat.acp;
        if self.mcp_flag(&acp.mcp_probing, id) {
            return;
        }
        let status = acp.mcp_probe_status.get(id).and_then(Value::as_str);
        let tools = acp.mcp_tools.get(id).and_then(Value::as_array).map_or(0, Vec::len);
        let error = acp.mcp_probe_error.get(id).and_then(Value::as_str);
        let stdio = server.get("type").and_then(Value::as_str).unwrap_or("stdio") == "stdio";
        let probe = Probe::read(status, tools, error, stdio);
        let (target, server) = (target.clone(), server.clone());
        self.install_set(Flow::Done { target, server, probe });
    }
}

#[cfg(test)]
#[path = "mcp_install_tests.rs"]
mod tests;
