//! The independent review's findings on the composer: pasted image limits and the agent's
//! image support, the chat's own folder and agent, Explorer without a folder, and the
//! launcher's draft while its chat starts.

use rune::{ClipboardContent, ClipboardImage};
use serde_json::json;
use ui::composer::menu;

use crate::composer::Host;
use crate::composer_agents::{MAX_IMAGE_BYTES, NO_IMAGES};
use crate::tests::loaded;

/// A PNG of `len` bytes: a 1 by 1 header, then padding.
fn png(len: usize) -> ClipboardContent {
    let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR\0\0\0\x01\0\0\0\x01".to_vec();
    bytes.resize(len, 0);
    ClipboardContent::new("").with_image(ClipboardImage::from_png(bytes).expect("a PNG"))
}

fn chat() -> (crate::State, Host) {
    let mut s = loaded();
    s.install_new_chat("chat-1", "agent-1");
    (s, Host::Chat("chat-1".into()))
}

fn toasts(s: &crate::State) -> Vec<String> {
    s.toasts.visible().iter().map(|t| t.title.clone()).collect()
}

fn mention_labels(s: &mut crate::State, host: &Host) -> Vec<String> {
    s.draft_mut(host).edit.insert("@");
    let view = s.menu_view(host).expect("the mention menu is open");
    menu::flat(&view.sections).into_iter().map(|i| i.label.clone()).collect()
}

#[test]
fn a_pasted_image_over_10_mb_is_refused_as_too_large() {
    let (mut s, host) = chat();
    assert!(s.composer_paste(&host, &png(MAX_IMAGE_BYTES + 1)));
    assert!(s.draft(&host).is_none_or(|d| d.attachments.is_empty()));
    assert_eq!(toasts(&s), ["File too large"]);
    assert!(s.composer_paste(&host, &png(MAX_IMAGE_BYTES)));
    assert_eq!(s.draft(&host).map(|d| d.attachments.len()), Some(1), "10 MB itself is allowed");
}

#[test]
fn an_agent_without_image_prompts_refuses_a_pasted_image() {
    let (mut s, host) = chat();
    s.chat.acp.agents.insert("agent-1".into(), json!({ "capabilities": { "promptCapabilities": { "image": false } } }));
    assert!(s.composer_paste(&host, &png(64)));
    assert!(s.draft(&host).is_none_or(|d| d.attachments.is_empty()));
    assert_eq!(toasts(&s), [NO_IMAGES]);
    s.chat.acp.agents.insert("agent-1".into(), json!({ "capabilities": { "promptCapabilities": { "image": true } } }));
    assert!(s.composer_paste(&host, &png(64)));
    assert_eq!(s.draft(&host).map(|d| d.attachments.len()), Some(1));
}

#[test]
fn a_launch_with_an_image_for_an_agent_without_images_keeps_the_draft() {
    let mut s = crate::tests::showing_alpha();
    s.apply(client::ws::Output::Reply { call: 7, result: Ok(None) });
    s.apply(client::ws::Output::Reply { call: 8, result: Ok(None) });
    s.composer.launching = Some(crate::launch::Launch {
        project_id: "p-a".into(),
        cwd: "/w/alpha".into(),
        config_id: "cfg".into(),
        send: ui::composer::Send::Blocks {
            wire: vec![json!({ "type": "image", "mimeType": "image/png", "data": "AA==" })],
            display: None,
        },
    });
    let sent = s.sync.sent.len();
    s.launch_spawned(Ok(Some(json!({ "agentId": "agent-9", "capabilities": { "promptCapabilities": { "image": false } }, "authMethods": [], "hostAuthReady": false }))));
    assert!(s.composer.launching.is_none());
    assert_eq!(s.sync.sent.len(), sent, "no session is created");
    assert_eq!(toasts(&s), [NO_IMAGES]);
    assert!(s.chat.acp.agents.contains_key("agent-9"), "the spawn reply registers the agent");
}

#[test]
fn a_chat_reads_mentions_and_skills_from_its_own_folder() {
    let (mut s, host) = chat();
    let mut list = s.chats.list.clone();
    list[0].cwd = Some("/w/alpha-wt".into());
    s.chats.replace(list);
    assert_eq!(s.root_of(&host).as_deref(), Some("/w/alpha-wt"));
    let view_root = mention_labels(&mut s, &host);
    assert!(view_root.contains(&"Explorer".to_owned()));
    let view = s.menu_view(&host).expect("menu");
    let explorer = menu::flat(&view.sections).into_iter().find(|i| i.label == "Explorer").expect("row").clone();
    assert_eq!(explorer.description.as_deref(), Some("Files and folders in alpha-wt/"));
}

#[test]
fn a_chat_with_no_folder_offers_no_explorer() {
    let mut s = loaded();
    s.install_new_chat("chat-q", "agent-1");
    let host = Host::Chat("chat-q".into());
    assert_eq!(s.root_of(&host), None);
    assert_eq!(mention_labels(&mut s, &host), ["Conversations"]);
}

#[test]
fn a_chat_chip_names_the_chats_own_agent_not_the_launcher_pick() {
    let (mut s, host) = chat();
    s.agents_loaded(Some(json!([{ "id": "cfg-a", "name": "A" }, { "id": "cfg-b", "name": "B" }])));
    s.last_agent_loaded(Some(json!({ "agentId": "cfg-a", "mode": "acp" })));
    s.composer.namespaces.insert("chat-1".into(), "f3a9c2".into());
    assert_eq!(s.agent_label(&host), None, "a namespace that names no config shows no agent");
    s.chat.acp.config_to_live_agent.insert("cfg-b\0/w/alpha\0p-a".into(), json!("agent-1"));
    assert_eq!(s.agent_label(&host).as_deref(), Some("B"), "the live agent's config");
}

#[test]
fn the_launcher_draft_takes_no_edits_while_its_chat_starts() {
    let mut s = loaded();
    let host = Host::Launcher("p-a".into());
    s.draft_mut(&host).edit.insert("go");
    s.composer.launching = Some(crate::launch::Launch {
        project_id: "p-a".into(),
        cwd: "/w/alpha".into(),
        config_id: "cfg".into(),
        send: ui::composer::Send::Text { text: "go".into() },
    });
    let mut tx = rune::Tx::default();
    s.composer_add(&host, "@", &mut tx);
    s.composer_add(&host, "browser", &mut tx);
    assert!(s.composer_paste(&host, &png(64)));
    s.open_add_sheet(&host);
    let draft = s.draft(&host).expect("draft");
    assert_eq!(draft.edit.text, "go");
    assert!(draft.attachments.is_empty());
    assert!(!s.overlays.is_open(crate::composer_sheet::ADD_SHEET));
}
