use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use sha1::{Digest, Sha1};

use super::{Attachment, ChatContexts, Doc, Part, Pill, compose, create_context};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..")
}

fn vectors() -> Value {
    let path = repo_root().join("tools/parity/vectors/out/composer/composer.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).expect("composer vectors")
}

fn str_of(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap_or_default().to_owned()
}

/// A recorded document part (`DocPart` in `composer.ts`).
fn part(v: &Value) -> Part {
    let has = |k: &str| v.get(k).is_some();
    let pill = if has("text") {
        return Part::Text(str_of(v, "text"));
    } else if has("skill") {
        Pill::Skill {
            name: str_of(v, "skill"),
            path: str_of(v, "path"),
        }
    } else if has("command") {
        Pill::Command {
            name: str_of(v, "command"),
        }
    } else if has("file") {
        Pill::File {
            display: str_of(v, "file"),
            abs_path: str_of(v, "path"),
        }
    } else if has("context") {
        Pill::Context {
            text: str_of(v, "context"),
            language: v["language"].as_str().map(str::to_owned),
        }
    } else if has("conversation") {
        Pill::Conversation {
            title: str_of(v, "conversation"),
            session_id: str_of(v, "sessionId"),
        }
    } else {
        Pill::Tool { id: str_of(v, "tool") }
    };
    Part::Pill(pill)
}

fn doc(v: &Value) -> Doc {
    Doc {
        parts: v.as_array().into_iter().flatten().map(part).collect(),
    }
}

fn send_json(vector: &Value) -> Value {
    let skills: Vec<(String, String)> = vector["skills"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|s| (str_of(s, "name"), str_of(s, "path")))
        .collect();
    let chats: Option<ChatContexts> = vector
        .get("contexts")
        .map(|c| serde_json::from_value(c.clone()).unwrap());
    let attachments: Vec<Attachment> = vector
        .get("attachments")
        .map(|a| serde_json::from_value(a.clone()).expect("attachments"))
        .unwrap_or_default();
    match compose(&doc(&vector["doc"]), &skills, chats.as_ref(), &attachments) {
        Ok(Some(send)) => serde_json::to_value(send).unwrap(),
        Ok(None) => Value::Null,
        Err(e) => json!({ "error": e.to_string() }),
    }
}

#[test]
fn every_recorded_document_serializes_and_sends_as_the_web_client_does() {
    let vectors = vectors();
    let mut failures = Vec::new();
    let documents = vectors["documents"].as_array().expect("documents");
    for vector in documents {
        let name = str_of(vector, "name");
        let display = doc(&vector["doc"]).display_text();
        if display != vector["display"] {
            failures.push(format!(
                "{name}: display\n  want {:?}\n  got  {display:?}",
                vector["display"]
            ));
        }
        if super::parse_display(&display).display_text() != display {
            failures.push(format!("{name}: the display string does not parse back to itself"));
        }
        let copy = super::wire::copy_text(&display);
        if copy != vector["copy"] {
            failures.push(format!("{name}: copy\n  want {:?}\n  got  {copy:?}", vector["copy"]));
        }
        let send = send_json(vector);
        if send != vector["send"] {
            failures.push(format!("{name}: send\n  want {}\n  got  {send}", vector["send"]));
        }
    }
    assert!(documents.len() >= 18, "only {} documents recorded", documents.len());
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn one_pill_of_each_type_sends_the_exact_prompt() {
    let doc = Doc {
        parts: vec![
            Part::Text("Use ".into()),
            Part::Pill(Pill::Skill {
                name: "parity-skill".into(),
                path: "/w/.claude/skills/parity-skill/SKILL.md".into(),
            }),
            Part::Text(" on ".into()),
            Part::Pill(Pill::File {
                display: "README.md".into(),
                abs_path: "/w/README.md".into(),
            }),
            Part::Text(" and ".into()),
            Part::Pill(Pill::Command { name: "review".into() }),
            Part::Text(" with ".into()),
            Part::Pill(Pill::Conversation {
                title: "Scripted chat".into(),
                session_id: "sess-1".into(),
            }),
            Part::Text(", ".into()),
            Part::Pill(Pill::Tool { id: "browser".into() }),
            Part::Text("\nabout ".into()),
            Part::Pill(Pill::Context {
                text: "line one\n\nline three".into(),
                language: None,
            }),
        ],
    };
    let Ok(Some(super::Send::Blocks { wire, .. })) = compose(&doc, &[], None, &[]) else {
        panic!("a pill send goes as blocks");
    };
    assert_eq!(
        wire[0]["text"],
        "# Agent Skills\n\nparity-skill: /w/.claude/skills/parity-skill/SKILL.md\n\n---\n\n\
         Context 1 (selected from the conversation):\n> line one\n>\n> line three\n\n\
         Use (parity-skill) on (README.md) and /review with \
         (conversation: Scripted chat, transcript unavailable [termul-chat:sess-1]), \
         (Use the browser tool from the troupe MCP server)\nabout [Context 1]"
    );
    assert_eq!(
        wire[1],
        json!({ "type": "resource_link", "uri": "file:///w/README.md", "name": "README.md", "mimeType": "text/markdown" })
    );
}

#[test]
fn selected_text_becomes_the_same_context() {
    for case in vectors()["contexts"].as_array().expect("contexts") {
        let got = create_context(case["raw"].as_str().unwrap(), case["language"].as_str());
        assert_eq!(serde_json::to_value(&got).unwrap(), case["context"], "{case}");
    }
}

#[test]
fn vectors_are_not_older_than_lib_composer() {
    let stale: Vec<String> = vectors()["sources"]
        .as_object()
        .expect("sources")
        .iter()
        .filter(|(path, recorded)| {
            let text = std::fs::read_to_string(repo_root().join(path))
                .unwrap()
                .replace("\r\n", "\n");
            let hash: String = Sha1::digest(text.as_bytes())
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            hash != recorded.as_str().unwrap_or_default()
        })
        .map(|(path, _)| path.clone())
        .collect();
    assert!(
        stale.is_empty(),
        "TypeScript changed after the composer vectors were recorded; rerun \
         `RECORD_VECTORS=1 bun run test tools/parity/vectors` and port the change:\n{}",
        stale.join("\n")
    );
}
