//! `ElicitationPrompt`: an agent's form or link request in place of the composer. A form's
//! fields are typed (text, number, whole number, a yes/no, one pick, several picks) and checked
//! on Submit as the web client checks them; Cancel, Decline and Submit (Done for a link) answer
//! through the model's `respondElicitation`.

use std::collections::BTreeMap;

use model::acp::Event;
use model::acp::approvals::PendingElicitation;
use rune::{El, TextEdit, col, icon, row, text};
use serde_json::{Map, Value, json};
use theme::Tokens;
use theme::icons as i;

use super::Lenses;
use crate::type_::sans;
use crate::{Button, ButtonSize, ButtonVariant, Input};

/// One field of the form (`ElicitationField`).
#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub name: String,
    pub kind: String,
    pub required: bool,
    pub title: Option<String>,
    pub description: Option<String>,
    /// `(value, label, description)`.
    pub options: Vec<(String, String, Option<String>)>,
}

impl Field {
    /// The title when not blank, else the name.
    pub fn label(&self) -> &str {
        self.title.as_deref().filter(|t| !t.trim().is_empty()).unwrap_or(&self.name)
    }
}

pub fn fields(p: &PendingElicitation) -> Vec<Field> {
    let s = |o: &model::acp::types::Object, k: &str| o.get(k).and_then(Value::as_str).map(str::to_owned);
    p.fields
        .iter()
        .map(|f| Field {
            name: s(f, "name").unwrap_or_default(),
            kind: s(f, "kind").unwrap_or_else(|| "string".into()),
            required: f.get("required").and_then(Value::as_bool).unwrap_or(false),
            title: s(f, "title"),
            description: s(f, "description").filter(|d| !d.is_empty()),
            options: f
                .get("options")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|o| {
                    let g = |k: &str| o.get(k).and_then(Value::as_str).map(str::to_owned);
                    (g("value").unwrap_or_default(), g("label").unwrap_or_default(), g("description").filter(|d| !d.is_empty()))
                })
                .collect(),
        })
        .collect()
}

/// What the reader entered in an open form.
#[derive(Clone, Debug, Default)]
pub struct Form {
    pub texts: BTreeMap<String, TextEdit>,
    pub flags: BTreeMap<String, bool>,
    pub picks: BTreeMap<String, Vec<String>>,
    /// The field the last Submit stopped at, and why.
    pub error: Option<(String, String)>,
}

impl PartialEq for Form {
    fn eq(&self, other: &Self) -> bool {
        let texts = |f: &Form| f.texts.iter().map(|(k, v)| (k.clone(), v.text.clone())).collect::<Vec<_>>();
        texts(self) == texts(other) && self.flags == other.flags && self.picks == other.picks && self.error == other.error
    }
}

impl Form {
    pub fn text(&mut self, field: &str) -> &mut TextEdit {
        self.texts.entry(field.to_owned()).or_insert_with(|| TextEdit::new(""))
    }

    /// One pick for a single-choice field; a toggle for a multi-choice one.
    pub fn pick(&mut self, field: &str, value: &str, multi: bool) {
        let picked = self.picks.entry(field.to_owned()).or_default();
        if !multi {
            *picked = vec![value.to_owned()];
        } else if let Some(at) = picked.iter().position(|v| v == value) {
            picked.remove(at);
        } else {
            picked.push(value.to_owned());
        }
        if self.error.as_ref().is_some_and(|(f, _)| f == field) {
            self.error = None;
        }
    }
}

/// `submit('accept')`'s checks: the content to send, or the field that stops it and why. A
/// phone sends an untouched required yes/no as `false` (its switch shows Off).
pub fn validate(fields: &[Field], form: &Form, phone: bool) -> Result<Map<String, Value>, (String, String)> {
    let mut content = Map::new();
    for f in fields {
        let label = f.label();
        let fail = |message: String| Err((f.name.clone(), message));
        match f.kind.as_str() {
            "boolean" => match form.flags.get(&f.name) {
                Some(&v) => {
                    content.insert(f.name.clone(), json!(v));
                }
                None if f.required && phone => {
                    content.insert(f.name.clone(), json!(false));
                }
                None if f.required => return fail(format!("{label} is required.")),
                None => {}
            },
            "number" | "integer" => {
                let raw = form.texts.get(&f.name).map_or("", |e| e.text.as_str());
                if raw.is_empty() {
                    if f.required {
                        return fail(format!("{label} is required."));
                    }
                    continue;
                }
                // `Number(raw)`: blank-padded decimals, no trailing garbage.
                let number = raw.trim().parse::<f64>().ok().filter(|n| n.is_finite());
                match number {
                    Some(n) if f.kind == "number" || n.fract() == 0.0 => {
                        content.insert(f.name.clone(), if n.fract() == 0.0 && n.abs() < 9.0e15 { json!(n as i64) } else { json!(n) });
                    }
                    _ if f.kind == "integer" => return fail(format!("{label} must be a whole number.")),
                    _ => return fail(format!("{label} must be a finite number.")),
                }
            }
            "multi-enum" => {
                let list = form.picks.get(&f.name).cloned().unwrap_or_default();
                if list.is_empty() {
                    if f.required {
                        return fail(format!("{label} is required."));
                    }
                    continue;
                }
                content.insert(f.name.clone(), json!(list));
            }
            "enum" => {
                let pick = form.picks.get(&f.name).and_then(|p| p.first()).cloned().unwrap_or_default();
                if pick.is_empty() {
                    if f.required {
                        return fail(format!("{label} is required."));
                    }
                    continue;
                }
                content.insert(f.name.clone(), json!(pick));
            }
            _ => {
                let words = form.texts.get(&f.name).map_or("", |e| e.text.as_str());
                if words.is_empty() {
                    if f.required {
                        return fail(format!("{label} is required."));
                    }
                    continue;
                }
                content.insert(f.name.clone(), json!(words));
            }
        }
    }
    Ok(content)
}

/// How the card answers beyond the model: a toast for a refused Submit, the link opener.
#[derive(Clone, Copy)]
pub struct Hooks<S> {
    pub toast: fn(&mut S, &str),
    pub open_url: fn(&mut S, &str, &mut rune::Tx),
    pub form: for<'a> fn(&'a mut S, &str) -> &'a mut Form,
}

/// One option of a pick field: its check or radio mark, label and description.
struct OptionRow<'a> {
    key: String,
    label: String,
    description: Option<&'a str>,
    on: bool,
    multi: bool,
    phone: bool,
}

fn option_row<S: 'static>(t: &Tokens, o: OptionRow, press: impl Fn(&mut S) + 'static) -> El {
    let OptionRow { key, label, description, on, multi, phone } = o;
    let c = &t.colors;
    let mark = row().size(16.0, 16.0).center().radius(if multi { 4.0 } else { 8.0 }).border(1.5, if on { c.primary_fill } else { c.foreground.alpha(0.25) });
    let mark = if on { mark.fill(c.primary_fill).child(icon(&i::CHECK, 10.0).color(c.primary_foreground)) } else { mark };
    let mut words = row().gap(4.0).min_w(0.0).grow(1.0).child(text(label.clone()).font(sans(14.0, 400.0, 20.0)).color(c.foreground));
    if let Some(d) = description {
        words = words.child(text(format!("\u{2014} {d}")).font(sans(14.0, 400.0, 20.0)).color(c.muted_foreground).single_line().truncate().min_w(0.0));
    }
    row()
        .key(key)
        .role(if multi { "checkbox" } else { "radio" })
        .label(label)
        .checked(on)
        .w_full()
        .min_h(if phone { 44.0 } else { 28.0 })
        .gap(8.0)
        .items_center()
        .focusable(true)
        .child(mark)
        .child(words)
        .on_press(press)
}

/// The prompt for `p`, with what the reader entered in `form`.
pub fn prompt<S: 'static>(t: &'static Tokens, p: &PendingElicitation, form: &Form, phone: bool, lenses: Lenses<S>, hooks: Hooks<S>) -> El {
    let c = &t.colors;
    let id = p.request_id.clone();
    let mode = p.mode.as_str().unwrap_or("form").to_owned();
    let message = p.message.as_str().unwrap_or_default().to_owned();
    let fields = fields(p);
    let size = if phone { ButtonSize::Touch } else { ButtonSize::Sm };
    let head = row()
        .w_full()
        .gap(10.0)
        .items_start()
        .child(row().mt(2.0).size(24.0, 24.0).center().radius(6.0).border(1.0, c.border.alpha(0.6)).fill(c.secondary).child(icon(&i::FILE_QUESTION, 13.0).color(c.muted_foreground)))
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(2.0)
                .child(text("Request from the agent").font(sans(12.0, 600.0, 20.0)).color(c.foreground))
                .child(text(message.clone()).font(sans(12.0, 400.0, 19.5)).color(c.muted_foreground).wrap()),
        );
    let mut body = col().w_full().mt(12.0).gap(12.0);
    if mode == "url"
        && let Some(url) = p.url.clone()
    {
        body = body.child(
            row().child(
                Button::new("Open link")
                    .variant(ButtonVariant::Outline)
                    .size(size)
                    .build(t)
                    .key(format!("elicitation-open-{id}"))
                    .on_press_tx(move |s: &mut S, tx: &mut rune::Tx| (hooks.open_url)(s, &url, tx)),
            ),
        );
    }
    if mode == "form" {
        for f in &fields {
            let caption = {
                let mut cap = col().w_full().child(text(f.label().to_owned()).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground));
                if let Some(d) = &f.description {
                    cap = cap.child(text(d.clone()).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground.alpha(0.8)).wrap());
                }
                cap
            };
            let (rid, name) = (id.clone(), f.name.clone());
            let control = match f.kind.as_str() {
                "boolean" => {
                    let on = form.flags.get(&f.name).copied().unwrap_or(false);
                    option_row(t, OptionRow { key: format!("elicitation-field-{name}"), label: "Yes".into(), description: None, on, multi: true, phone }, move |s: &mut S| {
                        let form = (hooks.form)(s, &rid);
                        form.flags.insert(name.clone(), !on);
                        form.error = None;
                    })
                }
                "enum" | "multi-enum" => {
                    let multi = f.kind == "multi-enum";
                    let picked = form.picks.get(&f.name).cloned().unwrap_or_default();
                    col().w_full().gap(4.0).role(if multi { "group" } else { "radiogroup" }).label(f.label().to_owned()).children(f.options.iter().enumerate().map(|(n, (value, label, description))| {
                        let (rid, name, value) = (rid.clone(), name.clone(), value.clone());
                        let on = picked.contains(&value);
                        option_row(t, OptionRow { key: format!("elicitation-field-{name}-{n}"), label: label.clone(), description: description.as_deref(), on, multi, phone }, move |s: &mut S| {
                            (hooks.form)(s, &rid).pick(&name, &value, multi)
                        })
                    }))
                }
                _ => {
                    let empty = TextEdit::new("");
                    let edit = form.texts.get(&f.name).unwrap_or(&empty);
                    let invalid = form.error.as_ref().is_some_and(|(e, _)| e == &f.name);
                    Input::new(edit)
                        .coarse(phone)
                        .invalid(invalid)
                        .label(f.label().to_owned())
                        .build(t)
                        .key(format!("elicitation-field-{name}"))
                        .on_edit(move |s: &mut S| (hooks.form)(s, &rid).text(&name))
                }
            };
            let mut block = col().w_full().gap(4.0).child(caption).child(control);
            if let Some((_, why)) = form.error.as_ref().filter(|(e, _)| e == &f.name) {
                block = block.child(text(why.clone()).font(sans(12.0, 400.0, 16.0)).color(c.destructive).role("alert"));
            }
            body = body.child(block);
        }
    }
    let answer = |action: &'static str| {
        let (id, fields) = (id.clone(), fields.clone());
        let form_mode = mode == "form";
        move |s: &mut S| {
            let content = if action == "accept" && form_mode {
                let checked = validate(&fields, (hooks.form)(s, &id), phone);
                match checked {
                    Ok(content) => Some(Value::Object(content)),
                    Err((field, why)) => {
                        (hooks.toast)(s, &why);
                        (hooks.form)(s, &id).error = Some((field, why));
                        return;
                    }
                }
            } else if action == "accept" {
                Some(Value::Object(Map::new()))
            } else {
                None
            };
            (lenses.step)(s, Event::RespondElicitation(id.clone(), action.to_owned(), content));
        }
    };
    let button = |label: &'static str, action: &'static str, primary: bool| {
        let b = Button::new(label).size(size);
        let b = if primary { b } else { b.variant(ButtonVariant::Outline) };
        b.build(t).key(format!("elicitation-{action}-{id}")).on_press(answer(action))
    };
    body = body.child(
        row()
            .w_full()
            .justify_end()
            .gap(if phone { 12.0 } else { 8.0 })
            .child(button("Cancel", "cancel", false))
            .child(button("Decline", "decline", false))
            .child(button(if mode == "url" { "Done" } else { "Submit" }, "accept", true)),
    );
    let card = col()
        .key(format!("elicitation-{}", p.request_id))
        .role("dialog")
        .label(message)
        .w_full()
        .px(if phone { 14.0 } else { 16.0 })
        .py(12.0)
        .radius(16.0)
        .border(1.0, c.border.alpha(0.6))
        .fill(c.card)
        .child(head)
        .child(body);
    crate::paint::shadows(card, &t.shadows.prompt_panel_shadow)
}
