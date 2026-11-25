//! The command palette's own state and its matching: the recent commands (this device's
//! `settings/recent-commands`), the pinned ones (the shared `settings/pinned-commands`), and
//! cmdk's `commandScore`, which decides which rows a query keeps and in what order. Port of
//! `recent-commands-store.ts`, `pinned-commands-store.ts` and cmdk 1.1's `command-score`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

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
