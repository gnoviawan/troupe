//! The store's pure reads: the Git badge count, the status-bar buckets, and the deepest
//! tracked working copy holding a path.

use std::collections::HashSet;

use super::{State, StatusDetail};
use crate::workspace::normalize_path;

/// `selectChangedFileCount`: a file both staged and unstaged counts once.
#[must_use]
pub fn changed_file_count(state: &State, cwd: &str) -> usize {
    let paths: HashSet<&str> = state.statuses(cwd).iter().map(|s| s.path.as_str()).collect();
    paths.len()
}

/// `GitStatusCounts`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub modified: usize,
    pub staged: usize,
    pub untracked: usize,
    pub has_changes: bool,
}

/// `countGitStatuses`: a file counts once per bucket.
#[must_use]
pub fn count_statuses(statuses: &[StatusDetail]) -> Counts {
    let (mut modified, mut staged, mut untracked) = (HashSet::new(), HashSet::new(), HashSet::new());
    for entry in statuses {
        if entry.staged {
            staged.insert(entry.path.as_str());
        }
        if entry.status == "untracked" {
            untracked.insert(entry.path.as_str());
        } else if !entry.staged {
            modified.insert(entry.path.as_str());
        }
    }
    Counts {
        modified: modified.len(),
        staged: staged.len(),
        untracked: untracked.len(),
        has_changes: !(modified.is_empty() && staged.is_empty() && untracked.is_empty()),
    }
}

/// `deepestTrackedKey`: the longest key that holds `path` on a segment boundary.
#[must_use]
pub fn deepest_tracked_key<'a>(path: &str, keys: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    let target = normalize_path(path);
    if target.is_empty() {
        return None;
    }
    let mut best: Option<&str> = None;
    for key in keys.into_iter().filter(|k| !k.is_empty()) {
        let contains = target == key || {
            let prefix = if key.ends_with('/') { key.to_owned() } else { format!("{key}/") };
            target.starts_with(&prefix)
        };
        if contains && best.is_none_or(|b| key.len() > b.len()) {
            best = Some(key);
        }
    }
    best
}
