use client::ws::Command;
use serde_json::{Value, json};
use troupe_protocol::ws::WsRequestType;
use ui::composer::field::{put_pill, trigger};
use ui::composer::menu::{Item, Row, Skill};
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
