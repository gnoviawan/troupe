//! What the slash and mention menus list (`slash-menu-model.ts` `buildSlashSections`,
//! `mention-menu-model.ts` `buildMentionSections`): sections of rows filtered by the query.
//! The slash menu lists the agent's skills and commands; the mention menu's root lists the two
//! categories, and its Conversations panel the project's other chats.

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
    pub items: Vec<Item>,
}

/// The mention menu's panel (`MentionPanel`; the explorer is a later unit).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Panel {
    #[default]
    Root,
    Conversations,
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
        sections.push(Section {
            heading: Some("Skills".into()),
            items: skill_items,
        });
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
        sections.push(Section {
            heading: Some("Commands".into()),
            items: command_items,
        });
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

/// `buildMentionSections` for the root and Conversations panels (file search is a later unit).
pub fn mention_sections(panel: Panel, root: &str, chats: &[ChatRef], filter: &str) -> Vec<Section> {
    let searching = !super::js_trim(filter).is_empty();
    match panel {
        Panel::Conversations => {
            let items: Vec<Item> = filter_chats(chats, filter).map(chat_item).collect();
            if items.is_empty() {
                vec![]
            } else {
                vec![Section { heading: None, items }]
            }
        }
        Panel::Root if searching => {
            let items: Vec<Item> = filter_chats(chats, filter)
                .take(MAX_ROOT_CHATS)
                .map(chat_item)
                .collect();
            if items.is_empty() {
                vec![]
            } else {
                vec![Section {
                    heading: Some("Conversations".into()),
                    items,
                }]
            }
        }
        Panel::Root => vec![Section {
            heading: None,
            items: vec![
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
        }],
    }
}

/// Every item in order, for the highlight.
pub fn flat(sections: &[Section]) -> Vec<&Item> {
    sections.iter().flat_map(|s| s.items.iter()).collect()
}
