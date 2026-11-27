//! The GitHub area's rules that are not drawing: the assignee choices, the timeline's order,
//! the checks' grouping, the close and reopen choices, the merge box's state, the picker's
//! Done label and the comment preview. Each is the React component's function of the same
//! name.

use model::github::{
    AssigneeFilter, Check, CheckStatus, CloseReason, Comment, FilterPatch, ItemDetail, ItemKind, ItemState,
    ItemStateAction, KindFilter, ListFilter, MergeAction, MergeMethod, PullDetail, TimelineEvent, TimelineEventKind,
};

use crate::git_history::parse_iso;

/// `InvolvementChoice`: the assignee filter, or review requested (PRs only).
pub fn involvement(filter: &ListFilter) -> String {
    if filter.review_requested {
        return "review".into();
    }
    match &filter.assignee {
        AssigneeFilter::Anyone => "anyone".into(),
        AssigneeFilter::Me => "me".into(),
        AssigneeFilter::Unassigned => "unassigned".into(),
        AssigneeFilter::User { login } => format!("user:{login}"),
    }
}

/// `involvementPatch`.
pub fn involvement_patch(choice: &str) -> FilterPatch {
    let (assignee, review) = match choice {
        "review" => (AssigneeFilter::Anyone, true),
        "me" => (AssigneeFilter::Me, false),
        "unassigned" => (AssigneeFilter::Unassigned, false),
        other => match other.strip_prefix("user:") {
            Some(login) => (AssigneeFilter::User { login: login.to_owned() }, false),
            None => (AssigneeFilter::Anyone, false),
        },
    };
    FilterPatch { assignee: Some(assignee), review_requested: Some(review), ..FilterPatch::default() }
}

/// The assignee menu's choices: the fixed ones, review requested unless the list shows only
/// issues, then each assignable person but the viewer, plus a picked login GitHub no longer
/// lists.
pub fn involvement_options(filter: &ListFilter, people: &[String], viewer: Option<&str>) -> Vec<(String, String)> {
    let mut out = vec![
        ("anyone".to_owned(), "Anyone".to_owned()),
        ("me".to_owned(), "Assigned to me".to_owned()),
        ("unassigned".to_owned(), "Unassigned".to_owned()),
    ];
    if filter.kind != KindFilter::Issue {
        out.push(("review".into(), "Review requested".into()));
    }
    let mut logins: Vec<&str> = people.iter().map(String::as_str).filter(|p| Some(*p) != viewer).collect();
    if let AssigneeFilter::User { login } = &filter.assignee
        && !logins.contains(&login.as_str())
    {
        logins.push(login);
    }
    out.extend(logins.into_iter().map(|login| (format!("user:{login}"), format!("Assigned to {login}"))));
    out
}

/// The kind segment: picking Issues drops a review-requested filter.
pub fn kind_patch(filter: &ListFilter, kind: KindFilter) -> FilterPatch {
    let review = (kind == KindFilter::Issue && filter.review_requested).then_some(false);
    FilterPatch { kind: Some(kind), review_requested: review, ..FilterPatch::default() }
}

/// One entry of the conversation.
#[derive(Clone, Debug, PartialEq)]
pub enum Entry<'a> {
    Comment(&'a Comment),
    Event(&'a TimelineEvent),
}

/// GitHub records a ClosedEvent about a second after a merge; the merge already says it.
const MERGE_CLOSE_WINDOW_MS: f64 = 5_000.0;

/// `timelineEntries`: comments and events oldest first, comments before events at equal
/// times, without the close that follows a merge.
pub fn timeline_entries(detail: &ItemDetail) -> Vec<Entry<'_>> {
    let merges: Vec<f64> = detail
        .timeline
        .iter()
        .filter(|e| e.kind == TimelineEventKind::Merged)
        .filter_map(|e| parse_iso(&e.created_at))
        .collect();
    let follows_merge = |e: &TimelineEvent| {
        let Some(at) = parse_iso(&e.created_at) else { return false };
        merges.iter().any(|m| at >= *m && at - m <= MERGE_CLOSE_WINDOW_MS)
    };
    let mut entries: Vec<(f64, usize, Entry<'_>)> = detail
        .comments
        .iter()
        .map(|c| (c.created_at.as_str(), Entry::Comment(c)))
        .chain(
            detail
                .timeline
                .iter()
                .filter(|e| e.kind != TimelineEventKind::Closed || !follows_merge(e))
                .map(|e| (e.created_at.as_str(), Entry::Event(e))),
        )
        .enumerate()
        .map(|(i, (at, entry))| (parse_iso(at).unwrap_or(0.0), i, entry))
        .collect();
    entries.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    entries.into_iter().map(|(_, _, e)| e).collect()
}

/// `EVENT_LOOK` and `EventRow`'s verb: the icon and what the actor did.
pub fn event_words(event: &TimelineEvent) -> (&'static str, &'static str) {
    let own = event.assignee.as_deref() == Some(event.actor.as_str());
    match event.kind {
        TimelineEventKind::Labeled => ("Tag", "added"),
        TimelineEventKind::Unlabeled => ("Tag", "removed"),
        TimelineEventKind::Assigned if own => ("User", "self-assigned this"),
        TimelineEventKind::Assigned => ("User", "assigned"),
        TimelineEventKind::Unassigned if own => ("User", "removed their assignment"),
        TimelineEventKind::Unassigned => ("User", "unassigned"),
        TimelineEventKind::Closed => ("CircleCheck", "closed this"),
        TimelineEventKind::Merged => ("GitMerge", "merged this"),
        TimelineEventKind::Reopened => ("CircleDot", "reopened this"),
    }
}

fn status_order(status: CheckStatus) -> u8 {
    match status {
        CheckStatus::Failure => 0,
        CheckStatus::Pending => 1,
        CheckStatus::Cancelled => 2,
        CheckStatus::Success => 3,
        CheckStatus::Neutral => 4,
        CheckStatus::Skipped => 5,
    }
}

/// What still needs a look stays open: failed, running and cancelled.
pub fn needs_attention(status: CheckStatus) -> bool {
    matches!(status, CheckStatus::Failure | CheckStatus::Pending | CheckStatus::Cancelled)
}

/// The checks failures first, then running, then the rest (a stable sort).
pub fn sorted_checks(checks: &[Check]) -> Vec<&Check> {
    let mut rows: Vec<&Check> = checks.iter().collect();
    rows.sort_by_key(|c| status_order(c.status));
    rows
}

/// `counts`: each status and how many checks have it, in first-seen order.
pub fn counts(rows: &[&Check]) -> Vec<(CheckStatus, usize)> {
    let mut out: Vec<(CheckStatus, usize)> = Vec::new();
    for check in rows {
        match out.iter_mut().find(|(s, _)| *s == check.status) {
            Some((_, n)) => *n += 1,
            None => out.push((check.status, 1)),
        }
    }
    out
}

/// `foldedWords`: "passed and skipped".
pub fn folded_words(rows: &[&Check]) -> String {
    let words: Vec<String> = counts(rows).into_iter().map(|(s, _)| ui::github::check_look(s).0.to_lowercase()).collect();
    match words.as_slice() {
        [] => String::new(),
        [one] => one.clone(),
        [head @ .., last] => format!("{} and {last}", head.join(", ")),
    }
}

/// One close or reopen choice.
#[derive(Clone, Debug, PartialEq)]
pub struct StateChoice {
    pub key: &'static str,
    pub label: &'static str,
    pub detail: Option<&'static str>,
    pub icon: &'static str,
    pub action: ItemStateAction,
}

/// `stateActions`: what GitHub offers for the item's kind and state; none for a merged PR.
pub fn state_actions(kind: ItemKind, state: ItemState) -> Vec<StateChoice> {
    let close = |key, label, detail, icon, reason| StateChoice { key, label, detail, icon, action: ItemStateAction::Close { reason } };
    let reopen = |label| StateChoice { key: "reopen", label, detail: None, icon: "CircleDot", action: ItemStateAction::Reopen };
    match (kind, state) {
        (ItemKind::Issue, ItemState::Open) => vec![
            close("close", "Close issue", Some("as completed"), "CircleCheck", CloseReason::Completed),
            close("not_planned", "Close as not planned", None, "CircleMinus", CloseReason::NotPlanned),
        ],
        (ItemKind::Issue, _) => vec![reopen("Reopen issue")],
        (ItemKind::Pr, ItemState::Merged) => Vec::new(),
        (ItemKind::Pr, ItemState::Closed) => vec![reopen("Reopen pull request")],
        (ItemKind::Pr, _) => vec![close("close", "Close pull request", None, "CircleMinus", CloseReason::Completed)],
    }
}

/// `METHODS`: the merge method's menu label and its button verb.
pub fn method_words(method: MergeMethod) -> (&'static str, &'static str) {
    match method {
        MergeMethod::Merge => ("Merge commit", "Merge"),
        MergeMethod::Squash => ("Squash and merge", "Squash and merge"),
        MergeMethod::Rebase => ("Rebase and merge", "Rebase and merge"),
    }
}

/// What the merge box shows, per `GithubMergeControls`. `None` for a merged or closed PR.
#[derive(Clone, Debug, PartialEq)]
pub struct MergeBox {
    pub can_merge: bool,
    pub reason: Option<String>,
    pub auto_merge: Option<MergeAction>,
}

pub fn merge_box(state: ItemState, pr: &PullDetail, method: MergeMethod) -> Option<MergeBox> {
    if matches!(state, ItemState::Merged | ItemState::Closed) {
        return None;
    }
    let status = pr.merge_state_status.as_str();
    let can_merge = state == ItemState::Open && matches!(status, "CLEAN" | "HAS_HOOKS");
    let reason = (!can_merge).then(|| match status {
        "BLOCKED" => "Merging is blocked by required reviews or checks.".to_owned(),
        "BEHIND" => "The branch is behind its base branch.".to_owned(),
        "DIRTY" => "The branch has conflicts with its base branch.".to_owned(),
        "UNSTABLE" => "Some checks are not passing.".to_owned(),
        "DRAFT" => "Draft pull requests cannot be merged.".to_owned(),
        "UNKNOWN" => "GitHub is still checking whether this can merge.".to_owned(),
        other => format!("GitHub reports {other}."),
    });
    let auto_merge = if pr.auto_merge_enabled {
        Some(MergeAction::DisableAutoMerge)
    } else if state == ItemState::Open && !can_merge {
        Some(MergeAction::EnableAutoMerge { method })
    } else {
        None
    };
    Some(MergeBox { can_merge, reason, auto_merge })
}

/// `doneLabel`: "Done · +1 label", "Done · +2 −1 assignees", or "Done".
pub fn done_label(next: &[String], prev: &[String], noun: &str) -> String {
    let adds = next.iter().filter(|v| !prev.contains(v)).count();
    let removes = prev.iter().filter(|v| !next.contains(v)).count();
    if adds + removes == 0 {
        return "Done".into();
    }
    let mut parts = Vec::new();
    if adds > 0 {
        parts.push(format!("+{adds}"));
    }
    if removes > 0 {
        parts.push(format!("\u{2212}{removes}"));
    }
    let noun = if adds + removes == 1 { noun.to_owned() } else { format!("{noun}s") };
    format!("Done \u{b7} {} {noun}", parts.join(" "))
}

/// The label filter's apply label.
pub fn filter_apply_label(selected: usize) -> String {
    match selected {
        0 => "Show all".into(),
        1 => "Show results \u{b7} 1 label".into(),
        n => format!("Show results \u{b7} {n} labels"),
    }
}

/// Removes every `open ... close` span (case-insensitive open tag) from `text`.
fn strip_spans(text: &str, open: &str, close: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    loop {
        let lower = rest.to_ascii_lowercase();
        let Some(start) = lower.find(open) else { break };
        let Some(len) = lower[start + open.len()..].find(close) else { break };
        out.push_str(&rest[..start]);
        out.push(' ');
        rest = &rest[start + open.len() + len + close.len()..];
    }
    out.push_str(rest);
    out
}

/// `plainPreview`: one line of plain text for a folded bot comment.
pub fn plain_preview(markdown: &str) -> String {
    let text = strip_spans(markdown, "<!--", "-->");
    let text = strip_spans(&text, "<details", "</details>");
    let text = strip_spans(&text, "```", "```");
    let text = strip_spans(&text, "<", ">");
    let text = strip_spans(&text, "![", ")");
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '[' {
            let label: String = chars.by_ref().take_while(|c| *c != ']').collect();
            if chars.peek() == Some(&'(') {
                for c in chars.by_ref() {
                    if c == ')' {
                        break;
                    }
                }
            }
            out.push_str(&label);
        } else if !"*_`#>~".contains(ch) {
            out.push(ch);
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `relativeAgo`: "5m ago", "just now", or "on Mar 3, 2026" for older dates.
pub fn relative_ago(iso: &str, now_ms: f64) -> String {
    let relative = crate::git_history::relative_time(iso, now_ms);
    if relative.is_empty() {
        return "earlier".into();
    }
    if relative == "now" {
        return "just now".into();
    }
    let short = relative.len() > 1
        && relative[..relative.len() - 1].bytes().all(|b| b.is_ascii_digit())
        && relative.ends_with(['m', 'h', 'd', 'w']);
    if short { format!("{relative} ago") } else { format!("on {relative}") }
}
