//! The command palette's own state and its matching: the recent commands (this device's
//! `settings/recent-commands`), the pinned ones (the shared `settings/pinned-commands`), and
//! cmdk's `commandScore`, which decides which rows a query keeps and in what order. Port of
//! `recent-commands-store.ts`, `pinned-commands-store.ts` and cmdk 1.1's `command-score`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// This device's recent commands (a device key).
pub const RECENT_KEY: &str = "settings/recent-commands";
pub const PINNED_KEY: &str = "settings/pinned-commands";
const MAX_RECENT: usize = 5;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Commands {
    pub recent_command_ids: Vec<String>,
    pub pinned_command_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Event {
    AddRecentCommand(String),
    SetRecentCommands(Vec<String>),
    TogglePinned(String),
    SetPinned(Vec<String>),
}

impl Commands {
    pub fn step(&mut self, event: Event) {
        match event {
            Event::AddRecentCommand(id) => {
                self.recent_command_ids.retain(|r| *r != id);
                self.recent_command_ids.insert(0, id);
                self.recent_command_ids.truncate(MAX_RECENT);
            }
            Event::SetRecentCommands(ids) => self.recent_command_ids = ids,
            Event::TogglePinned(id) => match self.pinned_command_ids.iter().position(|p| *p == id) {
                Some(at) => {
                    self.pinned_command_ids.remove(at);
                }
                None => self.pinned_command_ids.push(id),
            },
            Event::SetPinned(ids) => self.pinned_command_ids = ids,
        }
    }
}

const CONTINUE_MATCH: f64 = 1.0;
const SPACE_WORD_JUMP: f64 = 0.9;
const NON_SPACE_WORD_JUMP: f64 = 0.8;
const CHARACTER_JUMP: f64 = 0.17;
const TRANSPOSITION: f64 = 0.1;
const PENALTY_SKIPPED: f64 = 0.999;
const PENALTY_CASE_MISMATCH: f64 = 0.9999;
const PENALTY_NOT_COMPLETE: f64 = 0.99;

/// `IS_GAP_REGEXP`: `[\\/_+.#"@[({&]`.
fn is_gap(c: char) -> bool {
    matches!(c, '\\' | '/' | '_' | '+' | '.' | '#' | '"' | '@' | '[' | '(' | '{' | '&')
}

/// `IS_SPACE_REGEXP`: `[\s-]`.
fn is_space(c: char) -> bool {
    c.is_whitespace() || c == '-'
}

/// `formatInput`: lower case, every space or dash a space.
fn format_input(text: &str) -> Vec<char> {
    text.to_lowercase().chars().map(|c| if is_space(c) { ' ' } else { c }).collect()
}

struct Scorer<'a> {
    string: &'a [char],
    abbreviation: &'a [char],
    lower_string: &'a [char],
    lower_abbreviation: &'a [char],
    memo: HashMap<(usize, usize), f64>,
}

/// `charAt`: an index past either end reads as no character.
fn at(chars: &[char], index: isize) -> Option<char> {
    usize::try_from(index).ok().and_then(|i| chars.get(i).copied())
}

fn find(chars: &[char], c: char, from: usize) -> Option<usize> {
    chars.iter().skip(from).position(|x| *x == c).map(|i| i + from)
}

impl Scorer<'_> {
    fn score(&mut self, string_index: usize, abbreviation_index: usize) -> f64 {
        if abbreviation_index == self.abbreviation.len() {
            return if string_index == self.string.len() { CONTINUE_MATCH } else { PENALTY_NOT_COMPLETE };
        }
        if let Some(known) = self.memo.get(&(string_index, abbreviation_index)) {
            return *known;
        }
        let wanted = self.lower_abbreviation[abbreviation_index];
        let mut index = find(self.lower_string, wanted, string_index);
        let mut high = 0.0;
        while let Some(i) = index {
            let mut score = self.score(i + 1, abbreviation_index + 1);
            if score > high {
                let before = at(self.string, i as isize - 1).unwrap_or_default();
                let skipped = || self.string.get(string_index..i.saturating_sub(1)).unwrap_or_default();
                if i == string_index {
                    score *= CONTINUE_MATCH;
                } else if is_gap(before) {
                    score *= NON_SPACE_WORD_JUMP;
                    let breaks = skipped().iter().filter(|c| is_gap(**c)).count();
                    if breaks > 0 && string_index > 0 {
                        score *= PENALTY_SKIPPED.powi(breaks as i32);
                    }
                } else if is_space(before) {
                    score *= SPACE_WORD_JUMP;
                    let breaks = skipped().iter().filter(|c| is_space(**c)).count();
                    if breaks > 0 && string_index > 0 {
                        score *= PENALTY_SKIPPED.powi(breaks as i32);
                    }
                } else {
                    score *= CHARACTER_JUMP;
                    if string_index > 0 {
                        score *= PENALTY_SKIPPED.powi((i - string_index) as i32);
                    }
                }
                if self.string[i] != self.abbreviation[abbreviation_index] {
                    score *= PENALTY_CASE_MISMATCH;
                }
            }
            let next = at(self.lower_abbreviation, abbreviation_index as isize + 1);
            let previous = at(self.lower_string, i as isize - 1);
            if (score < TRANSPOSITION && previous == next) || (next == Some(wanted) && previous != Some(wanted)) {
                let transposed = self.score(i + 1, abbreviation_index + 2);
                if transposed * TRANSPOSITION > score {
                    score = transposed * TRANSPOSITION;
                }
            }
            if score > high {
                high = score;
            }
            index = find(self.lower_string, wanted, i + 1);
        }
        self.memo.insert((string_index, abbreviation_index), high);
        high
    }
}

/// cmdk's `commandScore(value, search, keywords)`: 0 drops the row, a higher score ranks it
/// higher in its group.
#[must_use]
pub fn command_score(value: &str, search: &str, keywords: &[String]) -> f64 {
    let value = if keywords.is_empty() { value.to_owned() } else { format!("{value} {}", keywords.join(" ")) };
    let string: Vec<char> = value.chars().collect();
    let abbreviation: Vec<char> = search.chars().collect();
    let lower_string = format_input(&value);
    let lower_abbreviation = format_input(search);
    if lower_string.len() != string.len() || lower_abbreviation.len() != abbreviation.len() {
        // A character whose lower case is longer: score the lowered text against itself.
        return Scorer {
            string: &lower_string,
            abbreviation: &lower_abbreviation,
            lower_string: &lower_string,
            lower_abbreviation: &lower_abbreviation,
            memo: HashMap::new(),
        }
        .score(0, 0);
    }
    Scorer { string: &string, abbreviation: &abbreviation, lower_string: &lower_string, lower_abbreviation: &lower_abbreviation, memo: HashMap::new() }
        .score(0, 0)
}

/// `CommandCategory`, in `COMMAND_CATEGORY_ORDER`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Projects,
    Workspace,
    Navigation,
    Tools,
}

impl Category {
    pub const ORDER: [Category; 4] = [Category::Projects, Category::Workspace, Category::Navigation, Category::Tools];

    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Category::Projects => "projects",
            Category::Workspace => "workspace",
            Category::Navigation => "navigation",
            Category::Tools => "tools",
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Category::Projects => "Projects",
            Category::Workspace => "Workspace",
            Category::Navigation => "Navigation",
            Category::Tools => "Tools",
        }
    }
}

/// One palette command (`CommandDef` without its icon and action).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub id: String,
    pub category: Category,
    pub label: String,
    pub description: Option<String>,
    pub keywords: Vec<String>,
    pub shortcut: Option<String>,
}

impl Row {
    /// `getSearchableValue`, trimmed as cmdk trims an item's value.
    #[must_use]
    pub fn searchable(&self) -> String {
        let mut parts: Vec<&str> = vec![&self.label];
        parts.extend(self.description.as_deref());
        parts.push(self.category.id());
        parts.push(self.category.label());
        parts.extend(self.keywords.iter().map(String::as_str));
        parts.into_iter().filter(|p| !p.is_empty()).collect::<Vec<_>>().join(" ").trim().to_owned()
    }
}

/// A group heading of the palette's list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Heading {
    Pinned,
    Recent,
    Category(Category),
}

impl Heading {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Heading::Pinned => "Pinned",
            Heading::Recent => "Recent",
            Heading::Category(c) => c.label(),
        }
    }
}

/// The groups the palette shows for `query`, each with its rows in order, as cmdk 1.1 leaves
/// them: with no query, Pinned and Recent above every category, unfiltered; with one, only the
/// categories, each keeping the rows that score above 0, best first (a stable sort). cmdk does
/// not reorder the groups.
#[must_use]
pub fn groups<'a>(rows: &'a [Row], query: &str, lists: &Commands) -> Vec<(Heading, Vec<&'a Row>)> {
    let mut out = Vec::new();
    if query.is_empty() {
        let find = |id: &String| rows.iter().find(|r| r.id == *id);
        out.push((Heading::Pinned, lists.pinned_command_ids.iter().filter_map(find).collect()));
        let mut recent: Vec<&Row> = rows.iter().filter(|r| lists.recent_command_ids.contains(&r.id)).collect();
        recent.sort_by_key(|r| lists.recent_command_ids.iter().position(|id| *id == r.id));
        out.push((Heading::Recent, recent));
        for category in Category::ORDER {
            out.push((Heading::Category(category), rows.iter().filter(|r| r.category == category).collect()));
        }
    } else {
        for category in Category::ORDER {
            let mut scored: Vec<(f64, &Row)> = rows
                .iter()
                .filter(|r| r.category == category)
                .map(|r| (command_score(&r.searchable(), query, &[]), r))
                .filter(|(score, _)| *score > 0.0)
                .collect();
            scored.sort_by(|a, b| b.0.total_cmp(&a.0));
            out.push((Heading::Category(category), scored.into_iter().map(|(_, r)| r).collect()));
        }
    }
    out.retain(|(_, rows)| !rows.is_empty());
    out
}

#[cfg(test)]
#[path = "palette_tests.rs"]
mod tests;
