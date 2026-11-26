//! Every bundled theme resolves the colours the TypeScript engine records, and a sample of them
//! equals what headless Chromium paints in the real web client
//! (`tests/fixtures/chromium-tokens.json`, `scripts/rune/record-theme-fixture.ts`).

use rune::theme::hex;
use serde_json::Value;
use theme::{Colors, Easings, Numbers, Shadows, Syntax, Terminal, Tokens};

fn file(id: &str) -> Value {
    let (_, json) = theme::THEMES
        .iter()
        .find(|(t, _)| *t == id)
        .expect("bundled");
    serde_json::from_str(json).expect("theme json")
}

fn object<'a>(doc: &'a Value, key: &str) -> &'a serde_json::Map<String, Value> {
    doc[key]
        .as_object()
        .unwrap_or_else(|| panic!("section {key}"))
}

#[test]
fn twenty_themes_in_the_web_order() {
    let ids: Vec<&str> = theme::all().iter().map(|t| t.id).collect();
    assert_eq!(
        ids,
        [
            "termul",
            "cursor",
            "catppuccin",
            "dracula",
            "nord",
            "gruvbox",
            "tokyonight",
            "ayu",
            "one-dark",
            "github",
            "termul-light",
            "cursor-light",
            "catppuccin-light",
            "dracula-light",
            "nord-light",
            "gruvbox-light",
            "tokyonight-light",
            "ayu-light",
            "one-dark-light",
            "github-light",
        ]
    );
}

#[test]
fn every_token_equals_the_generated_file() {
    for t in theme::all() {
        let doc = file(t.id);
        let mode = if t.dark { "dark" } else { "light" };
        assert_eq!(doc["appearance"], mode, "{}", t.id);

        let colors = object(&doc, mode);
        assert_eq!(colors.len(), Colors::NAMES.len(), "{}: colour count", t.id);
        for (name, value) in colors {
            let want = hex(value.as_str().unwrap()).unwrap();
            assert_eq!(t.colors.get(name), Some(&want), "{}: {name}", t.id);
        }

        let shadows = object(&doc, "shadows");
        assert_eq!(
            shadows.len(),
            Shadows::NAMES.len(),
            "{}: shadow count",
            t.id
        );
        for (name, value) in shadows {
            let want = rune_kit::css::box_shadows(value.as_str().unwrap(), |c| c);
            assert_eq!(t.shadows.get(name), Some(&want), "{}: {name}", t.id);
        }

        let numbers = object(&doc, "numbers");
        assert_eq!(
            numbers.len(),
            Numbers::NAMES.len(),
            "{}: number count",
            t.id
        );
        for (name, value) in numbers {
            assert_eq!(
                t.numbers.get(name),
                Some(&(value.as_f64().unwrap() as f32)),
                "{}: {name}",
                t.id
            );
        }

        let easings = object(&doc, "easings");
        assert_eq!(
            easings.len(),
            Easings::NAMES.len(),
            "{}: easing count",
            t.id
        );
        for (name, value) in easings {
            let points: Vec<f32> = value
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap() as f32)
                .collect();
            assert_eq!(
                t.easings.get(name).map(|e| e.to_vec()),
                Some(points),
                "{}: {name}",
                t.id
            );
        }

        for (section, names) in [("terminal", Terminal::NAMES), ("syntax", Syntax::NAMES)] {
            let colors = object(&doc, section);
            assert_eq!(colors.len(), names.len(), "{}: {section} count", t.id);
            for (name, value) in colors {
                let want = hex(value.as_str().unwrap()).unwrap();
                let got = if section == "terminal" {
                    t.terminal.get(name)
                } else {
                    t.syntax.get(name)
                };
                assert_eq!(got, Some(&want), "{}: {section} {name}", t.id);
            }
        }
    }
}

#[test]
fn sampled_tokens_equal_what_chromium_paints() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/chromium-tokens.json")).expect("fixture json");
    let themes = object(&fixture, "themes");
    assert_eq!(themes.len(), 20);
    let mut compared = 0;
    for (id, tokens) in themes {
        let t = theme::by_id(id).unwrap_or_else(|| panic!("fixture theme {id} is bundled"));
        let tokens = tokens.as_object().unwrap();
        assert_eq!(tokens.len(), 20, "{id}: 20 sampled tokens");
        for (name, painted) in tokens {
            let ours = t
                .colors
                .get(name)
                .unwrap_or_else(|| panic!("{id}: no token {name}"));
            assert_eq!(ours.hex(), painted.as_str().unwrap(), "{id}: {name}");
            compared += 1;
        }
    }
    assert_eq!(compared, 400);
}

#[test]
fn known_values_of_troupe_dark_and_light() {
    let dark: &Tokens = theme::tokens("termul", true);
    assert_eq!(dark.colors.background.hex(), "#08090aff");
    assert_eq!(dark.colors.card.hex(), "#0f1011ff");
    assert_eq!(dark.colors.popover_edge.hex(), "#38393aff");
    assert_eq!(dark.colors.row_hover.hex(), "#e5e5e608");
    assert_eq!(dark.numbers.radius, 8.0);
    assert_eq!(dark.easings.ease_out, [0.23, 1.0, 0.32, 1.0]);
    assert_eq!(dark.shadows.shadow_control.len(), 2);
    assert!(
        dark.shadows.switch_thumb_shadow.is_empty(),
        "`none` has no layers"
    );
    let light = theme::tokens("termul", false);
    assert_eq!((light.id, light.dark), ("termul-light", false));
}

#[test]
fn families_resolve_like_the_web_client() {
    assert_eq!(theme::theme_id("nord", false), "nord-light");
    assert_eq!(theme::theme_id("nord-light", true), "nord");
    assert_eq!(theme::tokens("catppuccin-light", true).id, "catppuccin");
    assert_eq!(theme::tokens("no-such-theme", true).id, "termul");
    assert_eq!(theme::tokens("no-such-theme", false).id, "termul-light");
}

#[test]
fn the_kit_takes_the_troupe_tokens() {
    for t in theme::all() {
        let k = theme::kit(&t.family, t.dark);
        assert_eq!(k.dark, t.dark, "{}", t.id);
        assert_eq!(k.c.page, t.colors.background, "{}", t.id);
        assert_eq!(k.c.text_base, t.colors.foreground, "{}", t.id);
        assert_eq!(k.c.text_muted, t.colors.muted_foreground, "{}", t.id);
        assert_eq!(k.c.border_focus, t.colors.focus_ring, "{}", t.id);
        assert_eq!(k.c.bg_accent, t.colors.primary_fill, "{}", t.id);
        assert_eq!(k.radius.lg, 8.0, "{}", t.id);
    }
}
