use rune::TextEdit;
use serde_json::json;

use super::elicitation::{Field, Form, validate};

fn field(name: &str, kind: &str, required: bool) -> Field {
    Field { name: name.into(), kind: kind.into(), required, title: None, description: None, options: Vec::new() }
}

fn typed(pairs: &[(&str, &str)]) -> Form {
    let mut form = Form::default();
    for (k, v) in pairs {
        form.texts.insert((*k).into(), TextEdit::new(*v));
    }
    form
}

#[test]
fn typed_fields_are_checked_as_the_web_client_checks_them() {
    let count = [field("count", "integer", true)];
    assert_eq!(validate(&count, &typed(&[("count", "3")]), false), Ok(json!({ "count": 3 }).as_object().unwrap().clone()));
    assert_eq!(validate(&count, &typed(&[("count", "2.5")]), false), Err(("count".into(), "count must be a whole number.".into())));
    assert_eq!(validate(&count, &typed(&[]), false), Err(("count".into(), "count is required.".into())));
    let ratio = [Field { title: Some("Ratio".into()), ..field("ratio", "number", false) }];
    assert_eq!(validate(&ratio, &typed(&[("ratio", "abc")]), false), Err(("ratio".into(), "Ratio must be a finite number.".into())));
    assert_eq!(validate(&ratio, &typed(&[("ratio", "0.5")]), false), Ok(json!({ "ratio": 0.5 }).as_object().unwrap().clone()));
    assert_eq!(validate(&ratio, &typed(&[]), false), Ok(serde_json::Map::new()), "an empty optional field is left out");
}

#[test]
fn a_required_yes_no_and_picks() {
    let ok = [field("ok", "boolean", true)];
    assert_eq!(validate(&ok, &Form::default(), false), Err(("ok".into(), "ok is required.".into())));
    assert_eq!(validate(&ok, &Form::default(), true), Ok(json!({ "ok": false }).as_object().unwrap().clone()), "a phone's switch shows Off");
    let mut form = Form::default();
    form.pick("tags", "a", true);
    form.pick("tags", "b", true);
    form.pick("tags", "a", true);
    form.pick("branch", "main", false);
    form.pick("branch", "dev", false);
    let fields = [field("tags", "multi-enum", true), field("branch", "enum", true), field("note", "string", false)];
    assert_eq!(validate(&fields, &form, false), Ok(json!({ "tags": ["b"], "branch": "dev" }).as_object().unwrap().clone()));
}
