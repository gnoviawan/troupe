//! The prompts that wait on the user: `PermissionPrompt` inside the composer card, and
//! `AskUserQuestion` (the `QuestionStepper` chrome) in place of the composer. Both answer
//! through the model's actions (`respondPermission`, `answerQuestion`).

use std::collections::BTreeSet;

use model::acp::Event;
use model::acp::approvals::{PendingPermission, PendingQuestion};
use rune::{El, col, el, icon, row, text};
use serde_json::Value;
use theme::Tokens;
use theme::icons as i;

use super::Lenses;
use crate::paint::shadows;
use crate::type_::{MICRO, mono, sans};
use crate::{Button, ButtonSize, ButtonVariant, kbd};

/// One option of a permission request (`PermissionOption`).
struct Choice {
    id: String,
    name: String,
    kind: String,
}

impl Choice {
    fn allows(&self) -> bool {
        matches!(self.kind.as_str(), "allow_once" | "allow_always")
    }
    fn rejects(&self) -> bool {
        matches!(self.kind.as_str(), "reject_once" | "reject_always")
    }
}

fn choices(options: &Value) -> Vec<Choice> {
    let text = |o: &Value, k: &str| o.get(k).and_then(Value::as_str).unwrap_or_default().to_owned();
    options
        .as_array()
        .into_iter()
        .flatten()
        .map(|o| Choice { id: text(o, "optionId"), name: text(o, "name"), kind: text(o, "kind") })
        .collect()
}

/// `pickPrimaryAllowOption`: the narrowest allow, else the first.
fn primary_allow(choices: &[Choice]) -> Option<&str> {
    let mut allows = choices.iter().filter(|c| c.allows());
    let first = allows.clone().next();
    allows.find(|c| c.kind == "allow_once").or(first).map(|c| c.id.as_str())
}

/// `permissionToolTitle`.
pub fn permission_tool_title(tool_call: Option<&model::acp::types::Object>) -> String {
    let Some(call) = tool_call else { return "this action".into() };
    call.get("title")
        .or_else(|| call.get("toolCallId"))
        .and_then(Value::as_str)
        .unwrap_or("this action")
        .to_owned()
}

/// `PermissionPrompt` embedded in the composer card: the warning-tinted head ("Approval
/// needed", what the agent wants to run) and a button per option, allows first, rejects
/// last; with no reject option, "Cancel request".
pub fn permission_prompt<S: 'static>(t: &'static Tokens, p: &PendingPermission, phone: bool, lenses: Lenses<S>) -> El {
    let c = &t.colors;
    let choices = choices(&p.options);
    let primary = primary_allow(&choices).map(str::to_owned);
    let size = if phone { ButtonSize::Touch } else { ButtonSize::Xs };
    let answer = |option: Option<String>| {
        let request = p.request_id.clone();
        move |s: &mut S| {
            // `respond(requestId, optionId)` never runs for an option without an id.
            if option.as_deref() == Some("") {
                return;
            }
            (lenses.step)(s, Event::RespondPermission(request.clone(), option.clone()));
        }
    };
    let button = |choice: &Choice| {
        let is_primary = primary.as_deref() == Some(choice.id.as_str());
        let mut b = Button::new(choice.name.clone()).size(size);
        if choice.allows() {
            b = b.icon(&i::SHIELD_CHECK);
        } else if choice.rejects() {
            b = b.icon(&i::SHIELD_ALERT);
        }
        b = if is_primary {
            b.variant(ButtonVariant::Default)
        } else if choice.rejects() {
            b.variant(ButtonVariant::Ghost).ink(c.muted_foreground)
        } else {
            b.variant(ButtonVariant::Outline).fill(c.secondary.alpha(0.3))
        };
        let b = b.build(t).key(format!("permission-option-{}", choice.id)).radius(8.0);
        // `size="touch"` keeps the 44-point floor with `px-3`.
        let b = if phone { b.px(12.0) } else { b };
        b
            .on_press(answer(Some(choice.id.clone())))
    };
    let ordered = choices
        .iter()
        .filter(|c| c.allows())
        .chain(choices.iter().filter(|c| !c.allows() && !c.rejects()))
        .chain(choices.iter().filter(|c| c.rejects()));
    let mut buttons: Vec<El> = ordered.map(button).collect();
    let has_reject = choices.iter().any(Choice::rejects);
    if !has_reject {
        buttons.push(
            Button::new("Cancel request")
                .icon(&i::SHIELD_ALERT)
                .variant(ButtonVariant::Ghost)
                .size(size)
                .ink(c.muted_foreground)
                .build(t)
                .key("permission-cancel")
                .radius(8.0)
                .px(if phone { 12.0 } else { 10.0 })
                .on_press(answer(None)),
        );
    }
    let title = permission_tool_title(p.tool_call.as_ref());
    let line = row()
        .w_full()
        .min_w(0.0)
        .gap(4.0)
        .items_center()
        .flex_wrap()
        .child(text("The agent is waiting for permission to run").font(sans(12.0, 400.0, 19.5)).color(c.muted_foreground).wrap().min_w(0.0))
        .child(
            row()
                .min_w(0.0)
                .px(6.0)
                .py(2.0)
                .radius(4.0)
                .border(1.0, c.border.alpha(0.5))
                .fill(c.muted.alpha(0.6))
                .child(text(title).font(mono(12.0, 400.0, 16.0)).color(c.foreground).single_line().truncate().min_w(0.0)),
        )
        .child(text(".").font(sans(12.0, 400.0, 19.5)).color(c.muted_foreground));
    let head = row()
        .w_full()
        .gap(10.0)
        .items_start()
        .child(
            row()
                .mt(2.0)
                .size(24.0, 24.0)
                .center()
                .radius(6.0)
                .border(1.0, c.warning.alpha(0.3))
                .fill(c.warning.alpha(0.15))
                .child(icon(&i::SHIELD_ALERT, 13.0).color(c.warning)),
        )
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(2.0)
                .child(text("Approval needed").font(sans(12.0, 600.0, 20.0)).color(c.foreground))
                .child(line),
        );
    let mut card = col()
        .key(format!("permission-{}", p.request_id))
        .role("region")
        .label("Approval needed")
        .w_full()
        .px(if phone { 14.0 } else { 16.0 })
        .py(12.0)
        .radius(15.0)
        .border_sides([0.0, 0.0, 1.0, 0.0], c.warning.alpha(0.2))
        .fill(c.warning.alpha(0.035))
        .child(head);
    if choices.is_empty() {
        card = card.child(
            text("The agent provided no choices. Cancel the request to keep this action blocked.")
                .font(sans(12.0, 400.0, 19.5))
                .color(c.muted_foreground)
                .wrap()
                .ml(32.0)
                .mt(8.0),
        );
    }
    card = card.child(row().w_full().mt(10.0).gap(if phone { 12.0 } else { 8.0 }).items_center().flex_wrap().children(buttons));
    if has_reject {
        card = card.child(
            text("Choose an option to resume the agent. This request stays here until you respond.")
                .font(sans(MICRO, 400.0, 15.0))
                .color(c.muted_foreground.alpha(if phone { 1.0 } else { 0.7 }))
                .wrap()
                .mt(8.0),
        );
    }
    card
}

/// What the reader picked in an open question, keyed by question id.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Picks {
    picked: std::collections::BTreeMap<String, Vec<String>>,
    /// The typed answer of a question that allows one (`allowFreeText`).
    other: std::collections::BTreeMap<String, rune::TextEdit>,
}

impl Picks {
    pub fn of(&self, question_id: &str) -> &[String] {
        self.picked.get(question_id).map_or(&[], Vec::as_slice)
    }

    /// Toggle `value`: several picks for a multi-select question, else the one.
    pub fn toggle(&mut self, question_id: &str, value: &str, multi: bool) {
        let picked = self.picked.entry(question_id.to_owned()).or_default();
        if !multi {
            *picked = vec![value.to_owned()];
        } else if let Some(at) = picked.iter().position(|v| v == value) {
            picked.remove(at);
        } else {
            picked.push(value.to_owned());
        }
    }

    pub fn forget(&mut self, question_id: &str) {
        self.picked.remove(question_id);
        self.other.remove(question_id);
    }

    pub fn other_of(&self, question_id: &str) -> Option<&rune::TextEdit> {
        self.other.get(question_id)
    }

    /// The question's typed answer.
    pub fn other(&mut self, question_id: &str) -> &mut rune::TextEdit {
        self.other.entry(question_id.to_owned()).or_default()
    }

    /// `sendAnswer`: the picks, then the typed answer; on a single-choice question typing
    /// replaces the pick.
    pub fn answer(&self, question_id: &str, multi: bool) -> Vec<String> {
        let typed = self.other.get(question_id).map_or("", |e| e.text.trim());
        let picked = self.of(question_id).to_vec();
        match (typed.is_empty(), multi) {
            (true, _) => picked,
            (false, true) => picked.into_iter().chain([typed.to_owned()]).collect(),
            (false, false) => vec![typed.to_owned()],
        }
    }
}

/// One option of a question (`StepperOption`), with its `cardinality`.
struct QuestionOption {
    value: String,
    label: String,
    description: Option<String>,
    multi: bool,
}

fn question_options(q: &PendingQuestion) -> Vec<QuestionOption> {
    q.options
        .as_array()
        .into_iter()
        .flatten()
        .map(|o| {
            let s = |k: &str| o.get(k).and_then(Value::as_str).map(str::to_owned);
            QuestionOption {
                value: s("value").unwrap_or_default(),
                label: s("label").unwrap_or_default(),
                description: s("description").filter(|d| !d.is_empty()),
                multi: s("cardinality").as_deref() == Some("multi"),
            }
        })
        .collect()
}

/// `isMulti`.
pub fn question_is_multi(q: &PendingQuestion) -> bool {
    q.multi_select == Some(true) || question_options(q).iter().any(|o| o.multi)
}

/// `parseOptionLabel`: a trailing "(Recommended)" becomes the badge.
fn option_label(label: &str) -> (&str, bool) {
    let trimmed = label.trim_end();
    match trimmed.strip_suffix("(Recommended)") {
        Some(rest) => (rest.trim_end(), true),
        None => (label, false),
    }
}

/// `RowKey`: the option's number, which doubles as its shortcut.
fn row_key(t: &Tokens, n: usize, active: bool) -> El {
    let c = &t.colors;
    let (border, fill, ink) = if active {
        (c.primary.alpha(0.25), c.primary.alpha(0.2), c.primary)
    } else {
        (c.foreground.alpha(0.08), c.muted, c.muted_foreground)
    };
    row()
        .h(20.0)
        .min_w(20.0)
        .px(4.0)
        .center()
        .radius(5.0)
        .border(1.0, border)
        .fill(fill)
        .child(text(n.to_string()).font(mono(MICRO, 500.0, 16.0)).color(ink))
        .a11y_hidden()
}

/// `AskUserQuestion`: the composer turned into the question. "Agent question" and `×` (cancel)
/// on top, the question, a numbered row per option with its radio or check, and the footer
/// with the key hints and Submit, which waits for a pick.
pub fn question_card<S: 'static>(t: &'static Tokens, q: &PendingQuestion, picked: &[String], other: Option<&rune::TextEdit>, phone: bool, lenses: Lenses<S>) -> El {
    let c = &t.colors;
    let options = question_options(q);
    let multi = question_is_multi(q);
    let heading = q.question.as_str().unwrap_or_default().to_owned();
    let selected: BTreeSet<&str> = picked.iter().map(String::as_str).collect();
    let qid = q.question_id.clone();
    let close = {
        let qid = qid.clone();
        Button::icon_only(&i::X, "Cancel", ButtonSize::IconXs)
            .variant(ButtonVariant::Ghost)
            .ink(c.muted_foreground)
            .build(t)
            .key(format!("question-cancel-{qid}"))
            .on_press(move |s: &mut S| {
                (lenses.reader_mut)(s).picks.forget(&qid);
                (lenses.step)(s, Event::AnswerQuestion(qid.clone(), Vec::new()));
            })
    };
    let header = row()
        .w_full()
        .gap(10.0)
        .pl(16.0)
        .pr(8.0)
        .pt(10.0)
        .items_center()
        .child(
            row()
                .gap(8.0)
                .items_center()
                .child(icon(&i::HELP_CIRCLE, 14.0).color(c.muted_foreground))
                .child(text("Agent question").font(sans(12.0, 500.0, 16.0)).color(c.muted_foreground)),
        )
        .child(el().grow(1.0))
        .child(el().w(1.0).h(14.0).mx(4.0).fill(c.border))
        .child(close);
    let mut prompt = col()
        .w_full()
        .gap(6.0)
        .px(20.0)
        .pt(14.0)
        .pb(6.0)
        .child(text(heading.clone()).font(sans(16.0, 600.0, 22.0)).color(c.foreground).wrap());
    if multi {
        prompt = prompt.child(text("Select all that apply.").font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground));
    }
    let rows = options.iter().enumerate().map(|(n, o)| {
        let on = selected.contains(o.value.as_str());
        let (label, recommended) = option_label(&o.label);
        let mut title = row()
            .min_h(20.0)
            .gap(8.0)
            .items_center()
            .child(text(label.to_owned()).font(sans(14.0, 500.0, 20.0)).color(c.foreground));
        if recommended {
            title = title.child(
                row()
                    .h(18.0)
                    .px(6.0)
                    .gap(4.0)
                    .items_center()
                    .radius(9999.0)
                    .fill(c.primary.alpha(0.12))
                    .child(icon(&i::SPARKLES, 10.0).color(c.primary))
                    .child(text("Recommended").font(sans(MICRO, 500.0, 16.0)).color(c.primary)),
            );
        }
        let mut body = col().grow(1.0).min_w(0.0).child(title);
        if let Some(d) = &o.description {
            body = body.child(text(d.clone()).font(sans(12.0, 400.0, 18.0)).color(c.muted_foreground).wrap().mt(2.0));
        }
        let mark = row()
            .size(16.0, 16.0)
            .center()
            .radius(if multi { 5.0 } else { 8.0 })
            .border(1.5, if on { c.primary_fill } else { c.foreground.alpha(0.15) });
        let mark = if on {
            mark.fill(c.primary_fill).child(icon(&i::CHECK, 10.0).color(c.primary_foreground))
        } else {
            mark
        };
        let (qid, value) = (qid.clone(), o.value.clone());
        let mut r = row()
            .key(format!("question-option-{n}"))
            .role("button")
            .label(o.label.clone())
            .w_full()
            .min_h(if phone { 44.0 } else { 36.0 })
            .gap(12.0)
            .py(8.0)
            .pl(10.0)
            .pr(12.0)
            .items_start()
            .radius(8.0)
            .focusable(true)
            .child(row().h(20.0).items_center().child(row_key(t, n + 1, on)))
            .child(body)
            .child(row().h(20.0).items_center().child(mark))
            .on_press(move |s: &mut S| (lenses.reader_mut)(s).picks.toggle(&qid, &value, multi));
        r = if on { r.fill(c.primary.alpha(0.07)).border(1.0, c.primary.alpha(0.3)) } else { r.hover(c.foreground.alpha(0.03)) };
        r
    });
    let free = q.allow_free_text == Some(true);
    let typed = if free { other.map_or("", |e| e.text.as_str()).trim().to_owned() } else { String::new() };
    let rows: Vec<El> = rows.collect();
    let rows = if free {
        let (qid, sent) = (qid.clone(), qid.clone());
        let empty = rune::TextEdit::new("");
        let field = rune::text_input(other.unwrap_or(&empty), sans(14.0, 400.0, 20.0))
            .key(format!("question-other-{qid}"))
            .label("Your own answer")
            .placeholder("Something else? Type your own answer", c.muted_foreground.alpha(0.7))
            .color(c.foreground)
            .single_line()
            .grow(1.0)
            .min_w(0.0)
            .on_edit(move |s: &mut S| (lenses.reader_mut)(s).picks.other(&qid))
            .on_submit(move |s: &mut S, _tx: &mut rune::Tx| {
                let values = (lenses.reader_mut)(s).picks.answer(&sent, multi);
                if values.is_empty() {
                    return;
                }
                (lenses.reader_mut)(s).picks.forget(&sent);
                (lenses.step)(s, Event::AnswerQuestion(sent.clone(), values));
            });
        let other_row = row()
            .w_full()
            .min_h(if phone { 44.0 } else { 36.0 })
            .gap(12.0)
            .py(8.0)
            .pl(10.0)
            .pr(12.0)
            .items_center()
            .radius(8.0)
            .child(row_key(t, options.len() + 1, !typed.is_empty()))
            .child(icon(&i::EDIT, 13.0).color(c.muted_foreground.alpha(if typed.is_empty() { 0.7 } else { 1.0 })))
            .child(field);
        rows.into_iter().chain([other_row]).collect()
    } else {
        rows
    };
    let body = if options.is_empty() && !free {
        col().w_full().child(
            text("The agent provided no options.").font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground).px(20.0).pb(12.0),
        )
    } else {
        // `QUESTION_PANEL_MAX_H`: the rows scroll, the header and the footer stay.
        col().w_full().max_h(380.0).scroll(rune::ScrollSpec { stick_bottom: false }).child(col().w_full().gap(2.0).px(8.0).py(6.0).children(rows))
    };
    let escape = qid.clone();
    let mut card = col()
        .key(format!("question-{qid}"))
        .role("dialog")
        .label(heading)
        .w_full()
        // `QuestionStepper`'s Escape closes the question only while focus is inside it.
        .on_key(move |s: &mut S, key: &rune::KeyEvent, _tx: &mut rune::Tx| {
            if !matches!(key.key, rune::Key::Escape) {
                return false;
            }
            (lenses.reader_mut)(s).picks.forget(&escape);
            (lenses.step)(s, Event::AnswerQuestion(escape.clone(), Vec::new()));
            true
        })
        .child(header)
        .child(prompt)
        .child(body);
    if !options.is_empty() || free {
        let can_submit = !picked.is_empty() || !typed.is_empty();
        let submit = {
            let qid = qid.clone();
            Button::new("Submit")
                .size(ButtonSize::Sm)
                .disabled(!can_submit)
                .build(t)
                .key(format!("question-submit-{qid}"))
                .radius(8.0)
                .h(if phone { 44.0 } else { 32.0 })
                .pr(if phone { 14.0 } else { 6.0 })
                .child(if phone {
                    // A keyboard hint: touch has no Enter key to press here.
                    el()
                } else {
                    row()
                        .size(20.0, 20.0)
                        .center()
                        .radius(5.0)
                        .fill(c.primary_foreground.alpha(0.15))
                        .child(text("↵").font(sans(MICRO, 400.0, 16.0)).color(c.primary_foreground))
                        .a11y_hidden()
                })
                .on_press(move |s: &mut S| {
                    let values = (lenses.reader_mut)(s).picks.answer(&qid, multi);
                    if values.is_empty() {
                        return;
                    }
                    (lenses.reader_mut)(s).picks.forget(&qid);
                    (lenses.step)(s, Event::AnswerQuestion(qid.clone(), values));
                })
        };
        let count = options.len();
        let picks = if count > 1 { format!("1–{count}") } else { "1".to_owned() };
        let hints = [(picks, "pick"), ("esc".to_owned(), "close")].map(|(k, w)| {
            row()
                .gap(6.0)
                .items_center()
                .child(kbd(t, k))
                .child(text(w).font(sans(MICRO, 400.0, 16.0)).color(c.muted_foreground.alpha(0.7)))
        });
        let mut right = row().gap(6.0).items_center();
        if multi && !picked.is_empty() {
            right = right.child(text(format!("{} selected", picked.len())).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground.alpha(0.7)).mr(4.0));
        }
        let mut footer = row()
            .w_full()
            .gap(8.0)
            .py(10.0)
            .pl(if phone { 12.0 } else { 20.0 })
            .pr(10.0)
            .items_center()
            .border_sides([1.0, 0.0, 0.0, 0.0], c.border);
        if !phone {
            footer = footer.child(row().gap(14.0).items_center().min_w(0.0).children(hints).a11y_hidden());
        }
        card = card.child(footer.child(el().grow(1.0)).child(right.child(submit)));
    }
    shadows(card.radius(16.0).border(1.0, c.border.alpha(0.6)).fill(c.card), &t.shadows.prompt_panel_shadow)
}
