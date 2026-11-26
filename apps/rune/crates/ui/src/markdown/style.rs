//! The chat's Markdown look: Streamdown's prose classes in the chat column, measured from the
//! web client's computed styles, and code colours from the theme's syntax map.

use rune::Color;
use rune_kit::MarkdownStyle;
use rune_kit::syntax::{Syntax, SyntaxTheme};
use theme::Tokens;

use crate::type_::{mono, sans};

/// The colour the web client's Shiki theme (`lib/themes/syntax-shiki.ts`) gives the TextMate
/// scopes each rune-highlights token kind stands for. The record of this choice is in
/// `docs/rune/decisions.md` ("Code highlighting").
pub fn syntax_colour(t: &Tokens, kind: Syntax) -> Color {
    let s = &t.syntax;
    match kind {
        Syntax::Keyword => s.keyword,
        Syntax::String | Syntax::Regex => s.string,
        Syntax::Number => s.number,
        Syntax::Comment => s.comment,
        Syntax::Type => s.type_,
        Syntax::Function => s.function,
        Syntax::Property | Syntax::Parameter => s.property,
        Syntax::Punctuation => s.punctuation,
        Syntax::Operator => s.operator,
        Syntax::Variable => s.variable,
        Syntax::Attribute => s.attribute_name,
        Syntax::Heading => s.heading,
        Syntax::Link => s.link,
        Syntax::Inserted => t.colors.diff_added_foreground,
        Syntax::Deleted => t.colors.destructive,
        // A kind added upstream after this rev draws in the text colour until it is mapped.
        _ => t.colors.foreground,
    }
}

const KINDS: [Syntax; 17] = [
    Syntax::Keyword,
    Syntax::String,
    Syntax::Number,
    Syntax::Comment,
    Syntax::Type,
    Syntax::Function,
    Syntax::Property,
    Syntax::Punctuation,
    Syntax::Inserted,
    Syntax::Deleted,
    Syntax::Parameter,
    Syntax::Heading,
    Syntax::Link,
    Syntax::Operator,
    Syntax::Regex,
    Syntax::Attribute,
    Syntax::Variable,
];

pub fn syntax_theme(t: &Tokens) -> SyntaxTheme {
    let mut theme = SyntaxTheme::neutral(true);
    for kind in KINDS {
        theme = theme.with(kind, syntax_colour(t, kind));
    }
    theme
}

/// The chat prose style for `t`.
pub fn markdown_style(t: &Tokens) -> MarkdownStyle {
    let c = &t.colors;
    MarkdownStyle::neutral(true)
        .text(c.foreground)
        .muted(c.muted_foreground)
        .link(c.primary)
        .body_font(sans(14.0, 400.0, 22.75))
        .heading_font(1, sans(24.0, 600.0, 32.0))
        .heading_font(2, sans(20.0, 600.0, 28.0))
        .heading_font(3, sans(18.0, 600.0, 28.0))
        .strong_weight(600.0)
        .inline_code_bg(c.muted)
        .inline_code_font(0.88, 400.0)
        .inline_code_pad(3.7)
        .code_block(Color::TRANSPARENT, Color::TRANSPARENT)
        .code_block_padding(16.0, 16.0)
        .code_block_font(mono(14.0, 400.0, 20.0))
        .code_block_text(c.foreground)
        .code_margin(0.0, 0.0)
        .syntax_theme(syntax_theme(t))
        .quote(c.border, 4.0, 16.0, c.muted_foreground)
        .rule(c.border)
        .table(c.border, Color::TRANSPARENT)
        .table_outline(None)
        .table_rules(c.border)
        .table_header_text(c.foreground)
        .table_header_weight(600.0)
        .table_cell_padding(8.0, 5.3)
        .table_header_pad(8.0, 4.8)
        .table_flush_left(false)
        .table_full_width(true)
        .table_tabular_figures(false)
        .table_margin(0.0, 0.0)
        .table_font(sans(14.0, 400.0, 20.0))
}
