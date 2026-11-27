//! The Accounts page's dialogs: Rename (`AccountNameDialog.tsx`), the Remove confirm, the
//! automatic switching risk warning (`AutoSwitchRiskDialog`), and Add account
//! (`AddAccountFlow.tsx` with the Claude link sign-in of `LinkSignInStep.tsx`). A sign-in code is
//! a credential: it goes to the host and is never logged or kept after the step ends.

use model::settings::accounts::{self as rules, Provider, Readiness};
use rune::{El, TextEdit, Tx, col, row, text};
use serde_json::{Value, json};
use theme::Tokens;
use troupe_protocol::ws::{WsError, WsRequestType};
use ui::overlay::Dialog;
use ui::settings::{Tone, toned_notice};
use ui::type_::{mono, sans};
use ui::{Button, ButtonSize, ButtonVariant, Input};

use crate::State;
use crate::settings_accounts::{AccountsCall, Act, Target, kind};
use crate::sync::Pending;

const RENAME: &str = "account-rename";
const REMOVE: &str = "account-remove";
const RISK: &str = "account-risk";
const ADD: &str = "account-add";
/// `MAX_NAME_CHARS`.
const MAX_NAME_CHARS: usize = 40;
/// `NAME_SUGGESTIONS`.
const NAME_SUGGESTIONS: [&str; 3] = ["Work", "Personal", "Team"];

/// The Claude link sign-in, as the user moves through it.
#[derive(Clone, Debug, PartialEq)]
pub enum Link {
    Starting,
    Awaiting { url: String, error: Option<String> },
    Submitting { url: String },
}

/// `AddAccountStep`.
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    Name,
    Signing(Link),
    Done,
    Failed(String),
}

pub struct AddFlow {
    pub provider: Provider,
    pub step: Step,
    pub name: TextEdit,
    pub error: Option<String>,
    pub busy: bool,
    pub account: Option<Target>,
    pub code: TextEdit,
}

/// `cleanCode`: the spaces and line breaks a copy picks up go.
fn clean_code(pasted: &str) -> String {
    pasted.chars().filter(|c| !c.is_whitespace()).collect()
}

impl State {
    pub(crate) fn accounts_open_add(&mut self, p: Provider) {
        log::info!("[settings.Accounts] {} add account", p.id());
        self.accounts_ui.add = Some(AddFlow { provider: p, step: Step::Name, name: TextEdit::new(""), error: None, busy: false, account: None, code: TextEdit::new("") });
    }

    /// Whether `id`'s link sign-in runs in the Add dialog.
    pub(crate) fn accounts_signing_in(&self, id: &str) -> bool {
        self.accounts_ui.add.as_ref().is_some_and(|a| matches!(a.step, Step::Signing(_)) && a.account.as_ref().is_some_and(|t| t.id == id))
    }

    /// The flow's Cancel: a link sign-in still waiting is stopped on the host.
    pub(crate) fn accounts_close_add(&mut self) {
        let Some(flow) = self.accounts_ui.add.take() else { return };
        if let (Step::Signing(_), Some(t)) = (&flow.step, &flow.account) {
            self.call(WsRequestType::CancelClaudeLogin, json!({ "accountId": t.id }), Pending::Accounts(AccountsCall::Cancel(t.id.clone())));
        }
    }

    /// `submitName`.
    pub(crate) fn accounts_submit_name(&mut self) {
        let Some(flow) = &mut self.accounts_ui.add else { return };
        let name = flow.name.text.trim().to_owned();
        if name.is_empty() {
            flow.error = Some("Enter a name.".into());
            return;
        }
        flow.busy = true;
        flow.error = None;
        let p = flow.provider;
        self.command(kind(p, Act::Add), json!({ "name": name }), Pending::Accounts(AccountsCall::Add(p)));
    }

    /// A row's Sign in: a Claude account signs in with the link in the Add dialog's sign-in step.
    pub(crate) fn accounts_sign_in(&mut self, target: Target) {
        if target.provider != Provider::Claude {
            self.toast_later(&format!("Signing in to a {} account", target.provider.name()));
            return;
        }
        let provider = target.provider;
        self.accounts_ui.add = Some(AddFlow { provider, step: Step::Name, name: TextEdit::new(&target.name), error: None, busy: false, account: Some(target), code: TextEdit::new("") });
        self.accounts_start_sign_in();
    }

    pub(crate) fn accounts_start_sign_in(&mut self) {
        let Some(flow) = &mut self.accounts_ui.add else { return };
        let Some(t) = flow.account.clone() else { return };
        if flow.provider != Provider::Claude {
            flow.step = Step::Failed(format!("Signing in to a {} account is not in this client yet.", flow.provider.name()));
            return;
        }
        flow.step = Step::Signing(Link::Starting);
        flow.code = TextEdit::new("");
        self.call(WsRequestType::StartClaudeLinkLogin, json!({ "accountId": t.id }), Pending::Accounts(AccountsCall::LinkStart(t.id)));
    }

    /// `submit` of the link step.
    pub(crate) fn accounts_finish_link(&mut self) {
        let Some(flow) = &mut self.accounts_ui.add else { return };
        let Step::Signing(Link::Awaiting { url, .. }) = &flow.step else { return };
        let url = url.clone();
        let code = clean_code(&flow.code.text);
        if code.is_empty() {
            flow.step = Step::Signing(Link::Awaiting { url, error: Some("Paste the code from the Claude page.".into()) });
            return;
        }
        let Some(t) = flow.account.clone() else { return };
        flow.step = Step::Signing(Link::Submitting { url });
        log::info!("[claudeAccounts.finishLinkLogin] finishing the link sign-in of {}", t.id);
        self.command(WsRequestType::FinishClaudeLinkLogin, json!({ "accountId": t.id, "code": code }), Pending::Accounts(AccountsCall::LinkFinish(t.id)));
    }

    pub(crate) fn accounts_add_make_default(&mut self) {
        let Some(t) = self.accounts_ui.add.take().and_then(|f| f.account) else { return };
        self.accounts_make_default(t);
    }

    pub(crate) fn accounts_add_answered(&mut self, call: AccountsCall, result: Result<Option<Value>, WsError>) {
        if let AccountsCall::Cancel(_) = call {
            return;
        }
        let Some(flow) = &mut self.accounts_ui.add else { return };
        let ours = |id: &str| flow.account.as_ref().is_some_and(|t| t.id == id);
        match (call, result) {
            (AccountsCall::Add(p), Ok(Some(account))) => {
                flow.busy = false;
                let t = Target { provider: p, id: account["id"].as_str().unwrap_or_default().to_owned(), name: account["name"].as_str().unwrap_or_default().to_owned() };
                log::info!("[settings.Accounts] {} account {} added", p.id(), t.id);
                flow.account = Some(t);
                self.accounts_reload(p);
                self.accounts_start_sign_in();
            }
            (AccountsCall::Add(p), result) => {
                flow.busy = false;
                let message = result.err().map_or_else(|| "The host sent no account".to_owned(), |e| e.message);
                log::warn!("[{}Accounts.add] add failed: {message}", p.id());
                flow.error = Some(message);
            }
            (AccountsCall::LinkStart(id), result) if ours(&id) && flow.step == Step::Signing(Link::Starting) => match result {
                Ok(reply) => match reply.as_ref().and_then(|r| r["url"].as_str()) {
                    Some(url) => flow.step = Step::Signing(Link::Awaiting { url: url.to_owned(), error: None }),
                    None => flow.step = Step::Failed("The host did not send a sign-in link.".into()),
                },
                Err(e) => {
                    log::warn!("[claudeAccounts.startLinkLogin] Claude account startLinkLogin failed: {}", e.code);
                    flow.step = Step::Failed(e.message);
                }
            },
            (AccountsCall::LinkFinish(id), result) if ours(&id) => {
                let Step::Signing(Link::Submitting { url }) = flow.step.clone() else { return };
                flow.code = TextEdit::new("");
                match result {
                    Ok(_) => {
                        flow.step = Step::Done;
                        let p = flow.provider;
                        self.accounts_reload(p);
                        self.accounts_usage(p, true);
                    }
                    Err(e) if e.code == "CLAUDE_LOGIN_CODE_INCOMPLETE" => flow.step = Step::Signing(Link::Awaiting { url, error: Some(e.message) }),
                    Err(e) if e.code == "CLAUDE_LOGIN_BUSY" => {
                        flow.step = Step::Signing(Link::Awaiting { url, error: Some("Still checking the last code. Wait a moment, then finish sign-in again.".into()) });
                    }
                    Err(e) => {
                        log::warn!("[claudeAccounts.finishLinkLogin] Claude account finishLinkLogin failed: {}", e.code);
                        flow.step = Step::Failed(e.message);
                    }
                }
            }
            _ => {}
        }
    }

    /// The projects that move to the default account when `target` goes: the ones that chose it.
    pub(crate) fn accounts_moving(&self, target: &Target) -> Vec<String> {
        self.projects_by_account(target.provider).remove(&target.id).unwrap_or_default()
    }
}

/// `AccountSteps`: "1 Name — 2 Sign in — 3 Done".
fn steps(t: &Tokens, step: &Step) -> El {
    let c = &t.colors;
    let at = match step {
        Step::Name => 0,
        Step::Done => 2,
        _ => 1,
    };
    ["Name", "Sign in", "Done"].iter().enumerate().fold(row().key("steps").role("list").label("Steps").gap(6.0).items_center(), |r, (i, label)| {
        let mark = if i < at { "✓".to_owned() } else { (i + 1).to_string() };
        let dot = row().key(format!("step-{i}")).w(16.0).h(16.0).radius(8.0).items_center().justify_center().border(1.0, if i == at { c.muted_foreground } else { c.border }).fill(if i < at { c.foreground } else if i == at { c.muted } else { c.background }).child(text(mark).font(sans(9.0, 600.0, 12.0)).color(if i < at { c.background } else if i == at { c.foreground } else { c.muted_foreground }));
        let mut r = r.child(dot).child(text(*label).font(sans(11.0, 400.0, 15.0)).color(if i <= at { c.secondary_foreground } else { c.muted_foreground }));
        if i < 2 {
            r = r.child(rune::el().key(format!("rule-{i}")).w(16.0).h(1.0).fill(c.border).a11y_hidden());
        }
        r
    })
}

/// `AccountSummary`: the new account's plan, usage and default.
fn summary(s: &State, t: &Tokens, flow: &AddFlow) -> El {
    let c = &t.colors;
    let g = s.account_group(flow.provider);
    let id = flow.account.as_ref().map(|a| a.id.as_str()).unwrap_or_default();
    let usage = g.usage.get(id);
    let plan_key = if flow.provider == Provider::Claude { "subscriptionType" } else { "planType" };
    let plan = rules::plan_label(usage.and_then(|u| u[plan_key].as_str())).unwrap_or_else(|| "—".into());
    let primary = rules::claude_primary(usage);
    let (label, value) = primary.map_or(("Usage".to_owned(), "not started".to_owned()), |m| (m.label, format!("{}% used", m.percent.round() as i64)));
    let default_id = g.snapshot.as_ref().and_then(|v| v["defaultAccountId"].as_str()).unwrap_or_default();
    let default_name = g.snapshot.as_ref().and_then(|v| v["accounts"].as_array()).and_then(|a| a.iter().find(|x| x["id"] == json!(default_id))).and_then(|x| x["name"].as_str());
    let default = default_name.map_or("No".to_owned(), |n| format!("No — {n} stays default"));
    let line = |key: &str, k: String, v: String| row().key(key.to_owned()).w_full().gap(16.0).child(text(k).font(sans(11.0, 400.0, 15.0)).color(c.muted_foreground)).child(rune::el().key("gap").grow(1.0)).child(text(v).font(sans(11.0, 400.0, 15.0)).color(c.secondary_foreground));
    col()
        .key("summary")
        .w_full()
        .gap(8.0)
        .padding(12.0)
        .radius(8.0)
        .border(1.0, c.border)
        .fill(c.background)
        .child(line("plan", "Plan".into(), plan))
        .child(line("usage", label, value))
        .child(line("default", "Default".into(), default))
}

fn header(t: &Tokens, title: &str, body: &str) -> El {
    col()
        .key("step-head")
        .w_full()
        .gap(4.0)
        .child(text(title.to_owned()).font(sans(16.0, 600.0, 22.0)).color(t.colors.foreground).role("heading"))
        .child(text(body.to_owned()).font(sans(14.0, 400.0, 20.0)).color(t.colors.muted_foreground).wrap())
}

fn error_line(t: &Tokens, key: &str, words: &str) -> El {
    text(words.to_owned()).font(sans(12.0, 400.0, 16.0)).color(t.colors.destructive).key(key.to_owned())
}

fn link_body(t: &'static Tokens, flow: &AddFlow, link: &Link, name: &str) -> Vec<El> {
    let c = &t.colors;
    let mut out = vec![header(t, &format!("Sign in to {name}"), "Open the sign-in page in any browser and sign in. Claude then shows a code: copy it and paste it here.")];
    let (url, error) = match link {
        Link::Starting => {
            out.push(toned_notice(t, "starting", Tone::Info, "Getting the sign-in link…"));
            return out;
        }
        Link::Awaiting { url, error } => (url.clone(), error.clone()),
        Link::Submitting { url } => (url.clone(), None),
    };
    let (open, copy) = (url.clone(), url.clone());
    let actions = row()
        .key("link-actions")
        .gap(8.0)
        .flex_wrap()
        .child(Button::new("Open sign-in page").variant(ButtonVariant::Outline).size(ButtonSize::Sm).icon(theme::icons::named("ExternalLink").expect("bundled icon")).build(t).key("open-link").on_press_tx(move |_: &mut State, tx: &mut Tx| {
            if let Err(e) = tx.open_url(&open) {
                log::warn!("[settings.Accounts] the sign-in page did not open: {e}");
            }
        }))
        .child(Button::new("Copy link").variant(ButtonVariant::Outline).size(ButtonSize::Sm).icon(theme::icons::named("Copy").expect("bundled icon")).build(t).key("copy-link").on_press_tx(move |_: &mut State, tx: &mut Tx| tx.copy(copy.clone())));
    out.push(actions);
    out.push(text(url).font(mono(12.0, 400.0, 16.0)).color(c.muted_foreground).wrap().key("link"));
    let code = Input::new(&flow.code)
        .placeholder("Paste the code")
        .label("Code from the Claude page")
        .disabled(matches!(link, Link::Submitting { .. }))
        .build(t)
        .key("code")
        .on_edit(|s: &mut State| &mut s.accounts_ui.add.as_mut().expect("the add dialog is open").code)
        .on_input_change(|s: &mut State, _| {
            if let Some(Step::Signing(Link::Awaiting { error, .. })) = s.accounts_ui.add.as_mut().map(|f| &mut f.step) {
                *error = None;
            }
        });
    out.push(col().key("code-field").gap(8.0).items_stretch().child(text("Code from the Claude page").font(sans(12.0, 500.0, 16.0)).color(c.foreground)).child(code));
    if let Some(e) = error {
        out.push(error_line(t, "code-error", &e));
    }
    out
}

/// The Add account dialog at its step.
fn add_dialog(s: &State, t: &'static Tokens, flow: &AddFlow, layer: &mut Vec<El>) {
    let c = &t.colors;
    let name = flow.account.as_ref().map_or_else(String::new, |a| a.name.clone());
    let mut body: Vec<El> = Vec::new();
    let mut footer: Vec<El> = Vec::new();
    let cancel = |label: &str| Button::new(label).variant(ButtonVariant::Ghost).size(ButtonSize::Sm).build(t).key("cancel").on_press(|s: &mut State| s.accounts_close_add());
    match &flow.step {
        Step::Name => {
            let article = if flow.provider.name().starts_with(['A', 'E', 'I', 'O', 'U']) { "an" } else { "a" };
            body.push(header(t, &format!("Add {article} {} account", flow.provider.name()), "Give it a short name. Troupe shows this name, never the email."));
            body.push(text("Name").font(sans(12.0, 500.0, 16.0)).color(c.foreground));
            let input = Input::new(&flow.name)
                .placeholder("e.g. Work")
                .label("Name")
                .invalid(flow.error.is_some())
                .disabled(flow.busy)
                .build(t)
                .key("account-name")
                .on_edit(|s: &mut State| &mut s.accounts_ui.add.as_mut().expect("the add dialog is open").name)
                .on_input_change(|s: &mut State, _| {
                    if let Some(f) = &mut s.accounts_ui.add
                        && f.name.text.chars().count() > MAX_NAME_CHARS
                    {
                        let keep: String = f.name.text.chars().take(MAX_NAME_CHARS).collect();
                        f.name.replace_text(keep);
                    }
                });
            body.push(input);
            let g = s.account_group(flow.provider);
            let used: Vec<String> = g.snapshot.as_ref().and_then(|v| v["accounts"].as_array()).into_iter().flatten().filter_map(|a| a["name"].as_str().map(str::to_lowercase)).collect();
            let suggestions = NAME_SUGGESTIONS.iter().filter(|n| !used.contains(&n.to_lowercase())).fold(row().key("suggestions").gap(6.0).flex_wrap(), |r, n| {
                r.child(Button::new(*n).variant(ButtonVariant::Secondary).size(ButtonSize::Xs).build(t).key(format!("suggest-{n}")).on_press(move |s: &mut State| {
                    if let Some(f) = &mut s.accounts_ui.add {
                        f.name.replace_text(*n);
                    }
                }))
            });
            body.push(suggestions);
            if let Some(e) = &flow.error {
                body.push(error_line(t, "error", e));
            }
            let signed_in = g.snapshot.as_ref().and_then(|v| v["accounts"].as_array()).is_some_and(|a| a.iter().any(|x| Readiness::of(x, g.usage.get(x["id"].as_str().unwrap_or_default())) == Readiness::Ready));
            if signed_in {
                let words = format!(
                    "Troupe does not recommend using many {0} accounts. A separate work account is fine. Using several accounts to get past usage limits can break {1}'s terms and get your accounts suspended.",
                    flow.provider.name(),
                    flow.provider.company()
                );
                body.push(toned_notice(t, "second-account-notice", Tone::Info, &words));
            }
            footer.push(cancel("Cancel"));
            footer.push(Button::new(if flow.busy { "Adding…" } else { "Continue" }).size(ButtonSize::Sm).disabled(flow.busy).build(t).key("continue").on_press(|s: &mut State| s.accounts_submit_name()));
        }
        Step::Signing(link) => {
            body.extend(link_body(t, flow, link, &name));
            body.push(text("Already signed in to another account on claude.ai? Sign out there first, or open the sign-in page in a private window.").font(sans(11.0, 400.0, 17.0)).color(c.muted_foreground).wrap());
            footer.push(cancel("Cancel"));
            let submitting = matches!(link, Link::Submitting { .. });
            footer.push(Button::new(if submitting { "Checking…" } else { "Finish sign-in" }).size(ButtonSize::Sm).disabled(!matches!(link, Link::Awaiting { .. })).build(t).key("finish").on_press(|s: &mut State| s.accounts_finish_link()));
        }
        Step::Done => {
            body.push(header(t, &format!("{name} is ready"), &format!("New chats in projects that use {name} sign in with it.")));
            body.push(summary(s, t, flow));
            footer.push(Button::new("Make default").variant(ButtonVariant::Ghost).size(ButtonSize::Sm).build(t).key("make-default").on_press(|s: &mut State| s.accounts_add_make_default()));
            footer.push(Button::new("Done").size(ButtonSize::Sm).build(t).key("done").on_press(|s: &mut State| s.accounts_ui.add = None));
        }
        Step::Failed(reason) => {
            body.push(header(t, "Sign-in did not finish", &format!("{reason} {name} stays in your list, signed out.")));
            footer.push(cancel("Close"));
            if flow.provider == Provider::Claude {
                footer.push(Button::new("Try again").size(ButtonSize::Sm).build(t).key("try-again").on_press(|s: &mut State| s.accounts_start_sign_in()));
            }
        }
    }
    if !matches!(flow.step, Step::Failed(_)) {
        body.insert(0, steps(t, &flow.step));
    }
    let mut d = Dialog::new(ADD, "").width(400.0).child(col().key("add-flow").w_full().gap(16.0).items_stretch().children(body));
    for f in footer {
        d = d.footer(f);
    }
    d.open(true).on_close(|s: &mut State| s.accounts_close_add()).build(t, layer);
}

fn rename_dialog(s: &State, t: &'static Tokens, layer: &mut Vec<El>) {
    let ui = &s.accounts_ui;
    let mut body = col()
        .key("rename-body")
        .w_full()
        .gap(6.0)
        .items_stretch()
        .child(text("Name").font(sans(12.0, 500.0, 16.0)).color(t.colors.foreground))
        .child(Input::new(&ui.rename).placeholder("e.g. Work, Personal").label("Name").build(t).key("rename-input").on_edit(|s: &mut State| &mut s.accounts_ui.rename));
    if let Some(e) = &ui.rename_error {
        body = body.child(error_line(t, "rename-error", e));
    }
    let busy = ui.rename_busy;
    Dialog::new(RENAME, "Rename account")
        .description("Troupe shows this name instead of the email address.")
        .width(420.0)
        .child(body)
        .footer(Button::new("Cancel").variant(ButtonVariant::Outline).size(ButtonSize::Sm).disabled(busy).build(t).key("rename-cancel").on_press(|s: &mut State| s.accounts_ui.renaming = None))
        .footer(Button::new(if busy { "Saving…" } else { "Save" }).size(ButtonSize::Sm).disabled(busy).build(t).key("rename-save").on_press(|s: &mut State| s.accounts_submit_rename()))
        .open(true)
        .on_close(|s: &mut State| {
            if !s.accounts_ui.rename_busy {
                s.accounts_ui.renaming = None;
            }
        })
        .build(t, layer);
}

fn remove_dialog(s: &State, t: &'static Tokens, target: &Target, layer: &mut Vec<El>) {
    let c = &t.colors;
    let who = if target.provider == Provider::Claude { "Claude Code" } else { target.provider.name() };
    let mut body = col().key("remove-body").w_full().gap(12.0).items_stretch();
    let moving = s.accounts_moving(target);
    if !moving.is_empty() {
        let g = s.account_group(target.provider);
        let default_id = g.snapshot.as_ref().and_then(|v| v["defaultAccountId"].as_str()).unwrap_or_default();
        let default_name = (default_id != target.id).then(|| g.snapshot.as_ref().and_then(|v| v["accounts"].as_array()).and_then(|a| a.iter().find(|x| x["id"] == json!(default_id))).and_then(|x| x["name"].as_str())).flatten();
        let list = moving.iter().enumerate().fold(
            col().key("moving").w_full().gap(8.0).padding(12.0).radius(8.0).border(1.0, c.border).fill(c.background).items_stretch().child(text(rules::moving_text(moving.len(), default_name)).font(sans(11.0, 500.0, 15.0)).color(c.secondary_foreground)),
            |list, (i, name)| list.child(text(name.clone()).font(sans(11.0, 400.0, 15.0)).color(c.muted_foreground).key(format!("project-{i}"))),
        );
        body = body.child(list);
    }
    let busy = s.accounts_ui.remove_busy;
    Dialog::new(REMOVE, format!("Remove {}?", target.name))
        .description(format!("{who} signs out of this account on this computer. Its chat history stays."))
        .width(420.0)
        .child(body)
        .footer(Button::new("Cancel").variant(ButtonVariant::Outline).size(ButtonSize::Sm).disabled(busy).build(t).key("remove-cancel").on_press(|s: &mut State| s.accounts_ui.removing = None))
        .footer(Button::new(if busy { "Removing…" } else { "Remove" }).variant(ButtonVariant::Destructive).size(ButtonSize::Sm).disabled(busy).build(t).key("remove-confirm").on_press(|s: &mut State| s.accounts_confirm_remove()))
        .open(true)
        .on_close(|s: &mut State| {
            if !s.accounts_ui.remove_busy {
                s.accounts_ui.removing = None;
            }
        })
        .build(t, layer);
}

fn risk_dialog(t: &'static Tokens, p: Provider, layer: &mut Vec<El>) {
    let c = &t.colors;
    let docs = p.docs_url();
    let more = col().key("risk-more").w_full().gap(4.0).items_start().child(text(rules::SHARED_SWITCH_RISK).font(sans(14.0, 400.0, 20.0)).color(c.muted_foreground).wrap()).child(
        Button::new("Read more").variant(ButtonVariant::Link).size(ButtonSize::Sm).build(t).key("read-more").on_press_tx(move |_: &mut State, tx: &mut Tx| {
            if let Err(e) = tx.open_url(&docs) {
                log::warn!("[account-provider-terms] [accounts] could not open {docs}: {e}");
            }
        }),
    );
    Dialog::new(RISK, format!("Switch {} accounts automatically?", p.name()))
        .description(p.risk())
        .width(420.0)
        .child(more)
        .footer(Button::new("Cancel").variant(ButtonVariant::Outline).size(ButtonSize::Sm).build(t).key("risk-cancel").on_press(|s: &mut State| s.accounts_risk_closed(false)))
        .footer(Button::new("Turn on anyway").variant(ButtonVariant::Destructive).size(ButtonSize::Sm).build(t).key("risk-confirm").on_press(|s: &mut State| s.accounts_risk_closed(true)))
        .open(true)
        .on_close(|s: &mut State| s.accounts_risk_closed(false))
        .build(t, layer);
}

/// The page's dialogs.
pub fn dialogs(s: &State, t: &'static Tokens, layer: &mut Vec<El>) {
    let ui = &s.accounts_ui;
    if let Some(flow) = &ui.add {
        add_dialog(s, t, flow, layer);
    }
    if ui.renaming.is_some() {
        rename_dialog(s, t, layer);
    }
    if let Some(target) = &ui.removing {
        remove_dialog(s, t, target, layer);
    }
    if let Some(p) = ui.risk {
        risk_dialog(t, p, layer);
    }
}
