//! What the slash and mention menus list (`slash-menu-model.ts` `buildSlashSections`,
//! `mention-menu-model.ts` `buildMentionSections`): sections of rows filtered by the query.
//! The slash menu lists the agent's skills and commands; the mention menu's root lists the two
//! categories, its Conversations panel the project's other chats, and its Explorer panel one
//! folder of the project (`/fs/ls`).

use serde::Deserialize;
use serde_json::Value;

/// A skill from the host's `/skills` route (`AgentSkillSummary`).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Skill {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub scope: String,
    pub path: String,
}

/// A chat the mention menu offers (`ConversationMatch`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatRef {
    pub title: String,
    pub session_id: String,
    pub agent: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Row {
    Skill(Skill),
    Command {
        name: String,
        description: Option<String>,
        hint: Option<String>,
    },
    Conversations,
    Explorer,
    Chat(ChatRef),
    /// A folder to drill into.
    Dir(Entry),
    File(Entry),
    /// The Explorer folder itself, as a folder pill.
    UseFolder(Entry),
}

/// A file or folder of the project (`MentionMatch`); a folder's `abs_path` ends in `/`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub rel_path: String,
    pub abs_path: String,
    pub dir: bool,
    pub ignored: bool,
}

impl Entry {
    /// The folder an Explorer panel lists, for a folder entry.
    pub fn folder(&self) -> String {
        self.abs_path.trim_end_matches('/').to_owned()
    }

    /// The file pill's text: a file's name, a folder's relative path with a `/`.
    pub fn display(&self) -> String {
        if self.dir { format!("{}/", self.rel_path) } else { self.name.clone() }
    }
}

/// One folder's `/fs/ls` read (`DirListing`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Listing {
    Loading,
    Ready(Vec<Entry>),
    Error,
}

pub const LISTING_LOADING: &str = "Loading…";
pub const LISTING_ERROR: &str = "Couldn't list this folder";
pub const LISTING_EMPTY: &str = "This folder is empty";
const MAX_LISTING_ENTRIES: usize = 200;

/// `relativeDir`: `dir` under `root` with a trailing `/`, or empty for the root.
pub fn relative_dir(root: &str, dir: &str) -> String {
    match dir.strip_prefix(root).and_then(|rest| rest.strip_prefix('/')) {
        Some(rest) if dir != root => format!("{rest}/"),
        _ => String::new(),
    }
}

/// `toListingMatch` over an `/fs/ls` answer. Paths come from `dir`, not the entry's `path`,
/// which the host may canonicalize.
pub fn listing_entries(value: &Value, dir: &str, root: &str) -> Option<Vec<Entry>> {
    let rel = relative_dir(root, dir);
    value
        .as_array()?
        .iter()
        .map(|e| {
            let name = e.get("name")?.as_str()?.to_owned();
            let is_dir = e.get("type").and_then(Value::as_str) == Some("directory");
            Some(Entry {
                rel_path: format!("{rel}{name}"),
                abs_path: if is_dir { format!("{dir}/{name}/") } else { format!("{dir}/{name}") },
                ignored: e.get("ignored").and_then(Value::as_bool).unwrap_or(false),
                dir: is_dir,
                name,
            })
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub label: String,
    pub description: Option<String>,
    pub row: Row,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    pub heading: Option<String>,
    /// A line under the heading: loading, empty, or a failed read.
    pub notice: Option<String>,
    pub items: Vec<Item>,
}

impl Section {
    fn of(heading: Option<String>, items: Vec<Item>) -> Section {
        Section { heading, notice: None, items }
    }
}

/// The mention menu's panel (`MentionPanel`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Panel {
    #[default]
    Root,
    Conversations,
    /// One folder of the project, by absolute path without a trailing `/`.
    Explorer(String),
}

fn matches(filter: &str, fields: &[&str]) -> bool {
    let f = super::js_trim(filter).to_lowercase();
    f.is_empty() || fields.iter().any(|x| x.to_lowercase().contains(&f))
}

/// `buildSlashSections` without the config and mode rows: skills, then the agent's commands
/// that do not name an installed skill.
pub fn slash_sections(commands: &[Value], skills: &[Skill], filter: &str) -> Vec<Section> {
    let installed: Vec<String> = skills
        .iter()
        .map(|s| super::js_trim(&s.name).to_lowercase())
        .filter(|n| !n.is_empty())
        .collect();
    let names_skill = |name: &str| {
        let bare = super::js_trim(name).to_lowercase();
        let promoted = bare.strip_prefix("skill:").map(|s| super::js_trim(s).to_owned());
        installed.contains(&bare) || promoted.is_some_and(|p| !p.is_empty() && installed.contains(&p))
    };
    let mut sections = Vec::new();
    let skill_items: Vec<Item> = skills
        .iter()
        .filter(|s| matches(filter, &[&s.name, &s.description]))
        .map(|s| Item {
            label: format!("/{}", s.name),
            description: (!s.description.is_empty()).then(|| s.description.clone()),
            row: Row::Skill(s.clone()),
        })
        .collect();
    if !skill_items.is_empty() {
        sections.push(Section::of(Some("Skills".into()), skill_items));
    }
    let command_items: Vec<Item> = commands
        .iter()
        .filter_map(|c| {
            let name = c.get("name")?.as_str()?.to_owned();
            let description = c.get("description").and_then(Value::as_str).map(str::to_owned);
            let hint = c
                .pointer("/input/hint")
                .and_then(Value::as_str)
                .map(|h| super::js_trim(h).to_owned())
                .filter(|h| !h.is_empty());
            (!names_skill(&name) && matches(filter, &[&name, description.as_deref().unwrap_or("")])).then(|| Item {
                label: format!("/{name}"),
                description: description.clone(),
                row: Row::Command {
                    name,
                    description,
                    hint,
                },
            })
        })
        .collect();
    if !command_items.is_empty() {
        sections.push(Section::of(Some("Commands".into()), command_items));
    }
    sections
}

/// The folder name `basename` shows for a project path.
fn folder_name(root: &str) -> &str {
    root.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(root)
}

const MAX_ROOT_CHATS: usize = 5;

fn chat_item(c: &ChatRef) -> Item {
    Item {
        label: c.title.clone(),
        description: None,
        row: Row::Chat(c.clone()),
    }
}

/// `filterConversations`.
fn filter_chats<'a>(chats: &'a [ChatRef], filter: &str) -> impl Iterator<Item = &'a ChatRef> {
    let q = super::js_trim(filter).to_lowercase();
    chats.iter().filter(move |c| {
        q.is_empty()
            || c.title.to_lowercase().contains(&q)
            || c.agent.as_ref().is_some_and(|a| a.to_lowercase().contains(&q))
    })
}

/// `buildMentionSections`. The Explorer lists `listing`; Rune has no file search yet, so a
/// query filters that folder's entries by name instead of searching the project.
pub fn mention_sections(panel: &Panel, root: &str, chats: &[ChatRef], filter: &str, listing: Option<&Listing>) -> Vec<Section> {
    let searching = !super::js_trim(filter).is_empty();
    match panel {
        Panel::Conversations => {
            let items: Vec<Item> = filter_chats(chats, filter).map(chat_item).collect();
            if items.is_empty() { vec![] } else { vec![Section::of(None, items)] }
        }
        Panel::Explorer(dir) => explorer_sections(dir, root, listing, filter),
        Panel::Root if searching => {
            let items: Vec<Item> = filter_chats(chats, filter)
                .take(MAX_ROOT_CHATS)
                .map(chat_item)
                .collect();
            if items.is_empty() { vec![] } else { vec![Section::of(Some("Conversations".into()), items)] }
        }
        Panel::Root => vec![Section::of(
            None,
            vec![
                Item {
                    label: "Conversations".into(),
                    description: Some(format!("{} in this project", chats.len())),
                    row: Row::Conversations,
                },
                Item {
                    label: "Explorer".into(),
                    description: Some(format!("Files and folders in {}/", folder_name(root))),
                    row: Row::Explorer,
                },
            ],
        )],
    }
}

fn entry_item(e: &Entry) -> Item {
    Item {
        label: e.name.clone(),
        description: None,
        row: if e.dir { Row::Dir(e.clone()) } else { Row::File(e.clone()) },
    }
}

/// `explorerSections`, without the root's Recent section (Rune keeps no recents yet).
fn explorer_sections(dir: &str, root: &str, listing: Option<&Listing>, filter: &str) -> Vec<Section> {
    let q = super::js_trim(filter).to_lowercase();
    let all: Vec<&Entry> = match listing {
        Some(Listing::Ready(entries)) => entries.iter().filter(|e| e.name.to_lowercase().contains(&q)).collect(),
        _ => Vec::new(),
    };
    let notice = match listing {
        None | Some(Listing::Loading) => Some(LISTING_LOADING.to_owned()),
        Some(Listing::Error) => Some(LISTING_ERROR.to_owned()),
        Some(Listing::Ready(_)) if all.is_empty() => Some(LISTING_EMPTY.to_owned()),
        Some(Listing::Ready(_)) => None,
    };
    let items: Vec<Item> = all.iter().take(MAX_LISTING_ENTRIES).map(|e| entry_item(e)).collect();
    let hidden = all.len() - items.len();
    let mut sections = Vec::new();
    let heading = if dir == root {
        Some(format!("{}/", folder_name(root)))
    } else {
        let rel = relative_dir(root, dir);
        let folder = Entry {
            name: folder_name(dir).to_owned(),
            rel_path: rel.trim_end_matches('/').to_owned(),
            abs_path: format!("{dir}/"),
            dir: true,
            ignored: false,
        };
        sections.push(Section::of(
            None,
            vec![Item { label: "Use this folder".into(), description: Some(rel), row: Row::UseFolder(folder) }],
        ));
        None
    };
    sections.push(Section { heading, notice, items });
    if hidden > 0 {
        sections.push(Section {
            heading: None,
            notice: Some(format!("{hidden} more not shown. Type a name to search.")),
            items: Vec::new(),
        });
    }
    sections
}

/// Every item in order, for the highlight.
pub fn flat(sections: &[Section]) -> Vec<&Item> {
    sections.iter().flat_map(|s| s.items.iter()).collect()
}
