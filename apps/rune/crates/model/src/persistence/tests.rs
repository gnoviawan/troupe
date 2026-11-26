use serde_json::json;

use super::*;
use crate::workspace::Leaf;

struct Seq(u32);

impl Ids for Seq {
    fn next_id(&mut self) -> String {
        self.0 += 1;
        format!("id-{}", self.0)
    }
}

fn file(path: &str, dirty: bool) -> OpenFile {
    OpenFile {
        file_path: path.to_owned(),
        cursor_position: Cursor { line: 1, col: 1 },
        scroll_top: 0.0,
        view_mode: ViewMode::Code,
        is_dirty: dirty,
        draft_content: dirty.then(|| "draft".to_owned()),
        last_modified: 1.0,
    }
}

#[test]
fn a_record_round_trips_through_the_store_envelope() {
    let raw = json!({
        "_version": 1,
        "data": {
            "openFiles": [file("/p/a.md", true), file("/p/b.md", false)],
            "activeFilePath": "/p/b.md",
            "expandedDirs": ["/p/src", "/elsewhere"],
            "activeTabId": "edit-/p/b.md",
            "paneLayout": {
                "type": "split", "id": "s", "direction": "horizontal", "sizes": [30, 70],
                "children": [
                    { "type": "leaf", "id": "l", "tabs": [{ "type": "editor", "filePath": "/p/a.md" }], "activeTabId": "edit-/p/a.md" },
                    { "type": "leaf", "id": "r", "tabs": [{ "type": "editor", "filePath": "/p/b.md" }, { "type": "terminal", "terminalId": "gone" }], "activeTabId": "edit-/p/b.md" }
                ]
            },
            "activePaneId": "r"
        }
    });
    let mut fx = Vec::new();
    let hydrated = hydrate(Some(raw), Some("/p"), &[], None, &mut Seq(0), &mut fx);
    assert!(fx.is_empty(), "{fx:?}");
    assert_eq!(hydrated.files.expanded_dirs, vec!["/p/src".to_owned()]);
    let written = serde_json::to_value(record(&hydrated.tree, &hydrated.files)).unwrap();
    assert_eq!(
        written["paneLayout"]["children"][1]["tabs"],
        json!([{ "type": "editor", "filePath": "/p/b.md" }]),
        "a terminal with no live match is dropped"
    );
    assert_eq!(written["activePaneId"], json!("r"));
    assert_eq!(written["activeTabId"], json!("edit-/p/b.md"));
    assert_eq!(written["openFiles"][0]["draftContent"], json!("draft"));
    assert_eq!(written["openFiles"][1].get("draftContent"), None);
}

#[test]
fn closing_the_active_file_hands_focus_to_the_last_open_one() {
    let mut files = Files {
        open: vec![file("/a", false), file("/b", false), file("/c", false)],
        active: Some("/b".into()),
        expanded_dirs: Vec::new(),
    };
    files.close("/b");
    assert_eq!(files.active.as_deref(), Some("/c"));
    files.close("/a");
    assert_eq!(files.active.as_deref(), Some("/c"));
    assert_eq!(files.open.len(), 1);
}

#[test]
fn the_active_file_follows_the_active_pane() {
    let workspace = ProjectWorkspace {
        root: PaneNode::Leaf(Leaf {
            id: "l".into(),
            tabs: vec![Tab::Git { id: "git-/p".into(), cwd: "/p".into() }],
            active_tab_id: Some("git-/p".into()),
        }),
        active_pane_id: "l".into(),
        fullscreen_pane_id: None,
        agent_launcher_pane_id: None,
        agent_browser_pane_id: None,
    };
    let mut files = Files { active: Some("/a".into()), ..Files::default() };
    files.follow(&workspace);
    assert_eq!(files.active, None);
}

#[test]
fn no_record_is_a_fresh_single_leaf() {
    let hydrated = hydrate(None, None, &[], None, &mut Seq(0), &mut Vec::new());
    assert_eq!(hydrated.tree.active_pane_id, "id-1");
    let written = serde_json::to_value(record(&hydrated.tree, &hydrated.files)).unwrap();
    assert_eq!(
        written,
        json!({
            "openFiles": [], "activeFilePath": null, "expandedDirs": [], "activeTabId": null,
            "paneLayout": { "type": "leaf", "id": "id-1", "tabs": [], "activeTabId": null },
            "activePaneId": "id-1"
        })
    );
}
