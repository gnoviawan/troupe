//! Settings → Accounts (`provider-section.tsx` with `ProviderSettings.tsx`,
//! `CodexProviderSettings.tsx` and `AntigravityProviderSettings.tsx`): each provider's accounts
//! with their plan and usage, the row menu (make default, sign in, rename, remove), the remove
//! confirm, and the automatic switching opt-in behind its risk warning. The words are
//! `model::settings::accounts`; the dialogs are `settings_accounts_dialogs`. Only names, ids and
//! usage reach this page; nothing here reads or logs a token.

use std::collections::{BTreeMap, BTreeSet};

use model::settings::accounts::{self as rules, AccountTone, Meter, Provider, Readiness, UsageTone};
use rune::{El, TextEdit, col, row, text};
use serde_json::{Value, json};
use theme::Tokens;
use troupe_protocol::ws::{WsError, WsRequestType};
use ui::overlay::{Menu, MenuItem, ToastKind};
use ui::settings::{card, setting};
use ui::type_::sans;
use ui::{Button, ButtonSize, ButtonVariant};

use crate::State;
use crate::sync::Pending;

pub const PAGE: &str = "provider";
const MENU: &str = "account-menu";
/// `CLAUDE_USAGE_POLL_MS`, in seconds.
const USAGE_POLL: f64 = 300.0;

/// One provider's group: its snapshot, its usage reads, and what is in flight.
#[derive(Default)]
pub struct Group {
    pub snapshot: Option<Value>,
    pub usage: BTreeMap<String, Value>,
    pub loading: bool,
    pub error: Option<String>,
    pub refreshing: bool,
    pub saving_auto: bool,
}

/// The account a row action is for.
#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    pub provider: Provider,
    pub id: String,
    pub name: String,
}

#[derive(Default)]
pub struct AccountsUi {
    pub groups: [Group; 3],
    pub menu: Option<(Provider, String)>,
    pub expanded: BTreeSet<String>,
    pub renaming: Option<Target>,
    pub rename: TextEdit,
    pub rename_error: Option<String>,
    pub rename_busy: bool,
    pub removing: Option<Target>,
    pub remove_busy: bool,
    /// The provider whose automatic switching waits for the risk warning.
    pub risk: Option<Provider>,
    pub add: Option<crate::settings_accounts_dialogs::AddFlow>,
    next_usage: Option<f64>,
}

/// What an accounts reply is for.
#[derive(Clone, Debug, PartialEq)]
pub enum AccountsCall {
    List(Provider),
    Usage(Provider),
    Default(Target),
    AutoSwitch(Provider),
    Rename(Target),
    Remove(Target),
    Add(Provider),
    LinkStart(String),
    LinkFinish(String),
    Cancel(String),
}

fn index(p: Provider) -> usize {
    match p {
        Provider::Claude => 0,
        Provider::Codex => 1,
        Provider::Antigravity => 2,
    }
}

/// An account action, as each provider's host request names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Act {
    List,
    Usage,
    Default,
    Auto,
    Add,
    Rename,
    Remove,
}

pub(crate) fn kind(p: Provider, act: Act) -> WsRequestType {
    use WsRequestType as W;
    match (p, act) {
        (Provider::Claude, Act::List) => W::ListClaudeAccounts,
        (Provider::Claude, Act::Usage) => W::ListClaudeUsage,
        (Provider::Claude, Act::Default) => W::SetDefaultClaudeAccount,
        (Provider::Claude, Act::Auto) => W::SetClaudeAutoSwitch,
        (Provider::Claude, Act::Add) => W::AddClaudeAccount,
        (Provider::Claude, Act::Rename) => W::RenameClaudeAccount,
        (Provider::Claude, Act::Remove) => W::RemoveClaudeAccount,
        (Provider::Codex, Act::List) => W::ListCodexAccounts,
        (Provider::Codex, Act::Usage) => W::ListCodexUsage,
        (Provider::Codex, Act::Default) => W::SetDefaultCodexAccount,
        (Provider::Codex, Act::Auto) => W::SetCodexAutoSwitch,
        (Provider::Codex, Act::Add) => W::AddCodexAccount,
        (Provider::Codex, Act::Rename) => W::RenameCodexAccount,
        (Provider::Codex, Act::Remove) => W::RemoveCodexAccount,
        (Provider::Antigravity, Act::List) => W::ListAntigravityAccounts,
        (Provider::Antigravity, Act::Usage) => W::ListAntigravityUsage,
        (Provider::Antigravity, Act::Default) => W::SetDefaultAntigravityAccount,
        (Provider::Antigravity, Act::Auto) => W::SetAntigravityAutoSwitch,
        (Provider::Antigravity, Act::Add) => W::AddAntigravityAccount,
        (Provider::Antigravity, Act::Rename) => W::RenameAntigravityAccount,
        (Provider::Antigravity, Act::Remove) => W::RemoveAntigravityAccount,
    }
}

impl State {
    pub(crate) fn account_group(&self, p: Provider) -> &Group {
        &self.accounts_ui.groups[index(p)]
    }

    fn account_group_mut(&mut self, p: Provider) -> &mut Group {
        &mut self.accounts_ui.groups[index(p)]
    }

    /// The page opened: every group reads its accounts and usage.
    pub(crate) fn accounts_opened(&mut self) {
        for p in Provider::ALL {
            let unread = self.account_group(p).snapshot.is_none();
            self.account_group_mut(p).loading = unread;
            self.accounts_reload(p);
            self.accounts_usage(p, false);
        }
        self.accounts_ui.next_usage = Some(self.now + USAGE_POLL);
    }

    pub(crate) fn accounts_reload(&mut self, p: Provider) {
        self.call(kind(p, Act::List), json!({}), Pending::Accounts(AccountsCall::List(p)));
    }

    pub(crate) fn accounts_usage(&mut self, p: Provider, force: bool) {
        self.account_group_mut(p).refreshing = true;
        self.call(kind(p, Act::Usage), json!({ "force": force }), Pending::Accounts(AccountsCall::Usage(p)));
    }

    /// The five-minute usage poll while the page shows.
    pub(crate) fn accounts_tick(&mut self, now: f64) -> Option<f64> {
        let showing = self.overlays.is_open(crate::settings_view::APP) && self.settings_page().id == PAGE;
        if !showing {
            self.accounts_ui.next_usage = None;
            return None;
        }
        if self.accounts_ui.next_usage.is_some_and(|at| at <= now) {
            for p in Provider::ALL {
                self.accounts_usage(p, false);
            }
            self.accounts_ui.next_usage = Some(now + USAGE_POLL);
        }
        self.accounts_ui.next_usage
    }

    pub(crate) fn accounts_make_default(&mut self, target: Target) {
        self.accounts_ui.menu = None;
        log::info!("[settings.Accounts] {} make default {}", target.provider.id(), target.id);
        self.call(kind(target.provider, Act::Default), json!({ "accountId": target.id }), Pending::Accounts(AccountsCall::Default(target)));
    }

    /// The switch: on asks first through the risk warning; off saves at once.
    pub(crate) fn accounts_auto_switch(&mut self, p: Provider, enabled: bool) {
        if enabled {
            self.accounts_ui.risk = Some(p);
        } else {
            self.accounts_save_auto(p, false);
        }
    }

    /// `closeWarning`.
    pub(crate) fn accounts_risk_closed(&mut self, accepted: bool) {
        let Some(p) = self.accounts_ui.risk.take() else { return };
        log::info!("[AccountGroupFooter] [accounts] {} automatic switching risk warning {}", p.id(), if accepted { "accepted" } else { "cancelled" });
        if accepted {
            self.accounts_save_auto(p, true);
        }
    }

    fn accounts_save_auto(&mut self, p: Provider, enabled: bool) {
        self.account_group_mut(p).saving_auto = true;
        self.call(kind(p, Act::Auto), json!({ "enabled": enabled }), Pending::Accounts(AccountsCall::AutoSwitch(p)));
    }

    pub(crate) fn accounts_open_rename(&mut self, target: Target) {
        self.accounts_ui.menu = None;
        self.accounts_ui.rename = TextEdit::new(&target.name);
        self.accounts_ui.rename_error = None;
        self.accounts_ui.renaming = Some(target);
    }

    pub(crate) fn accounts_submit_rename(&mut self) {
        let Some(target) = self.accounts_ui.renaming.clone() else { return };
        let name = self.accounts_ui.rename.text.trim().to_owned();
        if name.is_empty() {
            self.accounts_ui.rename_error = Some("Enter a name.".into());
            return;
        }
        self.accounts_ui.rename_busy = true;
        self.accounts_ui.rename_error = None;
        self.call(kind(target.provider, Act::Rename), json!({ "accountId": target.id, "name": name }), Pending::Accounts(AccountsCall::Rename(target)));
    }

    pub(crate) fn accounts_open_remove(&mut self, target: Target) {
        self.accounts_ui.menu = None;
        self.accounts_ui.removing = Some(target);
    }

    /// `confirmRemove`: only the account the confirm names.
    pub(crate) fn accounts_confirm_remove(&mut self) {
        let Some(target) = self.accounts_ui.removing.clone() else { return };
        if self.accounts_ui.remove_busy {
            return;
        }
        log::info!("[settings.Accounts] {} remove {}", target.provider.id(), target.id);
        self.accounts_ui.remove_busy = true;
        self.call(kind(target.provider, Act::Remove), json!({ "accountId": target.id }), Pending::Accounts(AccountsCall::Remove(target)));
    }

    pub(crate) fn accounts_answered(&mut self, call: AccountsCall, result: Result<Option<Value>, WsError>) {
        match call {
            AccountsCall::List(p) => {
                let g = self.account_group_mut(p);
                g.loading = false;
                match result {
                    Ok(Some(snapshot)) => {
                        g.snapshot = Some(snapshot);
                        g.error = None;
                    }
                    Ok(None) => g.error = Some("The host sent no account list".into()),
                    Err(e) => {
                        log::warn!("[{}Accounts.list] {} account list failed: {}", p.id(), p.name(), e.code);
                        g.error = Some(e.message);
                    }
                }
            }
            AccountsCall::Usage(p) => {
                let g = self.account_group_mut(p);
                g.refreshing = false;
                if let Ok(Some(reply)) = result {
                    g.usage = reply["usage"].as_array().into_iter().flatten().filter_map(|u| Some((u["accountId"].as_str()?.to_owned(), u.clone()))).collect();
                }
            }
            AccountsCall::Default(target) => {
                match result {
                    Ok(_) => self.toasts.push(self.now, ToastKind::Success, format!("{} is now the default account", target.name), None),
                    Err(e) => self.toasts.push(self.now, ToastKind::Error, "Could not change the default account", Some(&e.message)),
                };
                self.accounts_reload(target.provider);
            }
            AccountsCall::AutoSwitch(p) => {
                self.account_group_mut(p).saving_auto = false;
                if let Err(e) = result {
                    self.toasts.push(self.now, ToastKind::Error, format!("Could not change automatic switching: {}", e.message), None);
                }
                self.accounts_reload(p);
            }
            AccountsCall::Rename(target) => {
                self.accounts_ui.rename_busy = false;
                match result {
                    Ok(_) => {
                        self.accounts_ui.renaming = None;
                        self.accounts_reload(target.provider);
                    }
                    Err(e) => self.accounts_ui.rename_error = Some(e.message),
                }
            }
            AccountsCall::Remove(target) => {
                self.accounts_ui.remove_busy = false;
                match result {
                    Ok(reply) => {
                        let moved = reply.as_ref().and_then(|r| r["movedProjectIds"].as_array()).map_or(0, Vec::len);
                        log::info!("[settings.Accounts] {} account removed; {moved} projects moved", target.provider.id());
                        self.accounts_ui.removing = None;
                        self.accounts_reload(target.provider);
                        self.toasts.push(self.now, ToastKind::Success, format!("{} was removed", target.name), None);
                    }
                    Err(e) => {
                        self.toasts.push(self.now, ToastKind::Error, format!("Could not remove {}: {}", target.name, e.message), None);
                    }
                }
            }
            other => self.accounts_add_answered(other, result),
        }
    }

    /// The names of the projects that chose each account, in the projects list's order.
    pub(crate) fn projects_by_account(&self, p: Provider) -> BTreeMap<String, Vec<String>> {
        let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let Some(map) = self.account_group(p).snapshot.as_ref().and_then(|s| s["projectAccounts"].as_object()) else { return out };
        for project in &self.projects.store.projects {
            if let Some(account) = map.get(&project.id).and_then(Value::as_str) {
                out.entry(account.to_owned()).or_default().push(project.name.clone());
            }
        }
        out
    }
}

fn glyph(name: &str) -> &'static rune::Icon {
    theme::icons::named(name).expect("bundled icon")
}

fn dot(t: &Tokens, tone: AccountTone) -> El {
    let c = &t.colors;
    let fill = match tone {
        AccountTone::Ok => c.success,
        AccountTone::Warning | AccountTone::SignedOut => c.warning,
        AccountTone::Critical => c.destructive,
        AccountTone::Waiting | AccountTone::Unknown => c.muted_foreground.alpha(0.5),
    };
    rune::el().key("dot").w(6.0).h(6.0).radius(3.0).fill(fill).a11y_hidden()
}

/// `AccountAvatar`.
fn avatar(t: &Tokens, name: &str) -> El {
    let c = &t.colors;
    let letter: String = name.chars().next().map(|ch| ch.to_uppercase().collect()).unwrap_or_default();
    row()
        .key("avatar")
        .w(28.0)
        .h(28.0)
        .radius(14.0)
        .fill(c.muted)
        .border(1.0, c.foreground.alpha(0.1))
        .items_center()
        .justify_center()
        .child(text(letter).font(sans(11.0, 600.0, 14.0)).color(c.secondary_foreground))
        .a11y_hidden()
}

fn percent_color(t: &Tokens, tone: UsageTone) -> rune::Color {
    match tone {
        UsageTone::Normal => t.colors.foreground,
        UsageTone::Warning => t.colors.warning,
        UsageTone::Critical => t.colors.destructive,
    }
}

/// `UsageBar`: a 4 px track; any use above zero shows a sliver.
fn bar(t: &Tokens, m: &Meter) -> El {
    let c = &t.colors;
    let fill = if m.percent > 0.0 { m.percent.clamp(1.5, 100.0) } else { 0.0 };
    let ink = match rules::usage_tone(m.percent) {
        UsageTone::Normal => c.primary,
        UsageTone::Warning => c.warning,
        UsageTone::Critical => c.destructive,
    };
    row()
        .key("bar")
        .role("progressbar")
        .label(format!("{} usage", m.label))
        .w_full()
        .h(4.0)
        .radius(2.0)
        .fill(c.muted)
        .child(rune::el().key("fill").w_frac(fill as f32 / 100.0).h_full().radius(2.0).fill(ink))
}

/// `MainMeter` and the Codex and Antigravity meters: label, percent, reset, bar.
fn meter(t: &Tokens, key: String, m: &Meter, now: f64) -> El {
    let c = &t.colors;
    let tone = rules::usage_tone(m.percent);
    let label = if tone == UsageTone::Critical { format!("{} limit reached", m.label) } else { m.label.clone() };
    let mut line = row()
        .key("line")
        .w_full()
        .gap(6.0)
        .items_center()
        .child(text(label).font(sans(11.0, 500.0, 15.0)).color(if tone == UsageTone::Critical { c.destructive } else { c.secondary_foreground }).single_line().truncate().min_w(0.0).grow(1.0))
        .child(text(format!("{}%", m.percent.round() as i64)).font(sans(11.0, 600.0, 15.0)).color(percent_color(t, tone)));
    let reset_ms = m.resets_at.as_deref().and_then(crate::git_history::parse_iso);
    if let Some(reset) = rules::format_reset(reset_ms, now) {
        line = line.child(text(reset).font(sans(11.0, 400.0, 15.0)).color(c.muted_foreground));
    }
    col().key(key).w_full().gap(6.0).items_stretch().child(line).child(bar(t, m))
}

/// The ⋯ menu (`ClaudeAccountActions` and its Codex and Antigravity twins).
fn actions(s: &State, t: &'static Tokens, target: &Target, builtin: bool, is_default: bool, readiness: Readiness, layer: &mut Vec<El>) -> El {
    let open = s.accounts_ui.menu.as_ref() == Some(&(target.provider, target.id.clone()));
    let key = (target.provider, target.id.clone());
    let trigger = ui::quiet_icon_button(t, glyph("MoreHorizontal"), format!("Actions for {}", target.name), 28.0, 14.0).key("actions").on_press(move |s: &mut State| s.accounts_ui.menu = Some(key.clone()));
    let mut menu = Menu::new(format!("{MENU}-{}-{}", target.provider.id(), target.id)).width(176.0).open(open).on_close(|s: &mut State| s.accounts_ui.menu = None);
    if !is_default {
        let tg = target.clone();
        let ready = readiness == Readiness::Ready;
        let mut item = MenuItem::new("Make default").disabled(!ready).on_select(move |s: &mut State| s.accounts_make_default(tg.clone()));
        if !ready {
            item = item.shortcut("Sign in to it first");
        }
        menu = menu.item(item);
    }
    let sign = target.clone();
    menu = menu.item(MenuItem::new(if readiness == Readiness::SignedOut { "Sign in" } else { "Sign in again" }).on_select(move |s: &mut State| {
        s.accounts_ui.menu = None;
        s.accounts_sign_in(sign.clone());
    }));
    let rename = target.clone();
    menu = menu.item(MenuItem::new("Rename").on_select(move |s: &mut State| s.accounts_open_rename(rename.clone())));
    if !builtin {
        let remove = target.clone();
        menu = menu.separator().item(MenuItem::new("Remove").destructive().on_select(move |s: &mut State| s.accounts_open_remove(remove.clone())));
    }
    menu.build(t, layer, trigger)
}

fn small(t: &Tokens, words: String) -> El {
    text(words).font(sans(12.0, 400.0, 16.0)).color(t.colors.muted_foreground).wrap()
}

/// A Claude row's body under its name (`ClaudeAccountRow`).
fn claude_body(s: &State, t: &'static Tokens, target: &Target, usage: Option<&Value>, tone: AccountTone, projects: &[String], mut out: El) -> El {
    let (id, name) = (target.id.as_str(), target.name.as_str());
    let c = &t.colors;
    let now = crate::editor_host::epoch_ms();
    if tone == AccountTone::Waiting {
        return out.child(small(t, format!("Finish signing in to {name} in your browser…")));
    }
    if tone == AccountTone::SignedOut {
        out = out.child(small(t, "Sign in to use this account and see its limits.".into()));
        if !projects.is_empty() {
            out = out.child(small(t, format!("Chats in projects that use {name} cannot start until then.")));
        }
        return out;
    }
    let Some(u) = usage else { return out };
    match u["status"].as_str() {
        Some("error") => {
            return out.child(
                row()
                    .key("usage-error")
                    .gap(6.0)
                    .items_center()
                    .child(small(t, "Usage is not available right now.".into()))
                    .child(Button::new("Try again").variant(ButtonVariant::Link).size(ButtonSize::Xs).build(t).key("retry-usage").on_press(|s: &mut State| s.accounts_usage(Provider::Claude, true))),
            );
        }
        Some("unavailable") => return out.child(small(t, "This login has no plan limits to show.".into())),
        _ => {}
    }
    let Some(primary) = rules::claude_primary(Some(u)) else { return out.child(small(t, "No usage yet.".into())) };
    let expanded = s.accounts_ui.expanded.contains(id);
    let toggle_id = id.to_owned();
    out = out.child(meter(t, "main-meter".into(), &primary, now).role("button").focusable(true).label(format!("{} all limits for {name}", if expanded { "Hide" } else { "Show" })).on_press(move |s: &mut State| {
        if !s.accounts_ui.expanded.remove(&toggle_id) {
            s.accounts_ui.expanded.insert(toggle_id.clone());
        }
    }));
    let others = rules::claude_others(Some(u));
    if !others.is_empty() {
        let mut chips = row().key("others").gap(12.0).flex_wrap();
        for (i, o) in others.iter().enumerate() {
            let tone = rules::usage_tone(o.percent);
            let ink = if tone == UsageTone::Normal { c.secondary_foreground } else { percent_color(t, tone) };
            chips = chips.child(row().key(format!("other-{i}")).gap(6.0).child(text(o.label.clone()).font(sans(11.0, 400.0, 15.0)).color(c.muted_foreground)).child(text(format!("{}%", o.percent.round() as i64)).font(sans(11.0, 500.0, 15.0)).color(ink)));
        }
        out = out.child(chips);
    }
    if expanded {
        let mut detail = col().key("detail").w_full().gap(12.0).pl(38.0).items_stretch();
        for (i, m) in rules::claude_all(Some(u)).iter().enumerate() {
            detail = detail.child(meter(t, format!("detail-{i}"), m, now));
        }
        let mut foot = row().key("detail-foot").gap(16.0).flex_wrap();
        if !projects.is_empty() {
            foot = foot.child(small(t, format!("Used by: {}", projects.join(", "))));
        }
        foot = foot.child(small(t, rules::format_checked(u["checkedAt"].as_f64().unwrap_or(now), now)));
        out = out.child(detail.child(foot));
    }
    out
}

/// One account row.
fn account_row(s: &State, t: &'static Tokens, p: Provider, account: &Value, layer: &mut Vec<El>) -> El {
    let c = &t.colors;
    let g = s.account_group(p);
    let id = account["id"].as_str().unwrap_or_default().to_owned();
    let name = account["name"].as_str().unwrap_or_default().to_owned();
    let usage = g.usage.get(&id);
    let is_default = g.snapshot.as_ref().is_some_and(|snap| snap["defaultAccountId"] == json!(id));
    let projects = s.projects_by_account(p).remove(&id).unwrap_or_default();
    let signing = s.accounts_signing_in(&id);
    let readiness = Readiness::of(account, usage);
    let (tone, meta) = match p {
        Provider::Claude => rules::claude_status(account, usage, signing, projects.len()),
        _ => rules::other_status(account, usage, &projects),
    };
    let mut title = row().key("title").min_w(0.0).gap(6.0).items_center().child(text(name.clone()).font(sans(12.0, 500.0, 16.0)).color(c.foreground).single_line().truncate().min_w(0.0));
    if is_default && rules::shows_default_pill(&name) {
        title = title.child(text("Default").font(sans(10.0, 500.0, 12.0)).color(c.secondary_foreground).px(6.0).py(2.0).radius(4.0).fill(c.muted).key("default-pill"));
    }
    let meta_row = row().key("meta").min_w(0.0).gap(6.0).items_center().child(dot(t, tone)).child(text(meta).font(sans(11.0, 400.0, 15.0)).color(c.muted_foreground).single_line().truncate().min_w(0.0));
    let who = col().key("who").min_w(0.0).gap(4.0).child(title).child(meta_row);
    let target = Target { provider: p, id: id.clone(), name: name.clone() };
    let menu = actions(s, t, &target, account["builtin"].as_bool() == Some(true), is_default, readiness, layer);
    let line = row().key("line").w_full().gap(10.0).items_center().child(avatar(t, &name)).child(who.grow(1.0).min_w(0.0)).child(menu);
    let out = col().key(format!("{}-account-{id}", p.id())).w_full().py(14.0).gap(10.0).items_stretch().border_sides([0.0, 0.0, 1.0, 0.0], c.border.alpha(0.6)).child(line);
    if p == Provider::Claude {
        return claude_body(s, t, &target, usage, tone, &projects, out);
    }
    let now = crate::editor_host::epoch_ms();
    rules::meters(p, usage).iter().enumerate().fold(out, |out, (i, m)| out.child(meter(t, format!("meter-{i}"), m, now).pl(38.0)))
}

/// The automatic switching card, or the second-account hint (`AccountGroupFooter`).
fn footer(t: &'static Tokens, p: Provider, count: usize, auto: bool, saving: bool) -> El {
    let c = &t.colors;
    if count >= 2 {
        let switch = ui::base::Switch::new(auto).label("Switch accounts automatically").disabled(saving).build(t).key("auto-switch").on_press(move |s: &mut State| s.accounts_auto_switch(p, !auto));
        let words = format!("When a chat reaches its account's limit, move it to another signed-in account. Off by default, because {}'s terms may not allow it.", p.company());
        return card(t, &format!("{}-auto-switch-card", p.id()), vec![setting(t, &format!("{}-auto-switch", p.id()), "Switch accounts automatically", &words, Some(switch))]);
    }
    row()
        .key(format!("{}-second-account-hint", p.id()))
        .pt(12.0)
        .gap(8.0)
        .items_start()
        .child(rune::icon(glyph("KeyRound"), 12.0).color(c.muted_foreground).mt(2.0).a11y_hidden())
        .child(text("Add a second account to keep work and personal apart.").font(sans(11.0, 400.0, 17.0)).color(c.muted_foreground).wrap())
}

/// `ProviderGroupHeader` and the group under it.
fn provider_group(s: &State, t: &'static Tokens, p: Provider, layer: &mut Vec<El>) -> El {
    let c = &t.colors;
    let g = s.account_group(p);
    let failed = !g.loading && (g.error.is_some() || g.snapshot.is_none());
    let accounts: Vec<Value> = g.snapshot.as_ref().and_then(|snap| snap["accounts"].as_array().cloned()).unwrap_or_default();
    let mut head = row().key("head").w_full().min_h(28.0).gap(8.0).items_center().flex_wrap().child(text(p.heading()).font(sans(14.0, 500.0, 20.0)).color(c.foreground).role("heading"));
    if g.snapshot.is_some() && !failed {
        head = head.child(text(rules::count_text(accounts.len())).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground).key("count"));
    }
    head = head.child(rune::el().key("spacer").grow(1.0));
    let checked = if p == Provider::Claude && !failed { rules::oldest_check(g.usage.values()) } else { None };
    let status = if g.refreshing { Some("Updating…".to_owned()) } else { checked.map(|at| rules::format_checked(at, crate::editor_host::epoch_ms())) };
    if let Some(words) = status {
        head = head.child(text(words).font(sans(11.0, 400.0, 15.0)).color(c.muted_foreground).key("checked"));
    }
    if p == Provider::Claude && !failed {
        head = head.child(ui::quiet_icon_button(t, glyph("RefreshCw"), format!("Refresh {} usage", p.heading()), 24.0, 13.0).key("refresh").disabled(g.refreshing).on_press(move |s: &mut State| s.accounts_usage(p, true)));
    }
    if !failed {
        head = head.child(Button::new("Add account").variant(ButtonVariant::Outline).size(ButtonSize::Xs).icon(glyph("Plus")).build(t).key("add-account").on_press(move |s: &mut State| s.accounts_open_add(p)));
    }
    let out = col().key(format!("{}-accounts", p.id())).role("group").label(p.heading()).w_full().gap(12.0).items_stretch().child(head);
    if g.loading {
        return out.child(text(format!("Loading {} accounts…", p.name())).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground).key("loading"));
    }
    if failed {
        let why = g.error.as_ref().map(|e| format!("{e}. ")).unwrap_or_default();
        let words = col()
            .key("words")
            .grow(1.0)
            .min_w(0.0)
            .gap(2.0)
            .child(text(format!("{} accounts are not available right now", p.name())).font(sans(12.0, 500.0, 16.0)).color(c.foreground))
            .child(text(format!("{why}Chats keep using the account they started with.")).font(sans(11.0, 400.0, 15.0)).color(c.muted_foreground).wrap());
        let retry = Button::new("Try again").variant(ButtonVariant::Outline).size(ButtonSize::Xs).icon(glyph("RotateCcw")).build(t).key("retry").on_press(move |s: &mut State| s.accounts_reload(p));
        return out.child(row().key("failed").w_full().py(16.0).gap(12.0).items_center().child(rune::icon(glyph("WifiOff"), 16.0).color(c.muted_foreground).a11y_hidden()).child(words).child(retry));
    }
    let rows = accounts.iter().fold(col().key("rows").w_full().px(16.0).radius(8.0).fill(c.card).border(1.0, c.border).items_stretch(), |rows, a| rows.child(account_row(s, t, p, a, layer)));
    let auto = g.snapshot.as_ref().and_then(|snap| snap["autoSwitch"].as_bool()).unwrap_or(false);
    out.child(rows).child(footer(t, p, accounts.len(), auto, g.saving_auto))
}

/// The page.
pub fn page(s: &State, t: &'static Tokens, layer: &mut Vec<El>) -> Vec<El> {
    let c = &t.colors;
    let mut out: Vec<El> = Provider::ALL.iter().map(|p| provider_group(s, t, *p, layer)).collect();
    out.push(
        row()
            .key("per-project-note")
            .mt(24.0)
            .gap(6.0)
            .items_center()
            .child(rune::icon(glyph("Info"), 12.0).color(c.muted_foreground).a11y_hidden())
            .child(text("Choose an account per project in Project settings → Accounts.").font(sans(11.0, 400.0, 15.0)).color(c.muted_foreground)),
    );
    crate::settings_accounts_dialogs::dialogs(s, t, layer);
    out
}

#[cfg(test)]
#[path = "settings_accounts_tests.rs"]
mod tests;
