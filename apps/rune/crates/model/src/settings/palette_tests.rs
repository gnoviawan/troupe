use super::{Category, Commands, Heading, Row, groups};

fn row(id: &str, category: Category, label: &str, description: &str, keywords: &[&str]) -> Row {
    Row {
        id: id.to_owned(),
        category,
        label: label.to_owned(),
        description: (!description.is_empty()).then(|| description.to_owned()),
        keywords: keywords.iter().map(|k| (*k).to_owned()).collect(),
        shortcut: None,
    }
}

fn rows() -> Vec<Row> {
    vec![
        row("new-terminal", Category::Workspace, "New Terminal", "Open a new shell in the active pane", &["shell", "console", "pty", "workspace"]),
        row("open-canvas", Category::Workspace, "Open Canvas", "Open the project's .figcraft design document as a canvas tab", &["canvas", "figcraft", "design", "drawing", "vector"]),
        row("open-project-settings", Category::Navigation, "Project Settings", "Configure the active project workspace", &["settings", "project", "configure", "config"]),
        row("open-app-preferences", Category::Navigation, "App Preferences", "Open global application preferences", &["preferences", "prefs", "settings", "app", "global"]),
        row("project-alpha", Category::Projects, "alpha", "/tmp/w/alpha", &["project", "switch", "alpha", "/tmp/w/alpha"]),
        row("change-color-theme", Category::Tools, "Change Color Theme", "Preview and apply a UI color theme", &["theme", "color"]),
    ]
}

fn shown(groups: &[(Heading, Vec<&Row>)]) -> Vec<(&'static str, Vec<String>)> {
    groups.iter().map(|(h, rows)| (h.label(), rows.iter().map(|r| r.id.clone()).collect())).collect()
}

#[test]
fn searchable_joins_label_description_category_and_keywords() {
    assert_eq!(
        rows()[3].searchable(),
        "App Preferences Open global application preferences navigation Navigation preferences prefs settings app global"
    );
}

#[test]
fn no_query_lists_pinned_then_recent_then_every_category() {
    let rows = rows();
    let lists = Commands {
        recent_command_ids: vec!["change-color-theme".into(), "gone".into(), "new-terminal".into()],
        pinned_command_ids: vec!["open-app-preferences".into(), "gone".into()],
    };
    let ids = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    assert_eq!(
        shown(&groups(&rows, "", &lists)),
        vec![
            ("Pinned", ids(&["open-app-preferences"])),
            ("Recent", ids(&["change-color-theme", "new-terminal"])),
            ("Projects", ids(&["project-alpha"])),
            ("Workspace", ids(&["new-terminal", "open-canvas"])),
            ("Navigation", ids(&["open-project-settings", "open-app-preferences"])),
            ("Tools", ids(&["change-color-theme"])),
        ]
    );
}

#[test]
fn a_query_drops_pinned_and_recent_and_ranks_inside_each_group() {
    let rows = rows();
    let lists = Commands { recent_command_ids: vec!["new-terminal".into()], pinned_command_ids: vec!["new-terminal".into()] };
    let ids = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    assert_eq!(
        shown(&groups(&rows, "App Pref", &lists)),
        vec![
            ("Workspace", ids(&["open-canvas"])),
            ("Navigation", ids(&["open-app-preferences", "open-project-settings"])),
        ]
    );
    assert_eq!(shown(&groups(&rows, "zzz", &lists)), Vec::<(&str, Vec<String>)>::new());
}
