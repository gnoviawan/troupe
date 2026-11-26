//! Troupe's bundled themes as data for the Rune client: every token the web client's theme engine
//! resolves (`assets/themes/<id>.json`, written by `scripts/rune/gen-themes.ts`), the fonts it
//! bundles, its icon catalog, and the mapping onto `rune_kit::Kit`.
//!
//! ```ignore
//! let t = theme::tokens("termul", cx.system_dark());
//! let k = theme::kit("termul", cx.system_dark());
//! el().fill(t.colors.card).child(rune_kit::Button::new("Save").build(k))
//! ```

#![forbid(unsafe_code)]

#[rustfmt::skip]
mod generated;
pub mod icons;
mod kit;

use std::sync::OnceLock;

use rune::{Color, Shadow, Theme};
use serde_json::Value;

pub use generated::{Colors, Easings, Numbers, Shadows, Syntax, THEMES, Terminal};
pub use kit::kit;

/// The theme a user with no saved choice sees (`DEFAULT_COLOR_THEME_ID`).
pub const DEFAULT_FAMILY: &str = "termul";

/// Inter (variable, upright and italic), Paper Mono and the Termul Symbols fallback, for
/// `AppSpec::fonts`. The same files the web client bundles, converted from woff2
/// (`scripts/rune/gen-fonts.ts`).
pub const FONTS: &[&[u8]] = &[
    include_bytes!("../assets/fonts/InterVariable.ttf"),
    include_bytes!("../assets/fonts/InterVariable-Italic.ttf"),
    include_bytes!("../assets/fonts/PaperMono.ttf"),
    include_bytes!("../assets/fonts/TermulSymbols.ttf"),
];

/// `AppSpec::families`: (sans, mono), as `lib/font-stacks.ts` names them.
pub const FAMILIES: (&str, &str) = ("Inter", "Paper Mono, Termul Symbols");

/// The stylistic sets `body` turns on for Inter (`index.css`). Rune has no app-wide default, so
/// every sans `TextStyle` carries them.
pub const INTER_FEATURES: &str = "'cv02' 1, 'cv11' 1, 'ss01' 1, 'ss03' 1";

/// One bundled theme's tokens, typed.
#[derive(Clone, Debug)]
pub struct Tokens {
    pub id: &'static str,
    pub name: String,
    /// The family both appearances share (`catppuccin` for `catppuccin-light`).
    pub family: String,
    pub dark: bool,
    pub colors: Colors,
    pub shadows: Shadows,
    pub numbers: Numbers,
    pub easings: Easings,
    pub terminal: Terminal,
    pub syntax: Syntax,
}

/// The theme id for a family in one appearance, as `getEffectiveThemeId` builds it.
pub fn theme_id(family: &str, dark: bool) -> String {
    let family = family.strip_suffix("-light").unwrap_or(family);
    if dark {
        family.to_owned()
    } else {
        format!("{family}-light")
    }
}

/// The tokens of `family` in one appearance. An unknown family falls back to
/// [`DEFAULT_FAMILY`], as `getColorThemeDefinition` does.
pub fn tokens(family: &str, dark: bool) -> &'static Tokens {
    by_id(&theme_id(family, dark)).unwrap_or_else(|| {
        log::warn!("theme: unknown family {family:?}, using {DEFAULT_FAMILY:?}");
        by_id(&theme_id(DEFAULT_FAMILY, dark)).expect("the default theme is bundled")
    })
}

/// The tokens of the bundled theme `id` (`"nord-light"`), or None.
pub fn by_id(id: &str) -> Option<&'static Tokens> {
    all().iter().find(|t| t.id == id)
}

/// Every bundled theme, in the order the web client lists them.
pub fn all() -> &'static [Tokens] {
    static ALL: OnceLock<Vec<Tokens>> = OnceLock::new();
    ALL.get_or_init(|| {
        THEMES
            .iter()
            .map(|&(id, json)| Tokens::parse(id, json))
            .collect()
    })
}

impl Tokens {
    fn parse(id: &'static str, json: &str) -> Tokens {
        let doc: Value = serde_json::from_str(json).unwrap_or_else(|e| panic!("{id}.json: {e}"));
        let dark = doc["appearance"] == "dark";
        let theme = Theme::from_value(&doc, dark).unwrap_or_else(|e| panic!("{id}.json: {e}"));
        let src = Source {
            id,
            doc: &doc,
            theme: &theme,
        };
        Tokens {
            id,
            name: src.text("name"),
            family: src.text("family"),
            dark,
            colors: Colors::read(&src),
            shadows: Shadows::read(&src),
            numbers: Numbers::read(&src),
            easings: Easings::read(&src),
            terminal: Terminal::read(&src),
            syntax: Syntax::read(&src),
        }
    }
}

/// One theme file while it is read into [`Tokens`]. A missing token panics: the generated
/// structs and the generated files come from the same run.
pub(crate) struct Source<'a> {
    id: &'static str,
    doc: &'a Value,
    theme: &'a Theme,
}

impl Source<'_> {
    fn text(&self, key: &str) -> String {
        self.doc[key]
            .as_str()
            .unwrap_or_else(|| self.missing(key))
            .to_owned()
    }

    pub(crate) fn color(&self, name: &str) -> Color {
        self.theme
            .authored(name)
            .unwrap_or_else(|| self.missing(name))
    }

    /// Layers in CSS order, the first on top.
    pub(crate) fn shadow(&self, name: &str) -> Vec<Shadow> {
        let src = self.doc["shadows"][name]
            .as_str()
            .unwrap_or_else(|| self.missing(name));
        rune_kit::css::box_shadows(src, |c| c)
    }

    pub(crate) fn number(&self, name: &str) -> f32 {
        self.theme.num(name)
    }

    pub(crate) fn easing(&self, name: &str) -> [f32; 4] {
        let points = self.doc["easings"][name]
            .as_array()
            .unwrap_or_else(|| self.missing(name));
        let at = |i: usize| {
            points
                .get(i)
                .and_then(Value::as_f64)
                .unwrap_or_else(|| self.missing(name)) as f32
        };
        [at(0), at(1), at(2), at(3)]
    }

    pub(crate) fn section_color(&self, section: &str, name: &str) -> Color {
        self.doc[section][name]
            .as_str()
            .and_then(rune::theme::hex)
            .unwrap_or_else(|| self.missing(name))
    }

    fn missing(&self, name: &str) -> ! {
        panic!(
            "{}.json has no {name:?}; run bun scripts/rune/gen-themes.ts",
            self.id
        )
    }
}
