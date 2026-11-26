//! Every token of one bundled theme as a swatch, on the theme's own background: the colour tokens,
//! then the terminal and syntax colours. One scene per theme id.
//!
//! `cargo run -p theme --example theme-swatches -- --scene nord-light --snap nord-light.png`

#![forbid(unsafe_code)]

use rune::{AppSpec, Color, Cx, El, TextStyle, col, el, row, text};
use theme::{Colors, Syntax, Terminal, Tokens};

const COLUMNS: usize = 6;

struct State {
    theme: &'static str,
}

fn label(s: &str, size: f32, color: Color) -> El {
    text(s.to_owned())
        .font(
            TextStyle::new(size, 400.0)
                .lh(size + 4.0)
                .features(theme::INTER_FEATURES),
        )
        .color(color)
}

fn swatch(t: &Tokens, name: &str, color: Color) -> El {
    row()
        .w(206.0)
        .gap(8.0)
        .child(
            el().w(28.0)
                .h(28.0)
                .radius(6.0)
                .fill(color)
                .border(1.0, t.colors.border),
        )
        .child(
            col()
                .child(label(name, 11.0, t.colors.foreground))
                .child(label(&color.hex(), 11.0, t.colors.muted_foreground)),
        )
}

fn grid(t: &Tokens, title: &str, cells: Vec<El>) -> El {
    let mut rows = col()
        .gap(6.0)
        .child(label(title, 13.0, t.colors.secondary_foreground));
    let mut cells = cells.into_iter().peekable();
    while cells.peek().is_some() {
        rows = rows.child(row().gap(4.0).children(cells.by_ref().take(COLUMNS)));
    }
    rows
}

fn view(s: &State, _cx: &mut Cx<State>) -> El {
    let t = theme::by_id(s.theme).expect("bundled theme");
    let colors = Colors::NAMES
        .iter()
        .map(|n| swatch(t, n, *t.colors.get(n).expect("listed")))
        .collect();
    let terminal = Terminal::NAMES
        .iter()
        .map(|n| swatch(t, n, *t.terminal.get(n).expect("listed")))
        .collect();
    let syntax = Syntax::NAMES
        .iter()
        .map(|n| swatch(t, n, *t.syntax.get(n).expect("listed")))
        .collect();
    col()
        .w_full()
        .h_full()
        .padding(16.0)
        .gap(14.0)
        .fill(t.colors.background)
        .child(label(
            &format!("{} ({})", t.name, t.id),
            16.0,
            t.colors.foreground,
        ))
        .child(grid(t, "Colours", colors))
        .child(grid(t, "Terminal", terminal))
        .child(grid(t, "Syntax", syntax))
}

macro_rules! scenes {
    ($($id:literal),* $(,)?) => { &[$(($id, |s: &mut State| s.theme = $id)),*] };
}

fn main() {
    let spec = AppSpec {
        fonts: theme::FONTS,
        families: theme::FAMILIES,
        scenes: scenes![
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
        ],
        ..AppSpec::new(
            "Theme swatches",
            (1280.0, 1080.0),
            || State {
                theme: theme::DEFAULT_FAMILY,
            },
            view,
        )
    };
    rune::App::new(spec).run();
}
