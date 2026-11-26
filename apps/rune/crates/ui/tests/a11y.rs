//! What each base component tells assistive technology, read from
//! `Runtime::accessibility_json`. Each component's tree (without bounds) is a golden under
//! `tests/a11y/<name>.json`; accept an intended change with `RUNE_BLESS=1` and review the diff.

use std::path::PathBuf;

use rune::{Cx, El, Env, Runtime, TextEdit, col, text};
use serde_json::Value;
use theme::icons as i;
use ui::*;

#[derive(Default)]
struct App {
    edit: TextEdit,
}

fn t() -> &'static Tokens {
    theme::tokens("termul", true)
}

fn tree(view: fn(&App, &mut Cx<App>) -> El) -> Value {
    let mut rt = Runtime::new(App::default(), view, (640.0, 480.0), Env::new(true, 1.0));
    rt.frame(0.0);
    rt.settle_all();
    rt.frame(1.0 / 60.0);
    rt.accessibility_json()
}

fn strip(v: &mut Value) {
    if let Some(o) = v.as_object_mut() {
        for k in ["bounds", "scroll", "selection", "shortcut"] {
            o.remove(k);
        }
        if let Some(Value::Array(c)) = o.get_mut("children") {
            c.iter_mut().for_each(strip);
        }
    }
}

fn golden(name: &str, view: fn(&App, &mut Cx<App>) -> El) {
    let mut fresh = tree(view);
    strip(&mut fresh);
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/a11y").join(format!("{name}.json"));
    if std::env::var_os("RUNE_BLESS").is_some_and(|v| v != "0") {
        std::fs::write(&path, serde_json::to_string_pretty(&fresh).unwrap() + "\n").unwrap();
        return;
    }
    // Compared as JSON values: the repo's formatter rewraps the files.
    let want: Value = serde_json::from_str(
        &std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("missing golden {name}; run with RUNE_BLESS=1")),
    )
    .unwrap();
    assert!(
        want == fresh,
        "accessibility golden differs: {name}\n--- golden\n{want:#}\n--- fresh\n{fresh:#}\naccept with RUNE_BLESS=1 after review"
    );
}

/// The first node whose `role` is `role`.
fn find<'a>(v: &'a Value, role: &str) -> Option<&'a Value> {
    if v["role"] == role {
        return Some(v);
    }
    v["children"].as_array()?.iter().find_map(|c| find(c, role))
}

#[test]
fn button() {
    golden("button", |_, _| {
        col()
            .child(Button::new("Run task").build(t()).on_press(|_: &mut App| {}))
            .child(Button::new("Disabled").disabled(true).build(t()))
            .child(Button::icon_only(&i::SETTINGS, "Settings", ButtonSize::Icon).build(t()))
            .child(Button::new("Read the docs").variant(ButtonVariant::Link).as_link().build(t()))
    });
    let tree = tree(|_, _| Button::new("Run task").disabled(true).build(t()));
    let b = find(&tree, "button").expect("a button");
    assert_eq!(b["name"], "Run task");
    assert!(b["states"].as_array().is_some_and(|s| s.iter().any(|s| s == "disabled")), "{b}");
}

#[test]
fn badge() {
    golden("badge", |_, _| col().child(Badge::new("default").build(t())).child(Badge::new("outline").variant(BadgeVariant::Outline).build(t())));
}

#[test]
fn count_badge() {
    golden("count-badge", |_, _| col().child(CountBadge::new(1500).max(99).build(t())));
    assert_eq!(CountBadge::new(1500).max(99).text(), "99+");
    assert_eq!(CountBadge::new(42).max(99).text(), "42");
}

#[test]
fn kbd() {
    golden("kbd", |_, _| ui::kbd(t(), "Ctrl+K"));
}

#[test]
fn label() {
    golden("label", |_, _| col().child(ui::label(t(), "Default", false)).child(ui::label(t(), "Disabled", true)));
}

#[test]
fn separator() {
    golden("separator", |_, _| col().w(100.0).child(text("Above")).child(ui::separator(t(), Orientation::Horizontal)).child(text("Below")));
}

#[test]
fn skeleton() {
    golden("skeleton", |_, _| col().child(ui::skeleton(t()).w(100.0).h(16.0)));
}

#[test]
fn spinner() {
    golden("spinner", |_, _| col().child(Spinner::new().label("Loading, small").build(t().colors.foreground, true)).child(Spinner::new().decorative().build(t().colors.foreground, true)));
}

#[test]
fn shimmer_text() {
    golden("shimmer-text", |_, _| ui::shimmer_text(t(), "Thinking about the diff", type_::TEXT_SM, false));
}

#[test]
fn marquee_text() {
    golden("marquee-text", |_, _| col().w(120.0).child(ui::marquee_text(t(), "feature/very-long-branch-name", type_::TEXT_XS, 200.0, 120.0, true)));
}

#[test]
fn diff_stat_counts() {
    golden("diff-stat-counts", |_, _| ui::diff_stat_counts(t(), 128, 14, false, type_::TEXT_SM).unwrap());
    assert!(ui::diff_stat_counts(t(), 0, 0, true, type_::TEXT_SM).is_none());
}

#[test]
fn input() {
    golden("input", |s, _| col().child(Input::new(&s.edit).placeholder("api-server").label("Name").build(t())).child(Input::new(&s.edit).label("Read only").disabled(true).build(t())));
}

#[test]
fn textarea() {
    golden("textarea", |s, _| Textarea::new(&s.edit).label("Describe the change").rows(3).build(t()));
}

#[test]
fn switch() {
    golden("switch", |_, _| col().child(Switch::new(true).label("Wrap lines").build(t())).child(Switch::new(false).label("Disabled").disabled(true).build(t())));
}

#[test]
fn toggle() {
    golden("toggle", |_, _| col().child(Toggle::new("Bold").pressed(true).build(t())).child(Toggle::icon(&i::TERMINAL, "Wrap").build(t())));
}

#[test]
fn panel_styles() {
    golden("panel-styles", |s, _| {
        col()
            .child(Segmented::new("Git view", &["changes", "history"], 0).build(t(), "git", true, |_: &mut App, _| {}))
            .child(pressed_toggle(t(), ".ts", true))
            .child(panel_field(t(), &s.edit, "Filter files", false))
            .child(panel_header(t(), "Explorer", vec![panel_icon_button(t(), &i::PLUS, "New file")]))
            .child(quiet_icon_button(t(), &i::MORE_HORIZONTAL, "More", 32.0, 16.0))
    });
}

#[test]
fn icon_action_button() {
    golden("icon-action-button", |_, _| {
        icon_action_group(t(), true, vec![IconActionButton::new(&i::COPY, "Copy").build(t()), IconActionButton::new(&i::TRASH2, "Delete").disabled(true).build(t())])
    });
}

#[test]
fn bubble() {
    golden("bubble", |_, _| bubble_group(vec![Bubble::new("Ship it.").variant(BubbleVariant::Secondary).reactions(bubble_reactions(t(), "2 likes")).build(t())]));
}

#[test]
fn message() {
    golden("message", |_, _| {
        Message::new(MessageAlign::End)
            .avatar(text("FI"))
            .header("You · 09:12")
            .body(Bubble::new("Add a health check.").build(t()))
            .build(t())
    });
}

#[test]
fn marker() {
    golden("marker", |_, _| col().w(300.0).child(Marker::new("Switched to dev").icon(&i::GIT_BRANCH).build(t())).child(Marker::new("Today").variant(MarkerVariant::Separator).build(t())));
}

#[test]
fn scroll_area() {
    golden("scroll-area", |_, _| col().h(80.0).child(ui::scroll_area(t(), "list", col().children((0..12).map(|n| text(format!("row {n}")))))));
}

#[test]
fn accordion() {
    golden("accordion", |_, _| {
        Accordion::new(vec![AccordionItem::new("Shell", "Default shell.").open(true), AccordionItem::new("Font", "Font family.")]).build(t(), "acc", |_: &mut App, _| {})
    });
    let tree = tree(|_, _| Accordion::new(vec![AccordionItem::new("Shell", "Default shell.").open(true)]).build(t(), "acc", |_: &mut App, _| {}));
    let trigger = find(&tree, "button").expect("a trigger");
    assert!(trigger["states"].as_array().is_some_and(|s| s.iter().any(|s| s == "expanded")), "{trigger}");
}

#[test]
fn collapsible() {
    golden("collapsible", |_, _| Collapsible::new(Button::new("3 files").build(t()), text("README.md"), true).build());
}

#[test]
fn write_gate() {
    golden("write-gate", |_, _| ui::write_gate(t(), true, Button::new("Edit file").disabled(true).build(t())));
}

#[test]
fn icon_swap() {
    golden("icon-swap", |_, _| ui::icon_swap(&i::COPY, "copy", 16.0, t().colors.foreground));
}

#[test]
fn resizable() {
    golden("resizable", |_, _| col().w(100.0).h(100.0).child(resizable_handle(t(), ResizableDirection::Horizontal, true)));
}
