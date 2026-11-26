//! First run on the web client: the server setup gate and wizard (`ServerSetupGate`,
//! `ServerSetupWizard`, `RestartingScreen`), the web access token gate (`web-auth-gate.ts`,
//! `WebTokenGateScreen`) and the first-run onboarding gate (`onboarding-gate.ts`). The web entry
//! probes `/health` before it opens `/ws`: a server in setup mode shows the wizard, a server
//! that refuses this client's token shows the token gate, and only then does the link start.
//! The desktop app has no setup mode and no token gate, so its gates stay off.
//!
//! The token is never logged. The boundary logs name the step, the host and port, and whether
//! a token was issued.

use client::Config;
use client::http::{HttpError, HttpRequest, Method};
use model::setup::{self, Answers, AuthGate, Decision, Facts, HostRead, Submit, Writes};
use rune::{TextEdit, Waker};
use serde_json::{Value, json};

use crate::State;

/// The setup gate (`WebSetupGateStatus`); `Off` on the desktop app and in tests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Gate {
    #[default]
    Off,
    Checking,
    Setup,
    Ok,
}

/// What an HTTP reply answers.
#[derive(Clone, Debug, PartialEq)]
pub enum Ask {
    /// `GET /health` at boot (setup mode, write admission).
    Health,
    /// `GET /setup/status`: the wizard's defaults.
    Status,
    Validate,
    Apply,
    /// `GET /projects` with a token: the boot probe, or a submitted token's check.
    Projects { submitted: bool },
    /// The restarted server answered its new `/health`.
    Restarted,
}

/// A wizard answer the server took: where it comes back, and the token it issued.
#[derive(Clone, Debug, PartialEq)]
pub struct Applied {
    pub host: String,
    pub port: u16,
    pub token: Option<String>,
}

/// Total `/health` attempts before the setup gate opens anyway.
const MAX_PROBES: u32 = 5;
/// The restart handoff's first poll, its interval and when it offers the link.
const RESTART_FIRST_S: f64 = 0.5;
const RESTART_EVERY_S: f64 = 0.75;
const RESTART_SLOW_S: f64 = 30.0;

#[derive(Default)]
pub struct Wizard {
    pub answers: Option<Answers>,
    pub port: TextEdit,
    pub sessions_dir: TextEdit,
    pub projects_file: TextEdit,
    pub interval: TextEdit,
    pub load_error: bool,
    pub config_error: Option<String>,
    pub errors: std::collections::BTreeMap<String, String>,
    pub form_error: Option<String>,
    pub submitting: bool,
    pub advanced: bool,
    pub applied: Option<Applied>,
    pub slow: bool,
    /// When the restart handoff polls next, and when it began.
    poll_at: Option<f64>,
    began: f64,
}

#[derive(Default)]
pub struct FirstRun {
    pub gate: Gate,
    pub auth: AuthGate,
    pub token: TextEdit,
    pub token_error: Option<Submit>,
    pub submitting: bool,
    pub wizard: Wizard,
    pub writes: Writes,
    /// The onboarding decision once it stopped being pending (`useOnboardingGate`'s latch).
    pub latched: Option<Decision>,
    pub step: usize,
    pub landing: bool,
    /// The agent config the agents step picked, by id.
    pub agent: Option<String>,
    probes: u32,
    retry_at: Option<f64>,
    /// This boot's one-time setup token (`?setup=`), for `/setup/*` only.
    setup_token: Option<String>,
    /// The URL hash the route last followed.
    hash: Option<String>,
    /// A submitted token waiting for its check.
    pending_token: Option<String>,
    config: Option<Config>,
    waker: Option<Waker>,
    inbox: Inbox,
    /// Every request sent, for the tests.
    #[cfg(test)]
    pub(crate) sent: Vec<(Ask, HttpRequest)>,
}

type Reply = (Ask, Result<Value, HttpError>);

#[cfg(target_arch = "wasm32")]
type Inbox = std::rc::Rc<std::cell::RefCell<Vec<Reply>>>;
#[cfg(not(target_arch = "wasm32"))]
type Inbox = std::sync::Arc<std::sync::Mutex<Vec<Reply>>>;

fn field_error(errors: &Value) -> std::collections::BTreeMap<String, String> {
    errors
        .as_object()
        .map(|o| o.iter().filter_map(|(k, v)| v.as_str().map(|m| (k.clone(), m.to_owned()))).collect())
        .unwrap_or_default()
}

fn number_text(value: Option<f64>) -> String {
    value.map_or(String::new(), |v| format!("{v}"))
}

impl State {
    /// The web entry's boot: probe `/health` before the link starts.
    pub(crate) fn first_run_start(&mut self, mut config: Config, wake: Waker) {
        let page = page::read();
        if let Some(token) = page.landing_token {
            log::info!("[first-run] took the access token from the landing link");
            config.token = Some(token);
        }
        if page.cleared_hash {
            self.route = crate::Route::from_hash(&page.hash);
        }
        let f = &mut self.first_run;
        f.setup_token = page.setup_token;
        f.config = Some(config);
        f.waker = Some(wake);
        f.gate = Gate::Checking;
        f.auth = AuthGate::Checking;
        self.first_run_send(Ask::Health, HttpRequest::get("/health"));
    }

    fn first_run_send(&mut self, ask: Ask, request: HttpRequest) {
        let token = match &ask {
            Ask::Status | Ask::Validate | Ask::Apply => self.first_run.setup_token.clone(),
            Ask::Projects { submitted: true } => self.first_run.pending_token.clone(),
            _ => self.first_run.config.as_ref().and_then(|c| c.token.clone()),
        };
        let mut config = self.first_run.config.clone().unwrap_or_else(|| Config {
            base_url: self.server.clone(),
            token: None,
            device_id: self.device_id.clone(),
            client_instance_id: String::new(),
            seed: 0,
        });
        config.token = token;
        if let Ask::Restarted = ask {
            config.base_url = request.path.clone();
        }
        #[cfg(test)]
        self.first_run.sent.push((ask.clone(), request.clone()));
        page::send(self.first_run.inbox.clone(), self.first_run.waker.clone(), config, ask, request);
    }

    /// Runs the replies that came back and the timers that fell due; the next wake.
    pub(crate) fn first_run_tick(&mut self, now: f64) -> Option<f64> {
        let replies: Vec<Reply> = page::drain(&self.first_run.inbox);
        for (ask, result) in replies {
            self.first_run_replied(ask, result);
        }
        if self.first_run.retry_at.is_some_and(|at| now >= at) {
            self.first_run.retry_at = None;
            self.first_run_send(Ask::Health, HttpRequest::get("/health"));
        }
        let w = &mut self.first_run.wizard;
        if let (Some(at), Some(applied)) = (w.poll_at, w.applied.clone())
            && now >= at
        {
            if w.began == 0.0 {
                w.began = now;
            }
            w.slow = now - w.began >= RESTART_SLOW_S;
            w.poll_at = Some(now + RESTART_EVERY_S);
            let origin = setup::restart_origin(&applied.host, applied.port, &page::host());
            self.first_run_send(Ask::Restarted, HttpRequest::get(&origin));
        }
        // A live link the host refused for its token: the token screen comes back (#907).
        if self.first_run.gate != Gate::Off
            && self.first_run.auth == AuthGate::Ok
            && matches!(&self.status, client::link::Status::Halted { code, .. } if code.eq_ignore_ascii_case("unauthorized"))
        {
            log::warn!("[first-run] the server refused this client's token; showing the token screen");
            self.first_run.auth = AuthGate::Unauthorized;
        }
        self.onboarding_latch();
        self.follow_hash();
        [self.first_run.retry_at, self.first_run.wizard.poll_at].into_iter().flatten().reduce(f64::min)
    }

    /// The web client's hash router, both ways: a `#/…` the user changed moves the route, and a
    /// route the app changed (a restore returns to the workspace) writes its hash, as React's
    /// `navigate` does.
    fn follow_hash(&mut self) {
        if self.first_run.gate == Gate::Off {
            return;
        }
        let hash = page::hash();
        let last = self.first_run.hash.replace(hash.clone());
        if last.is_some() && last.as_deref() != Some(hash.as_str()) {
            let route = crate::Route::from_hash(&hash);
            log::info!("[app] the hash moved the route to {route:?}");
            if route == crate::Route::Snapshots {
                self.open_snapshots();
            } else {
                self.route = route;
            }
            return;
        }
        let wanted = match &self.route {
            crate::Route::Workspace => "#/".to_owned(),
            crate::Route::Snapshots => "#/snapshots".to_owned(),
            crate::Route::NotFound(path) => format!("#{path}"),
        };
        if crate::Route::from_hash(&hash) != self.route {
            page::set_hash(&wanted);
            self.first_run.hash = Some(wanted);
        }
    }

    pub(crate) fn first_run_replied(&mut self, ask: Ask, result: Result<Value, HttpError>) {
        match ask {
            Ask::Health => self.health_replied(result),
            Ask::Status => self.status_replied(result),
            Ask::Validate => self.validate_replied(result),
            Ask::Apply => self.apply_replied(result),
            Ask::Projects { submitted } => self.projects_replied(submitted, result),
            Ask::Restarted => {
                if result.is_ok()
                    && let Some(applied) = self.first_run.wizard.applied.clone()
                {
                    self.first_run.wizard.poll_at = None;
                    let url = setup::restart_landing_url(&applied.host, applied.port, applied.token.as_deref(), &page::host(), &page::path());
                    log::info!("[first-run] the server is back on {}:{}; going there", applied.host, applied.port);
                    page::go(&url);
                }
            }
        }
    }

    fn health_replied(&mut self, result: Result<Value, HttpError>) {
        let f = &mut self.first_run;
        match result {
            Ok(body) => {
                f.writes = if body["status"] == json!("ok") && body["allowRemoteWrites"] == json!(true) {
                    Writes::Admitted
                } else {
                    Writes::Refused
                };
                if body["setupRequired"] == json!(true) {
                    log::info!("[first-run] /health reports setupRequired; showing the server setup wizard");
                    f.gate = Gate::Setup;
                    self.load_wizard();
                } else {
                    f.gate = Gate::Ok;
                    self.first_run_send(Ask::Projects { submitted: false }, HttpRequest::get("/projects"));
                }
            }
            Err(e) => {
                f.probes += 1;
                if f.probes >= MAX_PROBES {
                    log::warn!("[first-run] /health failed {} times; opening the app ({e})", f.probes);
                    f.gate = Gate::Ok;
                    f.writes = Writes::TimedOut;
                    self.first_run_send(Ask::Projects { submitted: false }, HttpRequest::get("/projects"));
                } else {
                    let delay = (2.0 * 2f64.powi(f.probes as i32 - 1)).min(16.0);
                    log::warn!("[first-run] /health failed (attempt {}/{MAX_PROBES}); retrying in {delay} s: {e}", f.probes);
                    f.retry_at = Some(self.now + delay);
                }
            }
        }
    }

    fn projects_replied(&mut self, submitted: bool, result: Result<Value, HttpError>) {
        let refused = match &result {
            Ok(_) => Ok(()),
            Err(HttpError::Refused { code, .. }) => Err(Some(code.as_str())),
            Err(_) => Err(None),
        };
        let verdict = setup::auth_verdict(refused);
        let f = &mut self.first_run;
        if submitted {
            f.submitting = false;
            match verdict {
                AuthGate::Ok => {
                    log::info!("[first-run] the server accepted the entered token");
                    if let (Some(config), Some(token)) = (f.config.as_mut(), f.pending_token.take()) {
                        config.token = Some(token);
                    }
                    f.token_error = None;
                    f.auth = AuthGate::Ok;
                    self.first_run_connect();
                }
                AuthGate::Unauthorized => {
                    log::info!("[first-run] the server refused the entered token");
                    f.token_error = Some(Submit::Invalid);
                }
                _ => f.token_error = Some(Submit::Unreachable),
            }
            return;
        }
        f.auth = verdict;
        match verdict {
            AuthGate::Unauthorized => log::info!("[first-run] the server refused the boot probe; showing the token screen"),
            _ => {
                if verdict == AuthGate::NetworkError {
                    log::info!("[first-run] the boot probe failed; connecting anyway");
                }
                self.first_run_connect();
            }
        }
    }

    /// Starts the link with the token the gates settled on.
    fn first_run_connect(&mut self) {
        let (Some(config), Some(wake)) = (self.first_run.config.clone(), self.first_run.waker.clone()) else {
            return;
        };
        self.terms.host = Some(crate::term_host::TermHost::connect(config.clone(), wake.clone()));
        self.connect(crate::Host::connect(config, wake));
    }

    /// The token form's Continue (`handleSubmit`). The token is kept as the `#token=` link keeps
    /// it, then checked against a gated route.
    pub(crate) fn submit_token(&mut self) {
        let token = self.first_run.token.text.trim().to_owned();
        if token.is_empty() || self.first_run.submitting {
            self.first_run.token_error = Some(Submit::Empty);
            return;
        }
        page::save_token(&token);
        let f = &mut self.first_run;
        f.token_error = None;
        f.submitting = true;
        f.pending_token = Some(token);
        self.first_run_send(Ask::Projects { submitted: true }, HttpRequest::get("/projects"));
    }

    /// The wizard's load and its Try again.
    pub(crate) fn load_wizard(&mut self) {
        let w = &mut self.first_run.wizard;
        w.load_error = false;
        w.answers = None;
        self.first_run_send(Ask::Status, HttpRequest::get("/setup/status"));
    }

    fn status_replied(&mut self, result: Result<Value, HttpError>) {
        let w = &mut self.first_run.wizard;
        let parsed = result.ok().and_then(|data| {
            let answers = serde_json::from_value::<Answers>(data["defaults"].clone()).ok()?;
            Some((answers, data["configError"].as_str().map(str::to_owned)))
        });
        match parsed {
            Some((answers, config_error)) => {
                let answers = answers.normalized();
                w.port = TextEdit::new(&number_text(answers.port));
                w.sessions_dir = TextEdit::new(&answers.sessions_dir);
                w.projects_file = TextEdit::new(&answers.projects_file);
                w.interval = TextEdit::new(&number_text(answers.update_interval_secs));
                w.config_error = config_error;
                w.answers = Some(answers);
            }
            None => {
                log::warn!("[first-run] setup answer load failed");
                w.load_error = true;
            }
        }
    }

    /// The answers with what the text fields hold.
    fn wizard_answers(&self) -> Option<Answers> {
        let w = &self.first_run.wizard;
        let mut a = w.answers.clone()?;
        a.port = w.port.text.trim().parse::<f64>().ok();
        a.sessions_dir = w.sessions_dir.text.clone();
        a.projects_file = w.projects_file.text.clone();
        a.update_interval_secs = w.interval.text.trim().parse::<f64>().ok();
        Some(a)
    }

    /// A field changed: its error and the form's go.
    pub(crate) fn wizard_edited(&mut self, field: &str) {
        let w = &mut self.first_run.wizard;
        w.errors.remove(field);
        w.form_error = None;
    }

    pub(crate) fn wizard_set_host(&mut self, value: &str) {
        if let Some(a) = self.first_run.wizard.answers.as_mut() {
            a.set_host(value);
        }
        self.wizard_edited("host");
        self.wizard_edited("allowRemoteWrites");
    }

    pub(crate) fn wizard_set_channel(&mut self, value: &str) {
        if let Some(a) = self.first_run.wizard.answers.as_mut() {
            a.update_channel = (value != "none").then(|| value.to_owned());
        }
        self.wizard_edited("updateChannel");
    }

    pub(crate) fn wizard_toggle_remote(&mut self) {
        if let Some(a) = self.first_run.wizard.answers.as_mut() {
            a.allow_remote_writes = !a.allow_remote_writes;
        }
        self.wizard_edited("allowRemoteWrites");
    }

    /// Start Troupe (`handleSubmit`): the port check here, then the server's dry run.
    pub(crate) fn submit_wizard(&mut self) {
        if self.first_run.wizard.submitting {
            return;
        }
        let Some(answers) = self.wizard_answers() else { return };
        let w = &mut self.first_run.wizard;
        w.form_error = None;
        if let Some(error) = answers.port_error() {
            w.errors.insert("port".into(), error.into());
            return;
        }
        w.submitting = true;
        let body = serde_json::to_value(&answers).unwrap_or(Value::Null);
        self.first_run_send(Ask::Validate, HttpRequest { method: Method::Post, path: "/setup/validate".into(), body: Some(body), command_id: None });
    }

    fn validate_replied(&mut self, result: Result<Value, HttpError>) {
        let w = &mut self.first_run.wizard;
        match result {
            Err(_) => {
                w.submitting = false;
                w.form_error = Some("Could not reach the server. Check the connection and try again.".into());
            }
            Ok(verdict) if verdict["ok"] != json!(true) => {
                w.submitting = false;
                w.errors = field_error(&verdict["fieldErrors"]);
            }
            Ok(_) => {
                let body = self.wizard_answers().map(|a| serde_json::to_value(a).unwrap_or(Value::Null));
                self.first_run_send(Ask::Apply, HttpRequest { method: Method::Post, path: "/setup/apply".into(), body, command_id: None });
            }
        }
    }

    fn apply_replied(&mut self, result: Result<Value, HttpError>) {
        let now = self.now;
        let w = &mut self.first_run.wizard;
        w.submitting = false;
        match result {
            Err(e) => {
                let message = match &e {
                    HttpError::Refused { code, message, .. } => {
                        log::warn!("[first-run] setup apply failed (code {code})");
                        message.clone()
                    }
                    other => {
                        log::warn!("[first-run] setup apply failed ({other})");
                        String::new()
                    }
                };
                w.form_error = Some(if message.is_empty() { "The server rejected these settings. Try again.".into() } else { message });
            }
            Ok(reply) if reply["ok"] == json!(false) => w.errors = field_error(&reply["fieldErrors"]),
            Ok(reply) => {
                let applied = Applied {
                    host: reply["host"].as_str().unwrap_or(setup::LOOPBACK).to_owned(),
                    port: reply["port"].as_u64().map_or(8080, |p| p as u16),
                    token: reply["webAuthToken"].as_str().map(str::to_owned),
                };
                log::info!(
                    "[first-run] server setup applied; restarting on {}:{} (web auth token {})",
                    applied.host,
                    applied.port,
                    if applied.token.is_some() { "issued" } else { "not issued" }
                );
                w.applied = Some(applied);
                w.poll_at = Some(now + RESTART_FIRST_S);
                w.began = 0.0;
            }
        }
    }

    /// The facts the onboarding gate reads (`useOnboardingGate`).
    pub(crate) fn onboarding_facts(&self) -> Facts {
        let host_read = match self.settings.host_read {
            model::settings::HostRead::Pending => HostRead::Pending,
            model::settings::HostRead::Found => HostRead::Found,
            model::settings::HostRead::Missing => HostRead::Missing,
            model::settings::HostRead::Failed => HostRead::Failed,
        };
        let writes = if self.first_run.gate == Gate::Off { Writes::Admitted } else { self.first_run.writes };
        Facts {
            host_read,
            completed_at: self.settings.get("onboardingCompletedAt").as_str().map(str::to_owned),
            projects_loaded: self.projects.store.is_loaded,
            project_count: self.projects.store.projects.len(),
            writes,
        }
    }

    fn onboarding_latch(&mut self) {
        if self.first_run.latched.is_none() && self.status == client::link::Status::Live {
            let live = setup::decide(&self.onboarding_facts());
            if live != Decision::Pending {
                log::info!("[onboarding] first run {}", if live == Decision::Show { "shows" } else { "skipped" });
                self.first_run.latched = Some(live);
            }
        }
    }

    /// Whether the first-run onboarding replaces the shell.
    pub(crate) fn onboarding_shown(&self) -> bool {
        self.first_run.latched == Some(Decision::Show)
    }

    pub(crate) fn onboarding_step(&mut self, step: usize) {
        self.first_run.step = step.min(setup::STEPS.len() - 1);
        log::info!("[onboarding] Step {} ({})", self.first_run.step + 1, setup::STEPS[self.first_run.step]);
    }

    /// The project step's Skip or Continue (`landOnboarding`): record completion, leave, land.
    pub(crate) fn finish_onboarding(&mut self, project_id: Option<String>) {
        if self.first_run.landing {
            return;
        }
        self.first_run.landing = true;
        self.set_setting("onboardingCompletedAt", json!(model::snapshots::iso(crate::editor_host::epoch_ms())));
        self.first_run.latched = Some(Decision::Skip);
        let landing = setup::landing(self.first_run.agent.as_deref(), project_id.as_deref());
        log::info!("[onboarding] Completed: landing={landing:?} agent={}", self.first_run.agent.as_deref().unwrap_or("none"));
        if let setup::Landing::Launcher { project_id } = landing {
            self.select_project(&project_id);
        }
    }
}

/// The page the web client runs in: its URL, its storage and its navigation.
#[cfg(target_arch = "wasm32")]
mod page {
    use super::{Ask, Config, HttpRequest, Inbox, Reply, Waker};

    const TOKEN_KEY: &str = "termul.webAuthToken";

    pub struct Read {
        pub setup_token: Option<String>,
        pub landing_token: Option<String>,
        pub hash: String,
        pub cleared_hash: bool,
    }

    fn window() -> Option<web_sys::Window> {
        web_sys::window()
    }

    /// `?setup=` and a `#token=` landing; the token is moved into storage and off the URL.
    pub fn read() -> Read {
        let Some(location) = window().map(|w| w.location()) else {
            return Read { setup_token: None, landing_token: None, hash: String::new(), cleared_hash: false };
        };
        let search = location.search().unwrap_or_default();
        let setup_token = search
            .trim_start_matches('?')
            .split('&')
            .find_map(|pair| pair.strip_prefix("setup="))
            .map(|v| js_sys::decode_uri_component(v).ok().and_then(|s| s.as_string()).unwrap_or_else(|| v.to_owned()))
            .filter(|v| !v.is_empty());
        let hash = location.hash().unwrap_or_default();
        let landing_token = hash
            .strip_prefix("#token=")
            .map(|v| js_sys::decode_uri_component(v).ok().and_then(|s| s.as_string()).unwrap_or_else(|| v.to_owned()))
            .filter(|v| !v.is_empty());
        if let Some(token) = &landing_token {
            save_token(token);
            if let Some(history) = window().and_then(|w| w.history().ok()) {
                let path = location.pathname().unwrap_or_else(|_| "/".into());
                let _ = history.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(&format!("{path}{search}")));
            }
        }
        let cleared = landing_token.is_some();
        Read { setup_token, landing_token, hash: if cleared { String::new() } else { hash }, cleared_hash: cleared }
    }

    pub fn save_token(token: &str) {
        let saved = window().and_then(|w| w.local_storage().ok().flatten()).is_some_and(|s| s.set_item(TOKEN_KEY, token).is_ok());
        if !saved {
            log::warn!("[first-run] could not keep the access token in this browser");
        }
    }

    pub fn host() -> String {
        window().and_then(|w| w.location().hostname().ok()).unwrap_or_default()
    }

    pub fn hash() -> String {
        window().and_then(|w| w.location().hash().ok()).unwrap_or_default()
    }

    pub fn set_hash(hash: &str) {
        if let Some(w) = window() {
            let _ = w.location().set_hash(hash);
        }
    }

    pub fn path() -> String {
        window().and_then(|w| w.location().pathname().ok()).unwrap_or_else(|| "/".into())
    }

    pub fn go(url: &str) {
        if let Some(w) = window() {
            let _ = w.location().assign(url);
        }
    }

    pub fn send(inbox: Inbox, wake: Option<Waker>, config: Config, ask: Ask, request: HttpRequest) {
        wasm_bindgen_futures::spawn_local(async move {
            let result = match ask {
                // `/health` is not a Troupe body; the restart poll is cross-origin, so any
                // answer at all means the server is up.
                Ask::Health => client::web::http_get_bytes(&config, "/health")
                    .await
                    .and_then(|b| serde_json::from_slice(&b).map_err(|e| client::http::HttpError::Network(e.to_string()))),
                Ask::Restarted => ping(&format!("{}/health", config.base_url)).await,
                _ => client::web::http_send(&config, &request).await,
            };
            inbox.borrow_mut().push((ask, result));
            if let Some(wake) = wake {
                wake();
            }
        });
    }

    async fn ping(url: &str) -> Result<serde_json::Value, client::http::HttpError> {
        use wasm_bindgen::JsCast;
        let failed = |e: wasm_bindgen::JsValue| client::http::HttpError::Network(format!("{e:?}"));
        let init = web_sys::RequestInit::new();
        init.set_mode(web_sys::RequestMode::NoCors);
        let request = web_sys::Request::new_with_str_and_init(url, &init).map_err(failed)?;
        let window = window().ok_or_else(|| client::http::HttpError::Network("no window".into()))?;
        let reply = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request)).await.map_err(failed)?;
        reply.dyn_into::<web_sys::Response>().map_err(failed)?;
        Ok(serde_json::Value::Null)
    }

    pub fn drain(inbox: &Inbox) -> Vec<Reply> {
        std::mem::take(&mut *inbox.borrow_mut())
    }
}

/// The desktop app and the tests: no page. Requests wait in the inbox's sender list.
#[cfg(not(target_arch = "wasm32"))]
mod page {
    use super::{Ask, Config, HttpRequest, Inbox, Reply, Waker};

    pub struct Read {
        pub setup_token: Option<String>,
        pub landing_token: Option<String>,
        pub hash: String,
        pub cleared_hash: bool,
    }

    pub fn read() -> Read {
        Read { setup_token: None, landing_token: None, hash: String::new(), cleared_hash: false }
    }

    pub fn save_token(_: &str) {}

    pub fn host() -> String {
        String::new()
    }

    pub fn hash() -> String {
        String::new()
    }

    pub fn set_hash(_: &str) {}

    pub fn path() -> String {
        "/".into()
    }

    pub fn go(url: &str) {
        log::info!("[first-run] would go to {}", url.split('#').next().unwrap_or_default());
    }

    pub fn send(_: Inbox, _: Option<Waker>, _: Config, _: Ask, _: HttpRequest) {}

    pub fn drain(inbox: &Inbox) -> Vec<Reply> {
        inbox.lock().map(|mut i| std::mem::take(&mut *i)).unwrap_or_default()
    }
}
