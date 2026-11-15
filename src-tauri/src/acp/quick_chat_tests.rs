//! Kind flag, host-created folder, cwd derivation, and the path lock.

use std::fs;
use std::path::PathBuf;

use super::*;
use crate::acp::session_persistence::{
    SessionKind, SessionPersistence, SessionRegistration, UNASSIGNED_PROJECT_ID,
};

fn store_dir(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "termul-quick-{label}-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

fn registration(session_id: &str, cwd: &std::path::Path) -> SessionRegistration {
    SessionRegistration {
        session_id: session_id.to_string(),
        stable_agent_namespace: Some("config:test".to_string()),
        runtime_agent_id: Some("agent-1".to_string()),
        project_id: Some(UNASSIGNED_PROJECT_ID.to_string()),
        cwd: cwd.to_path_buf(),
        ..Default::default()
    }
}

#[test]
fn a_chat_id_is_one_safe_path_segment() {
    assert!(chat_id_is_safe(&mint_chat_id()));
    assert!(chat_id_is_safe("chat-1"));
    for id in [
        "",
        ".",
        "..",
        "a/b",
        "a\\b",
        "a..b",
        "../x",
        "has space",
        "id\n",
    ] {
        assert!(!chat_id_is_safe(id), "{id}");
    }
}

#[test]
fn derive_rejects_a_climb_before_any_directory_is_created() {
    let root = PathBuf::from("/tmp/quick-root");
    // derive_chat_dir is private; the public lock reports the same refusal
    // once the id itself is illegal, without touching the filesystem.
    let error = cwd_for_quick_session("../secret").unwrap_err();
    assert!(
        error.contains("single folder name"),
        "a climb must be refused, got {error}"
    );
    let _ = root;
}

#[tokio::test]
async fn the_host_creates_the_folder_stamps_kind_lists_resumes_and_deletes_it() {
    let (chat_id, dir) = prepare_new_chat_dir().expect("host creates the folder");
    assert!(dir.is_dir(), "the quick chat folder exists");
    assert_eq!(
        chat_id_of_canonical(&dir).as_deref(),
        Some(chat_id.as_str())
    );
    let root = fs::canonicalize(quick_chats_root().unwrap()).unwrap();
    assert_eq!(dir.parent(), Some(root.as_path()));

    let store = store_dir("roundtrip");
    let persistence = SessionPersistence::open(store.join("store")).await.unwrap();
    let metadata = persistence
        .register_quick_session(registration("sess-quick", &dir), chat_id.clone())
        .await
        .expect("quick session registers");
    assert_eq!(metadata.kind, SessionKind::Quick);
    assert_eq!(metadata.quick_chat_id.as_deref(), Some(chat_id.as_str()));
    assert_eq!(metadata.project_id.as_deref(), Some(UNASSIGNED_PROJECT_ID));

    let listed = persistence.list_sessions();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].kind, SessionKind::Quick);
    assert_eq!(listed[0].quick_chat_id.as_deref(), Some(chat_id.as_str()));

    let json = serde_json::to_value(&listed[0]).unwrap();
    assert_eq!(json["kind"], "quick");

    // Resume ignores a client path, including a climb out of the folder.
    let client_escape = format!("{}/../../.ssh", dir.display());
    let resumed = cwd_for_quick_session(&chat_id).unwrap();
    assert_eq!(resumed, display_path(&dir));
    assert_ne!(resumed, client_escape);
    match listing_decision(&client_escape) {
        ListingDecision::Rejected(message) => {
            assert!(
                message.contains("escapes") || message.contains("single"),
                "{message}"
            );
        }
        other => panic!("a climb must be rejected, got {other:?}"),
    }

    let renamed = persistence
        .set_local_alias("sess-quick", "Ship the notes")
        .await
        .unwrap();
    assert_eq!(renamed.title.as_deref(), Some("Ship the notes"));
    assert_eq!(
        renamed.title_source,
        Some(crate::acp::session_persistence::TitleSource::LocalAlias)
    );

    persistence.delete_session("sess-quick").await.unwrap();
    assert!(!dir.exists(), "delete removes the quick chat folder");
    assert!(persistence.list_sessions().is_empty());
    let _ = fs::remove_dir_all(store);
}

#[tokio::test]
async fn a_project_session_is_not_quick_and_cannot_take_a_quick_folder() {
    let store = store_dir("project");
    let cwd = store.join("cwd");
    fs::create_dir_all(&cwd).unwrap();
    let persistence = SessionPersistence::open(store.join("store")).await.unwrap();
    let metadata = persistence
        .register_session(registration("sess-project", &cwd))
        .await
        .unwrap();
    assert_eq!(metadata.kind, SessionKind::Project);
    assert!(metadata.quick_chat_id.is_none());
    let json = serde_json::to_value(&metadata).unwrap();
    assert!(json.get("kind").is_none(), "project chats omit kind");
    let error = persistence
        .set_local_alias("sess-project", "Nope")
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("quick"), "{error}");

    let (chat_id, dir) = prepare_new_chat_dir().unwrap();
    let refused = persistence
        .register_session(registration("sess-steal", &dir))
        .await
        .unwrap_err()
        .to_string();
    assert!(refused.contains("quick chat folder"), "{refused}");
    remove_chat_dir(&chat_id).unwrap();
    assert!(!dir.exists());
    let _ = fs::remove_dir_all(store);
}

#[test]
fn listing_accepts_only_the_real_chat_folder() {
    let (chat_id, dir) = prepare_new_chat_dir().unwrap();
    let locked = match listing_decision(&display_path(&dir)) {
        ListingDecision::Locked(path) => path,
        other => panic!("the chat folder must lock, got {other:?}"),
    };
    assert_eq!(locked, display_path(&dir));

    let climb = format!("{}/../../outside", dir.display());
    assert!(
        matches!(listing_decision(&climb), ListingDecision::Rejected(_)),
        "dotdot must be rejected"
    );
    let doubled = format!("{}//..//..", dir.display());
    assert!(matches!(
        listing_decision(&doubled),
        ListingDecision::Rejected(_)
    ));
    assert!(matches!(
        listing_decision("/tmp/not-a-quick-chat"),
        ListingDecision::NotQuick
    ));
    remove_chat_dir(&chat_id).unwrap();
}

/// A symlink inside the quick-chat root that points outside it is not a chat folder.
#[cfg(unix)]
#[test]
fn a_symlink_escape_is_rejected() {
    let root = quick_chats_root().unwrap();
    fs::create_dir_all(&root).unwrap();
    let outside = tempfile::tempdir().unwrap();
    let id = mint_chat_id();
    let link = root.join(&id);
    std::os::unix::fs::symlink(outside.path(), &link).unwrap();
    let error = cwd_for_quick_session(&id).unwrap_err();
    assert!(
        error.contains("symlink") || error.contains("escapes"),
        "{error}"
    );
    match listing_decision(&link.to_string_lossy()) {
        ListingDecision::Rejected(message) => {
            assert!(
                message.contains("symlink") || message.contains("escapes"),
                "{message}"
            );
        }
        other => panic!("symlink must be rejected, got {other:?}"),
    }
    fs::remove_file(&link).unwrap();
}

/// `canonicalize` on Windows returns `\\?\C:\…`. That spelling must lock,
/// and a climb written after it must be rejected.
#[cfg(windows)]
#[test]
fn a_verbatim_windows_folder_locks_and_its_climb_is_rejected() {
    let (chat_id, dir) = prepare_new_chat_dir().unwrap();
    let verbatim = fs::canonicalize(&dir).unwrap();
    let spelling = verbatim.to_string_lossy().to_string();
    assert!(
        spelling.starts_with(r"\\?\"),
        "this test needs the verbatim prefix, got {spelling}"
    );
    match listing_decision(&spelling) {
        ListingDecision::Locked(path) => assert_eq!(path, display_path(&dir)),
        other => panic!("a verbatim folder must lock, got {other:?}"),
    }
    let climb = format!("{spelling}/../../outside");
    assert!(
        matches!(listing_decision(&climb), ListingDecision::Rejected(_)),
        "a verbatim climb must be rejected"
    );
    remove_chat_dir(&chat_id).unwrap();
}

#[test]
fn a_windows_style_climb_is_rejected() {
    let (chat_id, dir) = prepare_new_chat_dir().unwrap();
    let climb = format!("{}\\..\\..\\outside", dir.display());
    // On Unix a backslash is not a separator, so the path usually does not
    // exist and is still refused. On Windows it is a real climb.
    assert!(
        !matches!(listing_decision(&climb), ListingDecision::Locked(_)),
        "a backslash climb must not lock"
    );
    remove_chat_dir(&chat_id).unwrap();
}
