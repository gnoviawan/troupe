//! The canvas draft (`canvas/draft/draft.ts`): a host head plus the steps applied on top of it
//! since. Nothing reaches the host revision until Save sends every pending op as one
//! `apply_ops` batch and the positions through `set_layout`.
//!
//! Undo and redo walk gestures. Undoing the gesture that is the last pending step drops that
//! step, so undoing back to the head leaves a clean draft; undoing one that is already saved
//! applies its inverse as a new step. The web client compares steps and the pending list by
//! object identity; here each step carries an id and the pending list a revision.

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use super::ops::{DraftDoc, OpRejection, apply_ops, inverse_ops};
use super::types::{Automation, Issue, Layout, NodePosition, Op, TRIGGER};

/// Positions to set; `None` removes the stored position.
pub type LayoutPatch = std::collections::BTreeMap<String, Option<NodePosition>>;

static NEXT: AtomicU64 = AtomicU64::new(1);

fn mint() -> u64 {
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// One direction of a gesture: ops for the revision, positions for `set_layout`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Step {
    #[serde(skip, default = "mint")]
    pub id: u64,
    pub ops: Vec<Op>,
    pub layout: LayoutPatch,
}

impl Step {
    pub fn new(ops: Vec<Op>, layout: LayoutPatch) -> Step {
        Step {
            id: mint(),
            ops,
            layout,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Gesture {
    pub forward: Step,
    pub backward: Step,
}

/// A Save met a newer head; the draft was replayed on it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Conflict {
    pub by: Option<String>,
    pub dropped: Vec<Op>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Draft {
    /// The host head this draft is based on.
    pub base: Automation,
    #[serde(rename = "baseIssues")]
    pub base_issues: Vec<Issue>,
    /// Applied since `base`, in order; empty means nothing to save.
    pub pending: Vec<Step>,
    /// `base` with every pending step applied.
    pub doc: DraftDoc,
    pub layout: Layout,
    /// From the latest dry run of `pending`, or `base_issues` when clean.
    pub issues: Vec<Issue>,
    pub undo: Vec<Gesture>,
    pub redo: Vec<Gesture>,
    pub conflict: Option<Conflict>,
    /// Moves whenever `pending` is replaced: a dry run answers for one revision.
    #[serde(skip, default = "mint")]
    pub pending_rev: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Gesture(Gesture),
    Undo,
    Redo,
    /// A dry run of the pending list at `pending_rev`; ignored once the draft moved on.
    Previewed {
        pending_rev: u64,
        issues: Vec<Issue>,
    },
    Saved {
        automation: Automation,
        issues: Vec<Issue>,
    },
    /// Positions the host refused to store: pending again, outside the history.
    Requeue(LayoutPatch),
    /// The host head changed under the draft (`automation_changed`).
    Head {
        automation: Automation,
        issues: Vec<Issue>,
    },
    Rebased {
        automation: Automation,
        issues: Vec<Issue>,
        by: Option<String>,
    },
    Discard,
    DismissConflict,
}

pub fn doc_of(automation: &Automation) -> DraftDoc {
    DraftDoc {
        name: automation.name.clone(),
        trigger: automation.trigger.clone(),
        graph: automation.graph.clone(),
        isolation: automation.isolation,
    }
}

pub fn open(automation: &Automation, issues: &[Issue]) -> Draft {
    Draft {
        base: automation.clone(),
        base_issues: issues.to_vec(),
        pending: Vec::new(),
        doc: doc_of(automation),
        layout: automation.layout.clone(),
        issues: issues.to_vec(),
        undo: Vec::new(),
        redo: Vec::new(),
        conflict: None,
        pending_rev: mint(),
    }
}

impl Draft {
    pub fn pending_ops(&self) -> Vec<Op> {
        self.pending
            .iter()
            .flat_map(|s| s.ops.iter().cloned())
            .collect()
    }

    pub fn dirty(&self) -> bool {
        !self.pending.is_empty()
    }
}

pub fn patch_layout(layout: &Layout, patch: &LayoutPatch) -> Layout {
    let mut next = layout.clone();
    for (key, at) in patch {
        match at {
            Some(at) => {
                next.insert(key.clone(), *at);
            }
            None => {
                next.remove(key);
            }
        }
    }
    next
}

/// The positions `next` sets or removes relative to `base`.
pub fn changed_positions(base: &Layout, next: &Layout) -> LayoutPatch {
    let keys: BTreeSet<&String> = base.keys().chain(next.keys()).collect();
    let mut patch = LayoutPatch::new();
    for key in keys {
        match (base.get(key), next.get(key)) {
            (Some(_), None) => {
                patch.insert(key.clone(), None);
            }
            (was, Some(now)) if was != Some(now) => {
                patch.insert(key.clone(), Some(*now));
            }
            _ => {}
        }
    }
    patch
}

fn apply_step(doc: &DraftDoc, layout: &Layout, step: &Step) -> Option<(DraftDoc, Layout)> {
    let doc = if step.ops.is_empty() {
        doc.clone()
    } else {
        apply_ops(doc, &step.ops).ok()?
    };
    Some((doc, patch_layout(layout, &step.layout)))
}

/// `base` with `steps` applied; none when one no longer applies.
fn replay(base: &Automation, steps: &[Step]) -> Option<(DraftDoc, Layout)> {
    let mut state = (doc_of(base), base.layout.clone());
    for step in steps {
        state = apply_step(&state.0, &state.1, step)?;
    }
    Some(state)
}

/// The gesture that applies `ops` and `layout` to the draft, with the ops and positions that
/// undo it, or why the host would refuse the ops.
pub fn plan_gesture(
    draft: &Draft,
    ops: Vec<Op>,
    layout: LayoutPatch,
) -> Result<Gesture, OpRejection> {
    if !ops.is_empty() {
        apply_ops(&draft.doc, &ops)?;
    }
    let backward_ops = inverse_ops(&draft.doc, &ops).unwrap_or_default();
    let backward_layout: LayoutPatch = layout
        .keys()
        .map(|k| (k.clone(), draft.layout.get(k).copied()))
        .collect();
    Ok(Gesture {
        forward: Step::new(ops, layout),
        backward: Step::new(backward_ops, backward_layout),
    })
}

/// Positions never change issues, and no dry run runs while no ops are pending, so with no
/// pending ops the head's issues are the current ones.
fn settle_issues(mut draft: Draft) -> Draft {
    if !draft.pending.iter().any(|s| !s.ops.is_empty()) {
        draft.issues = draft.base_issues.clone();
    }
    draft
}

/// Steps `step` onto the draft: drops it when it undoes the last pending step, else appends it.
fn walk(draft: &Draft, step: &Step, undoes: &Step) -> Option<Draft> {
    let mut next = draft.clone();
    if draft
        .pending
        .last()
        .is_some_and(|last| last.id == undoes.id)
    {
        next.pending.pop();
        (next.doc, next.layout) = replay(&draft.base, &next.pending)?;
    } else {
        (next.doc, next.layout) = apply_step(&draft.doc, &draft.layout, step)?;
        next.pending.push(step.clone());
    }
    Some(settle_issues(next))
}

fn node_exists(doc: &DraftDoc, id: &str) -> bool {
    id == TRIGGER || doc.graph.node(id).is_some()
}

/// `steps` replayed op by op on `head`. An op is dropped when the host would refuse it there,
/// or when it connects a step that is gone.
fn rebase(head: &Automation, steps: &[Step]) -> (Vec<Step>, Vec<Op>) {
    let mut doc = doc_of(head);
    let mut dropped = Vec::new();
    let mut kept = Vec::new();
    for step in steps {
        let mut ops = Vec::new();
        for op in &step.ops {
            let orphan = matches!(op, Op::Connect { edge } if !node_exists(&doc, &edge.from) || !node_exists(&doc, &edge.to));
            match apply_ops(&doc, std::slice::from_ref(op)) {
                Ok(next) if !orphan => {
                    ops.push(op.clone());
                    doc = next;
                }
                _ => dropped.push(op.clone()),
            }
        }
        let layout: LayoutPatch = step
            .layout
            .iter()
            .filter(|(k, _)| node_exists(&doc, k))
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        if !ops.is_empty() || !layout.is_empty() {
            kept.push(Step::new(ops, layout));
        }
    }
    (kept, dropped)
}

pub fn reduce(draft: &Draft, action: Action) -> Draft {
    let mut next = match action {
        Action::Gesture(gesture) => {
            let Some((doc, layout)) = apply_step(&draft.doc, &draft.layout, &gesture.forward)
            else {
                return draft.clone();
            };
            let mut next = draft.clone();
            next.doc = doc;
            next.layout = layout;
            next.pending.push(gesture.forward.clone());
            next.undo.push(gesture);
            next.redo.clear();
            next
        }
        Action::Undo => {
            let Some(gesture) = draft.undo.last() else {
                return draft.clone();
            };
            let Some(mut next) = walk(draft, &gesture.backward, &gesture.forward) else {
                return draft.clone();
            };
            next.undo.pop();
            next.redo.push(gesture.clone());
            next
        }
        Action::Redo => {
            let Some(gesture) = draft.redo.last() else {
                return draft.clone();
            };
            let Some(mut next) = walk(draft, &gesture.forward, &gesture.backward) else {
                return draft.clone();
            };
            next.redo.pop();
            next.undo.push(gesture.clone());
            next
        }
        Action::Previewed {
            pending_rev,
            issues,
        } => {
            let mut next = draft.clone();
            if pending_rev == draft.pending_rev && !draft.pending.is_empty() {
                next.issues = issues;
            }
            return next;
        }
        Action::Saved { automation, issues } => {
            let mut opened = open(&automation, &issues);
            // History stays valid only on our own revision: one bump for the ops, none for positions.
            let own = draft.base.revision + u64::from(!draft.pending_ops().is_empty());
            if automation.revision == own {
                opened.undo = draft.undo.clone();
                opened.redo = draft.redo.clone();
            }
            opened
        }
        Action::Requeue(layout) => {
            let mut next = draft.clone();
            next.layout = patch_layout(&draft.layout, &layout);
            next.pending.push(Step::new(Vec::new(), layout));
            next
        }
        Action::Head { automation, issues } => {
            // A dirty draft keeps its base; Save then meets the new head and rebases.
            if draft.dirty() || automation.revision < draft.base.revision {
                return draft.clone();
            }
            let mut opened = open(&automation, &issues);
            // Another author's revision makes the saved half of the history stale.
            if automation.revision == draft.base.revision {
                opened.undo = draft.undo.clone();
                opened.redo = draft.redo.clone();
            }
            opened
        }
        Action::Rebased {
            automation,
            issues,
            by,
        } => {
            if !draft.dirty() || automation.revision <= draft.base.revision {
                return draft.clone();
            }
            let (steps, dropped) = rebase(&automation, &draft.pending);
            let mut rebased = open(&automation, &issues);
            for step in steps {
                let Ok(gesture) = plan_gesture(&rebased, step.ops, step.layout) else {
                    continue;
                };
                rebased = reduce(&rebased, Action::Gesture(gesture));
            }
            rebased.conflict = Some(Conflict { by, dropped });
            rebased
        }
        Action::Discard => open(&draft.base, &draft.base_issues),
        Action::DismissConflict => {
            let mut next = draft.clone();
            next.conflict = None;
            return next;
        }
    };
    next.pending_rev = mint();
    next
}

#[cfg(test)]
#[path = "draft_tests.rs"]
mod tests;
