use client::ws::Command;
use serde_json::{Value, json};
use troupe_protocol::ws::WsRequestType;
use ui::composer::field::{put_pill, trigger};
use ui::composer::menu::{self, Item, Row, Skill};
use ui::composer::{Pill, view::MenuView};

use super::Host;
use crate::tests::loaded;

fn prompts(s: &crate::State) -> Vec<Value> {
    s.sync
        .sent
        .iter()
        .filter_map(|c| match c {
            Command::Call {
                kind: WsRequestType::SendPrompt,
                payload,
                ..
            } => Some(payload.clone()),
            _ => None,
        })
        .collect()
}

fn chat() -> (crate::State, Host) {
    let mut s = loaded();
    s.install_new_chat("chat-1", "agent-1");
    (s, Host::Chat("chat-1".into()))
}

#[test]
fn send_dispatches_the_composed_prompt_through_the_model() {
    let (mut s, host) = chat();
    let draft = s.draft_mut(&host);
    draft.edit.insert("Run /mat");
    let t = trigger(&draft.edit).expect("slash");
    put_pill(
        &mut draft.edit,
        t.range,
        &Pill::Skill {
            name: "matrix-review".into(),
            path: "/s/SKILL.md".into(),
        },
    );
    draft.edit.insert("now");
    s.composer_send(&host);
    let sent = prompts(&s);
    assert_eq!(sent.len(), 1, "one send_prompt");
    assert_eq!(sent[0]["agentId"], "agent-1");
    assert_eq!(
        sent[0]["content"],
        json!([{ "type": "text", "text": "# Agent Skills\n\nmatrix-review: /s/SKILL.md\n\n---\n\nRun (matrix-review) now" }])
    );
    assert_eq!(
        sent[0]["displayContent"],
        Value::Null,
        "only a send-now passes display blocks"
    );
    assert_eq!(
        s.draft(&host).map(|d| d.edit.text.as_str()),
        Some(""),
        "the draft clears"
    );
    let messages = &s.chat.acp.messages["chat-1"];
    assert_eq!(
        messages[0].blocks,
        [json!({ "type": "text", "text": "Run \u{E000}matrix-review\u{E001}\u{E002}\u{2007}\u{E003} now" })],
        "the optimistic user message keeps the pills"
    );
}

#[test]
fn a_blank_draft_sends_nothing() {
    let (mut s, host) = chat();
    s.draft_mut(&host).edit.insert("   ");
    s.composer_send(&host);
    assert!(prompts(&s).is_empty());
}

#[test]
fn the_slash_menu_lists_skills_and_a_pick_becomes_a_pill() {
    let (mut s, host) = chat();
    s.composer.skills.insert(String::new(), vec![]);
    let root = s
        .projects
        .store
        .projects
        .iter()
        .find(|p| p.id == "p-a")
        .and_then(|p| p.path.clone())
        .unwrap_or_default();
    let skill = Skill {
        name: "matrix-review".into(),
        description: "Review".into(),
        scope: "global".into(),
        path: "/s".into(),
    };
    s.composer.skills.insert(root, vec![skill.clone()]);
    s.draft_mut(&host).edit.insert("/ma");
    let Some(MenuView { sections, .. }) = s.menu_view(&host) else {
        panic!("the slash menu opens")
    };
    assert_eq!(sections[0].heading.as_deref(), Some("Skills"));
    assert_eq!(sections[0].items[0].label, "/matrix-review");
    let mut tx = rune::Tx::default();
    s.composer_pick(
        &host,
        &Item {
            label: "/matrix-review".into(),
            description: None,
            row: Row::Skill(skill),
        },
        &mut tx,
    );
    let doc = ui::composer::field::doc_of(&s.draft(&host).expect("draft").edit);
    assert_eq!(
        doc.display_text(),
        "\u{E000}matrix-review\u{E001}\u{E002}\u{2007}\u{E003} "
    );
    assert!(s.menu_view(&host).is_none(), "the menu closes");
}

#[test]
fn a_second_command_is_refused() {
    let (mut s, host) = chat();
    s.draft_mut(&host).edit.insert("/a");
    let command = Item {
        label: "/a".into(),
        description: None,
        row: Row::Command {
            name: "a".into(),
            description: None,
            hint: None,
        },
    };
    let mut tx = rune::Tx::default();
    s.composer_pick(&host, &command, &mut tx);
    s.draft_mut(&host).edit.insert("/b");
    s.composer_pick(&host, &command, &mut tx);
    let doc = ui::composer::field::doc_of(&s.draft(&host).expect("draft").edit);
    assert_eq!(doc.pills().count(), 1, "one command per draft");
}

#[test]
fn return_in_the_launcher_starts_a_chat() {
    use rune::{Key, KeyEvent, Mods};
    let mut rt = crate::tests::runtime_at((1280.0, 800.0));
    rt.state
        .agents_loaded(Some(json!([{ "id": "cfg", "name": "Chat Scenario Agent" }])));
    rt.frame(0.05);
    let root = rt
        .state
        .projects
        .store
        .projects
        .iter()
        .find(|p| p.id == "p-a")
        .and_then(|p| p.path.clone())
        .unwrap_or_default();
    let skill = Skill {
        name: "matrix-review".into(),
        description: "Review".into(),
        scope: "global".into(),
        path: "/s/SKILL.md".into(),
    };
    rt.state.composer.skills.insert(root, vec![skill]);
    let key = "composer-launcher-p-a-field";
    crate::tests::tap(&mut rt, key);
    let typed = |rt: &mut rune::Runtime<crate::State>, text: &str| {
        for c in text.chars() {
            rt.key(KeyEvent {
                key: Key::Text(c.to_string()),
                mods: Mods::default(),
            });
        }
        rt.frame(0.1);
    };
    typed(&mut rt, "Use /matrix");
    rt.key(KeyEvent {
        key: Key::Enter,
        mods: Mods::default(),
    });
    rt.frame(0.12);
    typed(&mut rt, "on it");
    rt.key(KeyEvent {
        key: Key::Enter,
        mods: Mods::default(),
    });
    rt.frame(0.15);
    let spawn = rt.state.sync.sent.iter().find_map(|c| match c {
        Command::Call {
            kind: WsRequestType::SpawnAgent,
            payload,
            ..
        } => Some(payload.clone()),
        _ => None,
    });
    let spawn = spawn.expect("Return sends the launcher's draft");
    assert_eq!(
        spawn["config"],
        json!({ "configId": "cfg", "name": "Chat Scenario Agent" }),
        "the host takes configId, not the stored id"
    );
}

#[test]
fn a_refused_prompt_shows_the_host_error_once_prefixed() {
    let (mut s, host) = chat();
    s.draft_mut(&host).edit.insert("hello");
    s.composer_send(&host);
    let turn = prompts(&s)[0]["turnId"].as_str().expect("turn id").to_owned();
    let error = troupe_protocol::ws::WsError {
        code: "agent_error".into(),
        message: "agent busy elsewhere".into(),
        details: None,
    };
    s.prompt_settled("chat-1".into(), turn, Err(error));
    let session = &s.chat.acp.sessions["chat-1"];
    assert_eq!(session.last_error.as_deref(), Some("Error: agent busy elsewhere"));
}

#[test]
fn send_waits_for_the_chat_to_load_and_keeps_the_draft() {
    let mut s = loaded();
    let host = Host::Chat("chat-9".into());
    s.draft_mut(&host).edit.insert("early");
    s.composer_send(&host);
    assert!(prompts(&s).is_empty());
    assert_eq!(s.draft(&host).map(|d| d.edit.text.as_str()), Some("early"));
}

#[test]
fn a_phone_shows_the_chat_the_launcher_started() {
    let mut s = crate::tests::showing_alpha();
    s.apply(client::ws::Output::Reply {
        call: 7,
        result: Ok(None),
    });
    s.apply(client::ws::Output::Reply {
        call: 8,
        result: Ok(None),
    });
    assert!(s.workspace.record().is_some(), "alpha's workspace shows");
    let mut rt = rune::Runtime::new(s, crate::view, (390.0, 844.0), rune::Env::new(true, 1.0));
    rt.frame(0.0);
    assert!(
        crate::tests::present(&rt, "launcher-p-a"),
        "the phone starts on the launcher"
    );
    rt.update_if(|s, _| {
        s.install_new_chat("chat-1", "agent-1");
        s.workspace
            .step(model::workspace::Event::AddAgentChatTab("chat-1".into(), None));
        true
    });
    rt.frame(0.05);
    rt.frame(0.1);
    assert!(
        crate::tests::present(&rt, "composer-dock-chat-1"),
        "the new chat's composer shows"
    );
}

#[test]
fn a_prompt_goes_out_ledgered_so_a_reconnect_resends_it() {
    let (mut s, host) = chat();
    s.draft_mut(&host).edit.insert("hello");
    s.composer_send(&host);
    let ledgered: Vec<bool> = s
        .sync
        .sent
        .iter()
        .filter_map(|c| match c {
            Command::Call {
                kind: WsRequestType::SendPrompt,
                ledgered,
                ..
            } => Some(*ledgered),
            _ => None,
        })
        .collect();
    assert_eq!(ledgered, [true]);
}

#[test]
fn a_busy_host_queues_the_prompt_and_the_retry_sends_it() {
    let (mut s, host) = chat();
    s.draft_mut(&host).edit.insert("hello");
    s.composer_send(&host);
    let turn = prompts(&s)[0]["turnId"].as_str().expect("turn id").to_owned();
    let busy = troupe_protocol::ws::WsError {
        code: "rate_limited".into(),
        message: "a prompt turn is already in progress".into(),
        details: None,
    };
    s.prompt_settled("chat-1".into(), turn, Err(busy));
    assert_eq!(s.chat.acp.sessions["chat-1"].last_error, None, "busy is not a failure");
    assert!(s.toasts.visible().is_empty(), "no failure toast");

    let (fired, next) = s.fire_retries(s.now + 1.0);
    assert!(!fired, "the first retry waits two seconds");
    assert_eq!(next, Some(s.now + 2.0));
    let (fired, _) = s.fire_retries(s.now + 2.0);
    assert!(fired);
    let sent = prompts(&s);
    assert_eq!(sent.len(), 2, "the retry sends the queued prompt");
    assert_eq!(sent[1]["content"], json!([{ "type": "text", "text": "hello" }]));
}

#[test]
fn the_chip_names_the_agent_of_the_chat_config_exactly() {
    let (mut s, host) = chat();
    let agent = |id: &str, name: &str| crate::composer::AgentConfig { id: id.into(), name: name.into(), raw: Value::Null };
    s.composer.agents = vec![agent("claude", "Claude"), agent("claude-code", "Claude Code")];
    s.composer.namespaces.insert("chat-1".into(), "config:claude-code".into());
    assert_eq!(s.agent_label(&host).as_deref(), Some("Claude Code"));
}

#[test]
fn the_launcher_chip_switches_the_agent_and_remembers_it() {
    let mut rt = crate::tests::runtime_at((1280.0, 800.0));
    rt.update_if(|s, _| {
        s.agents_loaded(Some(json!([
            { "id": "cfg-a", "name": "Chat Scenario Agent" },
            { "id": "cfg-b", "name": "Second Agent" }
        ])));
        true
    });
    rt.frame(0.05);
    crate::tests::tap(&mut rt, "composer-launcher-p-a-agent");
    assert!(
        crate::tests::present(&rt, "composer-launcher-p-a-agent-menu-item1"),
        "the chip opens the agent menu"
    );
    crate::tests::tap(&mut rt, "composer-launcher-p-a-agent-menu-item1");
    let host = Host::Launcher("p-a".into());
    assert_eq!(rt.state.agent_label(&host).as_deref(), Some("Second Agent"));
    let saved: Vec<Value> = rt
        .state
        .sync
        .sent
        .iter()
        .filter_map(|c| match c {
            Command::Call {
                kind: WsRequestType::StoreWrite,
                payload,
                ..
            } if payload["key"].as_str().is_some_and(|k| k.ends_with("/agents/last-selected")) => {
                Some(payload["value"]["data"].clone())
            }
            _ => None,
        })
        .collect();
    assert_eq!(saved, [json!({ "agentId": "cfg-b", "mode": "acp" })]);
    assert!(!rt.state.draft(&host).is_some_and(|d| d.agent_open), "the menu closes");
}

#[test]
fn the_explorer_lists_a_folder_and_a_file_pick_sends_a_resource_link() {
    let (mut s, host) = chat();
    let root = s.root_of(&host).expect("chat-1 is in alpha");
    let mut tx = rune::Tx::default();
    let pick = |s: &mut crate::State, label: &str, tx: &mut rune::Tx| {
        let view = s.menu_view(&host).expect("the mention menu is open");
        let item = menu::flat(&view.sections)
            .into_iter()
            .find(|i| i.label == label)
            .unwrap_or_else(|| panic!("no row {label}"))
            .clone();
        s.composer_pick(&host, &item, tx);
    };
    s.draft_mut(&host).edit.insert("Read @");
    pick(&mut s, "Explorer", &mut tx);
    let view = s.menu_view(&host).expect("explorer");
    assert_eq!(view.title.as_deref(), Some("Explorer"));
    assert_eq!(view.sections[0].notice.as_deref(), Some("Loading…"));
    s.listing_read(
        root.clone(),
        &root,
        Ok(json!([
            { "name": "src", "path": "/x", "type": "directory", "extension": null, "size": 0, "modifiedAt": 0 },
            { "name": "README.md", "path": "/x", "type": "file", "extension": "md", "size": 1, "modifiedAt": 0 }
        ])),
    );
    pick(&mut s, "src", &mut tx);
    let view = s.menu_view(&host).expect("src");
    assert_eq!(view.crumb, "src/");
    assert_eq!(menu::flat(&view.sections)[0].label, "Use this folder");
    s.listing_read(
        format!("{root}/src"),
        &root,
        Ok(json!([{ "name": "main.rs", "path": "/x", "type": "file", "extension": "rs", "size": 1, "modifiedAt": 0 }])),
    );
    pick(&mut s, "main.rs", &mut tx);
    assert!(s.menu_view(&host).is_none(), "the menu closes");
    s.composer_send(&host);
    let sent = prompts(&s);
    let path = format!("{root}/src/main.rs");
    assert_eq!(
        sent[0]["content"],
        json!([
            { "type": "text", "text": "Read (main.rs)" },
            { "type": "resource_link", "uri": ui::composer::file_url(&path), "name": "main.rs", "mimeType": "text/x-rust" }
        ])
    );
}

#[test]
fn the_phone_plus_opens_the_add_sheet_and_mention_puts_an_at_sign() {
    let mut s = crate::tests::showing_alpha();
    s.apply(client::ws::Output::Reply { call: 7, result: Ok(None) });
    s.apply(client::ws::Output::Reply { call: 8, result: Ok(None) });
    let mut rt = rune::Runtime::new(s, crate::view, (390.0, 844.0), rune::Env::new(true, 1.0));
    rt.frame(0.0);
    crate::tests::tap(&mut rt, "composer-launcher-p-a-add");
    assert!(crate::tests::present(&rt, "add-tile-camera"), "the sheet shows its tiles");
    crate::tests::tap(&mut rt, "add-tools");
    assert!(crate::tests::present(&rt, "add-tool-browser"), "Troupe tools is a sub-view");
    crate::tests::tap(&mut rt, "back");
    crate::tests::tap(&mut rt, "add-mention");
    assert!(!crate::tests::present(&rt, "add-tile-camera"), "the sheet closes");
    let host = Host::Launcher("p-a".into());
    assert_eq!(rt.state.draft(&host).map(|d| d.edit.text.as_str()), Some("@"));
    assert!(rt.state.menu_view(&host).is_some(), "the mention menu opens");
}
