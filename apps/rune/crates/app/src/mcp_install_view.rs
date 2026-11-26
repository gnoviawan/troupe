//! The catalog install dialogs (`McpCatalogInstallDialog.tsx`, `McpInstallForm.tsx` and
//! `McpConsentDialog.tsx`): the form, the consent review, and the step dialogs around them.

use model::settings::mcp_install::{self as rules, Control, Field, Flow, Probe};
use rune::{El, Tx, col, row, text};
use serde_json::Value;
use theme::Tokens;
use ui::base::{Badge, BadgeVariant, Spinner, Switch};
use ui::overlay::{Dialog, Select};
use ui::settings::{Tone, toned_notice};
use ui::type_::{mono, sans};
use ui::{Button, ButtonSize, ButtonVariant, Input};

use crate::State;
use crate::mcp_catalog::CatalogStatus;
use crate::settings_mcp_dialog::DIALOG_CHROME;

const KEY: &str = "mcp-install-dialog";

fn verbatim(t: &Tokens, words: &str, muted: bool) -> El {
    if words.is_empty() {
        return text("(empty)").font(sans(12.0, 400.0, 16.0)).color(t.colors.muted_foreground);
    }
    let ink = if muted { t.colors.muted_foreground } else { t.colors.foreground };
    text(words.to_owned()).font(mono(12.0, 400.0, 16.0)).color(ink).wrap()
}

/// `Section`: a `label-panel` heading over its body.
fn section(t: &Tokens, key: &str, heading: &str, body: El) -> El {
    col()
        .key(key.to_owned())
        .role("group")
        .label(heading.to_owned())
        .w_full()
        .gap(6.0)
        .items_stretch()
        .child(text(heading.to_owned()).font(sans(12.0, 500.0, 17.0)).color(t.colors.muted_foreground))
        .child(body)
}

fn boxed(t: &Tokens, key: &str) -> El {
    col().key(key.to_owned()).w_full().gap(2.0).px(12.0).py(8.0).radius(8.0).fill(t.colors.muted.alpha(0.4)).border(1.0, t.colors.border.alpha(0.6)).items_stretch()
}

fn pending(t: &Tokens, key: &str, words: &str) -> El {
    row()
        .key(key.to_owned())
        .gap(8.0)
        .items_center()
        .child(Spinner::new().size(13.0).decorative().build(t.colors.muted_foreground, false))
        .child(text(words.to_owned()).font(sans(12.0, 400.0, 16.0)).color(t.colors.muted_foreground))
}

/// `McpConsentReview`: the warnings, the launch, the folders, the values and the source.
fn review(t: &'static Tokens, view: &Value, digest: &str) -> El {
    let strs = |v: &Value| v.as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_owned).collect::<Vec<_>>();
    let mut out = col().key("consent-review").w_full().gap(16.0).items_stretch();
    let warnings = strs(&view["warnings"]);
    for (danger, key, tone) in [(true, "danger", Tone::Danger), (false, "caution", Tone::Warning)] {
        let words: Vec<&str> = warnings.iter().filter(|w| rules::is_danger(w) == danger).map(|w| rules::warning_text(w)).collect();
        if !words.is_empty() {
            out = out.child(toned_notice(t, &format!("warnings-{key}"), tone, &words.join("\n")));
        }
    }
    let transport = view["transport"].as_str().unwrap_or("stdio");
    if transport == "stdio" {
        out = out.child(section(t, "command", "Command", verbatim(t, view["command"].as_str().unwrap_or_default(), false)));
        let mut args = boxed(t, "argv");
        for (i, arg) in strs(&view["argv"]).iter().enumerate() {
            args = args.child(verbatim(t, arg, false).key(format!("arg-{i}")));
        }
        out = out.child(section(t, "arguments", "Arguments", args));
    } else {
        out = out.child(section(t, "url", &format!("URL ({transport})"), verbatim(t, view["url"].as_str().unwrap_or_default(), false)));
    }
    let mounts = view["hostMounts"].as_array().cloned().unwrap_or_default();
    if !mounts.is_empty() {
        let mut list = boxed(t, "mounts");
        for (i, m) in mounts.iter().enumerate() {
            let mut line = row().key(format!("mount-{i}")).gap(6.0).items_center().flex_wrap().child(verbatim(t, m["hostPath"].as_str().unwrap_or_default(), false));
            if m["sensitive"].as_bool() == Some(true) {
                line = line.child(Badge::new("dangerous").variant(BadgeVariant::Destructive).build(t));
            }
            list = list.child(line);
        }
        out = out.child(section(t, "host-mounts", "Folders from this computer", list));
    }
    for (key, heading) in [("env", "Environment"), ("headers", "Headers")] {
        let values = view[key].as_array().cloned().unwrap_or_default();
        if values.is_empty() {
            continue;
        }
        let mut list = col().key(format!("{key}-values")).w_full().gap(6.0).items_stretch();
        for (i, v) in values.iter().enumerate() {
            let mut name = row().key("name").gap(6.0).items_center().flex_wrap().child(verbatim(t, v["name"].as_str().unwrap_or_default(), false));
            if v["isSecret"].as_bool() == Some(true) {
                name = name.child(Badge::new("secret").variant(BadgeVariant::Secondary).build(t));
            }
            if v["origin"].as_str() == Some("file") {
                name = name.child(Badge::new("from file").variant(BadgeVariant::Outline).build(t));
            }
            let mut entry = col().key(format!("value-{i}")).gap(2.0).items_stretch().child(name);
            if let Some(value) = v["value"].as_str() {
                entry = entry.child(verbatim(t, value, true));
            }
            list = list.child(entry);
        }
        out = out.child(section(t, key, heading, list));
    }
    if let Some(p) = view["provenance"].as_object() {
        let fact = |key: &str, label: &str, body: El| row().key(key.to_owned()).w_full().gap(12.0).items_start().child(text(label.to_owned()).font(sans(12.0, 400.0, 16.0)).color(t.colors.muted_foreground).w(112.0)).child(body.grow(1.0).min_w(0.0));
        let plain = |words: String| text(words).font(sans(12.0, 400.0, 16.0)).color(t.colors.foreground).wrap();
        let mut facts = col().key("facts").w_full().gap(4.0).items_stretch().child(fact("publisher", "Publisher", plain(rules::namespace_label(&p["namespace"]))));
        if let Some(url) = p.get("repositoryUrl").and_then(Value::as_str).map(str::to_owned) {
            let link = text(url.clone()).font(sans(12.0, 400.0, 16.0)).color(t.colors.foreground).underline().wrap().role("link").focusable(true).on_press_tx(move |s: &mut State, tx: &mut Tx| {
                if tx.open_url(&url).is_err() {
                    s.toasts.push(s.now, ui::overlay::ToastKind::Error, "Couldn't open the repository link.", None);
                }
            });
            facts = facts.child(fact("repository", "Repository", link));
        }
        let mut version = row().key("version-value").gap(6.0).items_center().flex_wrap().child(text(p.get("version").and_then(Value::as_str).unwrap_or_default().to_owned()).font(mono(12.0, 400.0, 16.0)).color(t.colors.foreground));
        if p.get("mutableTag").and_then(Value::as_bool) == Some(true) {
            version = version.child(Badge::new("mutable tag").variant(BadgeVariant::Outline).build(t));
        }
        facts = facts.child(fact("version", "Version", version));
        if let Some(at) = p.get("snapshotFetchedAt").and_then(Value::as_str) {
            let age = rules::snapshot_age(&crate::git_history::relative_time(at, crate::editor_host::epoch_ms()));
            facts = facts.child(fact("snapshot", "Catalog snapshot", plain(age)));
        }
        out = out.child(section(t, "source", "Source", facts));
    }
    if !digest.is_empty() {
        out = out.child(text(format!("Launch digest {digest}")).font(mono(10.0, 400.0, 14.0)).color(t.colors.muted_foreground).wrap().key("digest"));
    }
    out
}

/// `InstallField`.
fn field(s: &State, t: &'static Tokens, f: &Field, value: &str, error: Option<&String>, disabled: bool, menus: &mut Vec<El>) -> El {
    let c = &t.colors;
    let id = f.id.clone();
    let mut title = row().key("label").gap(2.0).child(text(f.label.clone()).font(sans(12.0, 500.0, 16.0)).color(c.foreground));
    if f.is_required {
        title = title.child(text("*").font(sans(12.0, 500.0, 16.0)).color(c.muted_foreground).a11y_hidden());
    }
    let control = match f.control() {
        Control::Boolean => {
            let on = value == "true";
            Switch::new(on).label(f.label.clone()).disabled(disabled).build(t).key("switch").on_press(move |s: &mut State| s.install_pick(&id, if on { "false" } else { "true" }))
        }
        Control::Choice => {
            let choices = f.choices.clone().unwrap_or_default();
            let options: Vec<&str> = choices.iter().map(String::as_str).collect();
            let (open_id, pick_id) = (id.clone(), id.clone());
            Select::new(format!("choice-{}", f.id), &options)
                .value(choices.iter().position(|v| v == value))
                .placeholder(f.placeholder.clone().unwrap_or_else(|| "Choose a value".into()))
                .label(f.label.clone())
                .width(460.0)
                .disabled(disabled)
                .open(s.mcp_ui.install.choice_open.as_deref() == Some(f.id.as_str()))
                .on_open(move |s: &mut State| s.mcp_ui.install.choice_open = Some(open_id.clone()))
                .on_pick(move |s: &mut State, i| {
                    s.mcp_ui.install.choice_open = None;
                    if let Some(v) = choices.get(i) {
                        s.install_pick(&pick_id, v);
                    }
                })
                .on_close(|s: &mut State| s.mcp_ui.install.choice_open = None)
                .build(t, menus)
        }
        kind => {
            let edit = s.mcp_ui.install.edits.get(&f.id).cloned().unwrap_or_default();
            let (edit_id, typed_id) = (id.clone(), id.clone());
            let input = Input::new(&edit).placeholder(f.placeholder.clone().unwrap_or_default()).label(f.label.clone()).invalid(error.is_some()).disabled(disabled).build(t);
            let input = if kind == Control::Secret { input.secure() } else { input };
            input
                .key("input")
                .grow(1.0)
                .min_w(0.0)
                .on_edit(move |s: &mut State| s.mcp_ui.install.edits.entry(edit_id.clone()).or_default())
                .on_input_change(move |s: &mut State, _| s.install_typed(&typed_id))
        }
    };
    let mut out = col().key(format!("field-{}", f.id)).w_full().gap(6.0).items_stretch().child(title).child(control);
    if let Some(d) = &f.description {
        out = out.child(text(d.clone()).font(sans(11.0, 400.0, 15.0)).color(c.muted_foreground).wrap());
    }
    if let Some(e) = error {
        out = out.child(text(e.clone()).font(sans(12.0, 400.0, 16.0)).color(c.destructive).key("error"));
    }
    out
}

fn step(t: &'static Tokens, title: String, height: f32, body: Vec<El>, footer: Vec<El>, layer: &mut Vec<El>) {
    let content = col().key("install-body").w_full().gap(16.0).items_stretch().children(body);
    let mut d = Dialog::new(KEY, title).width(512.0).child(col().key("install-scroll").w_full().max_h((height * 0.9 - DIALOG_CHROME).max(160.0)).child(content).scroll(rune::ScrollSpec { stick_bottom: false }));
    for f in footer {
        d = d.footer(f);
    }
    d.open(true).on_close(|s: &mut State| s.install_dismiss()).build(t, layer);
}

fn button(t: &Tokens, key: &str, label: &str, outline: bool, disabled: bool, f: fn(&mut State)) -> El {
    let b = Button::new(label).size(ButtonSize::Sm).disabled(disabled);
    let b = if outline { b.variant(ButtonVariant::Outline) } else { b };
    b.build(t).key(key.to_owned()).on_press(move |s: &mut State| f(s))
}

fn describe(t: &Tokens, words: &str) -> El {
    text(words.to_owned()).font(sans(14.0, 400.0, 20.0)).color(t.colors.muted_foreground).wrap().key("description")
}

/// `McpCatalogInstallDialog`: the step the flow is at.
pub fn install_dialog(s: &State, t: &'static Tokens, _phone: bool, height: f32, layer: &mut Vec<El>) {
    let flow = &s.mcp_ui.install.flow;
    match flow {
        Flow::Idle => {}
        Flow::Consent { plan, .. } | Flow::Committing { plan, .. } => {
            let busy = matches!(flow, Flow::Committing { .. });
            let verb = if plan["source"]["kind"] == "import" { "Import" } else { "Install" };
            let mut body = col().key("consent").w_full().gap(16.0).items_stretch().child(review(t, &plan["consentView"], plan["launchDigest"].as_str().unwrap_or_default()));
            let sync_off = matches!(&s.mcp_ui.catalog.status, CatalogStatus::Ready { catalog, .. } if model::settings::mcp_catalog::sync_state(catalog) == "disabled");
            if sync_off {
                body = body.child(
                    row()
                        .key("sync-off")
                        .w_full()
                        .gap(8.0)
                        .items_center()
                        .child(toned_notice(t, "sync-notice", Tone::Info, "Catalog sync is off, so this listing may be out of date.").grow(1.0).min_w(0.0))
                        .child(Button::new("Turn on sync").variant(ButtonVariant::Outline).size(ButtonSize::Xs).build(t).key("turn-on-sync").on_press(|s: &mut State| s.catalog_set_sync(true))),
                );
            }
            Dialog::new(KEY, format!("{verb} {}", plan["name"].as_str().unwrap_or_default()))
                .description("Review exactly what will run. Nothing is saved until you approve.")
                .width(576.0)
                .child(col().key("install-scroll").w_full().max_h((height * 0.9 - DIALOG_CHROME).max(160.0)).child(body).scroll(rune::ScrollSpec { stick_bottom: false }))
                .footer(button(t, "cancel", "Cancel", true, busy, State::install_dismiss))
                .footer(button(t, "approve", if busy { "Installing…" } else { "Approve" }, false, busy, State::install_approve))
                .open(true)
                .on_close(|s: &mut State| s.install_dismiss())
                .build(t, layer);
        }
        Flow::Form { filling, .. } | Flow::Planning(filling) => {
            let planning = matches!(flow, Flow::Planning(_));
            let (error, errors) = match flow {
                Flow::Form { error, field_errors, .. } => (error.clone(), field_errors.clone()),
                _ => (None, Default::default()),
            };
            let mut menus = Vec::new();
            let mut form = col().key("mcp-install-form").w_full().gap(20.0).items_stretch();
            for (kind, heading) in rules::FIELD_GROUPS {
                let group: Vec<&Field> = filling.fields.iter().filter(|f| f.target.kind == kind).collect();
                if group.is_empty() {
                    continue;
                }
                let mut set = col().key(heading.to_lowercase()).role("group").label(heading).w_full().gap(12.0).items_stretch().child(text(heading).font(sans(12.0, 500.0, 17.0)).color(t.colors.muted_foreground));
                for f in group {
                    let value = filling.values.get(&f.id).map_or("", String::as_str);
                    set = set.child(field(s, t, f, value, errors.get(&f.id), planning, &mut menus));
                }
                form = form.child(set);
            }
            let mut body = vec![describe(t, "Fill in what this server needs. Next, you review exactly what will run."), form];
            if let Some(e) = error {
                body.push(toned_notice(t, "form-error", Tone::Danger, &e));
            }
            let footer = vec![button(t, "cancel", "Cancel", true, false, State::install_dismiss), button(t, "review", if planning { "Checking…" } else { "Review" }, false, planning, State::install_submit)];
            step(t, format!("Install {}", filling.target.title), height, body, footer, layer);
            layer.extend(menus);
        }
        Flow::Describing(target) => {
            let body = vec![pending(t, "describing", "Loading what this server needs…")];
            step(t, format!("Install {}", target.title), height, body, vec![button(t, "cancel", "Cancel", true, false, State::install_dismiss)], layer);
        }
        Flow::Probing { server, .. } | Flow::Done { server, .. } => {
            let name = server["name"].as_str().unwrap_or_default();
            let id = server["id"].as_str().unwrap_or_default();
            let status = match flow {
                Flow::Done { probe: Probe::SignInRequired, .. } => {
                    let acp = &s.chat.acp;
                    if s.mcp_flag(&acp.mcp_oauth_connected, id) {
                        toned_notice(t, "probe", Tone::Success, "Signed in.")
                    } else {
                        let connecting = s.mcp_flag(&acp.mcp_oauth_connecting, id);
                        let words = if connecting { "Finish signing in in the browser window that opened.".to_owned() } else { Probe::SignInRequired.text() };
                        row()
                            .key("sign-in-row")
                            .w_full()
                            .gap(8.0)
                            .items_center()
                            .child(toned_notice(t, "probe", Tone::Info, &words).grow(1.0).min_w(0.0))
                            .child(Button::new(if connecting { "Signing in…" } else { "Sign in" }).variant(ButtonVariant::Outline).size(ButtonSize::Xs).disabled(connecting).build(t).key("sign-in").on_press(|s: &mut State| s.install_sign_in()))
                    }
                }
                Flow::Done { probe, .. } => toned_notice(t, "probe", if matches!(probe, Probe::Connected(_)) { Tone::Success } else { Tone::Warning }, &probe.text()),
                _ => pending(t, "probing", "Checking that it responds…"),
            };
            let body = vec![describe(t, "It's in your MCP servers list, turned on."), status];
            step(t, format!("Installed {name}"), height, body, vec![button(t, "done", "Done", false, false, State::install_dismiss)], layer);
        }
        Flow::Failed { target, failure } => {
            let body = vec![describe(t, "Nothing was saved."), toned_notice(t, "failure", Tone::Danger, &failure.text())];
            let mut footer = vec![button(t, "close", "Close", true, false, State::install_dismiss)];
            if failure.can_start_over() {
                footer.push(button(t, "start-over", "Start over", false, false, State::install_start_over));
            }
            step(t, format!("Couldn't install {}", target.title), height, body, footer, layer);
        }
    }
}
