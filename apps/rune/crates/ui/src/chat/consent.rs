//! `BrowserConsentCard`: the in-chat card for a host browser consent ticket, which asks before
//! the agent's browser opens a site or a file, or sends a password, payment, upload, download or
//! page data. Only the host's ticket fields show; the agent's own words are labelled as its.
//! Each option sends exactly the option the user pressed (`respondBrowserConsent`).

use rune::{El, col, icon, row, text};
use serde_json::Value;
use theme::Tokens;
use theme::icons as i;

use crate::type_::{MICRO, mono, sans};
use crate::{Button, ButtonSize, ButtonVariant};

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

fn sensitive(action: &str) -> bool {
    !matches!(action, "navigate" | "local_network")
}

/// `browserConsentTitle`.
pub fn title(action: &str, file: bool) -> &'static str {
    if file {
        return "Allow the agent to open this file?";
    }
    match action {
        "password_submit" => "Allow the agent to submit a password?",
        "payment_submit" => "Allow the agent to submit payment details?",
        "upload" => "Allow the agent to upload a file?",
        "download" => "Allow the agent to keep a download?",
        "tainted_exfil" => "Allow the agent to send page data to another site?",
        _ => "Allow the agent to open this site?",
    }
}

/// `browserConsentSummary`.
pub fn summary(agent: &str, action: &str, origin: &str, file: bool) -> String {
    if file {
        return format!("{agent} wants to open the file {origin} in the agent browser.");
    }
    let phrase = match action {
        "local_network" => "open a local network address at",
        "password_submit" => "submit a password on",
        "payment_submit" => "submit payment details on",
        "upload" => "upload a file to",
        "download" => "download a file from",
        "tainted_exfil" => "send page data to",
        _ => "open",
    };
    format!("{agent} wants to {phrase} {origin} in the agent browser.")
}

/// `browserConsentOptionLabel`: sensitive actions and files are allowed one at a time.
pub fn option_label(option: &str, action: &str, file: bool) -> &'static str {
    let one_at_a_time = sensitive(action) || file;
    match (option, one_at_a_time) {
        ("allow_session", false) => "Allow for this session",
        ("allow_project", false) => "Always allow for this project",
        ("allow_session", true) => "Allow once",
        ("allow_project", true) => "Always allow on this site for this project",
        _ => "Deny",
    }
}

/// The card for `ticket`, from `agent`; `answer` sends the pressed option.
pub fn card<S: 'static>(t: &'static Tokens, ticket: &Value, agent: &str, phone: bool, answer: fn(&mut S, String, String)) -> El {
    let c = &t.colors;
    let request = s(ticket, "requestId").unwrap_or_default().to_owned();
    let action = s(ticket, "action").unwrap_or("navigate");
    let file = s(ticket, "targetKind") == Some("file");
    let origin = s(ticket, "origin").unwrap_or_default();
    let target_url = s(ticket, "targetUrl").unwrap_or_default();
    let bytes = ticket.get("targetUrlBytes").and_then(Value::as_u64).unwrap_or(target_url.len() as u64);
    let address = if bytes as usize > target_url.chars().count() { format!("{target_url} ({bytes} bytes)") } else { target_url.to_owned() };
    let small = sans(MICRO, 400.0, 19.5);
    let strong = sans(MICRO, 500.0, 19.5);
    let term = |label: &str| text(label.to_owned()).font(small).color(c.muted_foreground).w(64.0);
    let mut facts = col().w_full().mt(8.0).gap(2.0);
    let fact = |label: &str, value: El| row().w_full().gap(8.0).items_start().child(term(label)).child(value.min_w(0.0).grow(1.0));
    let target = if file {
        col()
            .child(text(s(ticket, "fileName").unwrap_or(origin).to_owned()).font(strong).color(c.foreground).wrap_anywhere())
            .child(text(origin.to_owned()).font(mono(MICRO, 400.0, 19.5)).color(c.muted_foreground).wrap_anywhere())
    } else {
        col().child(text(origin.to_owned()).font(strong).color(c.foreground).single_line().truncate())
    };
    facts = facts
        .child(fact(if file { "File" } else { "Site" }, target))
        .child(fact("Address", text(address).font(mono(MICRO, 400.0, 19.5)).color(c.muted_foreground).wrap_anywhere()));
    if let Some(f) = ticket.get("file") {
        let line = format!("{} ({} bytes)", s(f, "name").unwrap_or_default(), f.get("bytes").and_then(Value::as_u64).unwrap_or(0));
        facts = facts.child(fact("File", text(line).font(strong).color(c.foreground).wrap()));
    }
    if let Some(said) = s(ticket, "element") {
        facts = facts.child(fact("Agent says", text(format!("\u{201c}{said}\u{201d}")).font(small).color(c.muted_foreground).wrap()));
    }
    let profile = if s(ticket, "profile") == Some("ephemeral") {
        "Ephemeral profile, discarded when the session ends"
    } else {
        "Global agent profile, shared across all projects"
    };
    facts = facts.child(fact("Profile", text(profile).font(small).color(c.muted_foreground).wrap()));
    let head = row()
        .w_full()
        .gap(10.0)
        .items_start()
        .child(row().mt(2.0).size(24.0, 24.0).center().radius(6.0).border(1.0, c.warning.alpha(0.3)).fill(c.warning.alpha(0.15)).child(icon(&i::SHIELD_ALERT, 13.0).color(c.warning)))
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .child(text(title(action, file)).font(sans(12.0, 600.0, 20.0)).color(c.foreground))
                .child(
                    text(format!("{} You'll watch it work in a visible tab.", summary(agent, action, origin, file)))
                        .font(sans(12.0, 400.0, 19.5))
                        .color(c.muted_foreground)
                        .wrap()
                        .mt(2.0),
                )
                .child(facts),
        );
    let options: Vec<String> = ticket.get("options").and_then(Value::as_array).into_iter().flatten().filter_map(|o| o.as_str().map(str::to_owned)).collect();
    let buttons = options.iter().map(|option| {
        let deny = option == "deny";
        let b = Button::new(option_label(option, action, file))
            .icon(if deny { &i::SHIELD_ALERT } else { &i::SHIELD_CHECK })
            .size(if phone { ButtonSize::Touch } else { ButtonSize::Sm });
        let b = if option == "allow_session" {
            b
        } else if deny {
            b.variant(ButtonVariant::Ghost).ink(c.muted_foreground)
        } else {
            b.variant(ButtonVariant::Outline)
        };
        let (request, option) = (request.clone(), option.clone());
        b.build(t).key(format!("browser-consent-{option}")).radius(8.0).px(12.0).on_press(move |s: &mut S| answer(s, request.clone(), option.clone()))
    });
    let card = col()
        .key(format!("browser-consent-{request}"))
        .role("region")
        .label(title(action, file))
        .w_full()
        .px(if phone { 14.0 } else { 16.0 })
        .py(12.0)
        .radius(16.0)
        .border(1.0, c.warning.alpha(0.25))
        .fill(c.card)
        .child(head)
        .child(row().w_full().mt(10.0).gap(8.0).items_center().flex_wrap().children(buttons.collect::<Vec<_>>()));
    crate::paint::shadows(card, &t.shadows.shadow_control)
}
