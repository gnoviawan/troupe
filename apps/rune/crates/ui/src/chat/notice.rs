//! `ChatErrorNotice`: a failed turn docked at the top of the composer card (`ComposerDock`
//! with the destructive tone): the red dot, "The last turn did not finish." and the error,
//! Details for the code and data, Retry, and Dismiss. `formatAcpAgentError` rewrites rate
//! limits and dropped streams into short advice.

use rune::{El, col, el, row, text};
use serde_json::Value;
use theme::Tokens;
use theme::icons as i;

use crate::type_::{MICRO, mono, sans};
use crate::{Button, ButtonSize, ButtonVariant};

pub const RATE_LIMIT_MESSAGE: &str = "Connection was rate-limited by the agent service. Wait a moment, then retry.";
pub const STREAM_INTERRUPT_MESSAGE: &str = "Agent connection was interrupted. Wait a moment, then retry.";

/// `\b` around `word` in `hay`.
fn has_word(hay: &str, word: &str) -> bool {
    let boundary = |c: Option<char>| c.is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
    hay.match_indices(word).any(|(at, _)| boundary(hay[..at].chars().next_back()) && boundary(hay[at + word.len()..].chars().next()))
}

/// `[\s_-]?` between `a` and `b`.
fn joined(hay: &str, a: &str, b: &str) -> bool {
    ["", " ", "_", "-", "\t", "\n"].iter().any(|sep| hay.contains(&format!("{a}{sep}{b}")))
}

fn rate_limited(m: &str) -> bool {
    m.contains("enhance_your_calm") || (joined(m, "enhance", "your") && (joined(m, "your", "calm"))) || joined(m, "rate", "limit") || m.contains("too many requests") || has_word(m, "429")
}

fn interrupted(m: &str) -> bool {
    let after = |a: &str, words: &[&str]| words.iter().any(|w| m.contains(&format!("{a} {w}")));
    ["retriableerror", "nghttp2_", "destroyed", "broken pipe", "econnreset", "econnrefused", "disconnected"].iter().any(|n| m.contains(n))
        || m.match_indices("pipe").any(|(at, _)| m[at + 4..].chars().next().is_none_or(|c| !(c.is_alphanumeric() || c == '_')))
        || after("stream", &["closed", "ended", "destroyed", "error"])
        || after("connection", &["closed", "lost", "reset", "refused", "aborted"])
        || m.find("connection").is_some_and(|at| m[at..].contains("timed out"))
        || after("agent thread", &["is no longer running", "dropped"])
}

/// `formatAcpAgentError`.
pub fn format_agent_error(raw: &str) -> String {
    let lower = raw.to_lowercase();
    if rate_limited(&lower) {
        RATE_LIMIT_MESSAGE.to_owned()
    } else if interrupted(&lower) {
        STREAM_INTERRUPT_MESSAGE.to_owned()
    } else {
        raw.to_owned()
    }
}

/// What the notice says and offers.
pub struct Spec<'a> {
    pub key: String,
    pub title: &'a str,
    pub message: String,
    pub code: Option<f64>,
    pub data: Option<&'a Value>,
    pub details_open: bool,
    /// Retry's label, when a retry is offered.
    pub retry: Option<&'a str>,
}

/// How the notice answers.
pub struct Actions<S> {
    pub details: Box<dyn Fn(&mut S)>,
    pub retry: Box<dyn Fn(&mut S)>,
    pub dismiss: Box<dyn Fn(&mut S)>,
}

/// `ChatErrorNotice` in its composer dock.
pub fn error_notice<S: 'static>(t: &'static Tokens, spec: Spec, actions: Actions<S>) -> El {
    let c = &t.colors;
    let has_details = spec.code.is_some() || spec.data.is_some();
    let words = format!("{} {}", spec.title, spec.message);
    let lead = spec.title.len();
    let message = row()
        .grow(1.0)
        .min_w(0.0)
        .gap(10.0)
        .items_start()
        .child(row().h(20.0).items_center().child(el().size(7.0, 7.0).radius(4.0).fill(c.destructive)).a11y_hidden())
        .child(
            text(words)
                .font(sans(12.0, 400.0, 20.0))
                .color(c.muted_foreground)
                .spans(vec![rune::Span::new(0, lead).color(c.foreground).weight(500.0)])
                .wrap()
                .min_w(0.0)
                .grow(1.0),
        );
    let Actions { details, retry, dismiss } = actions;
    let mut buttons = row().gap(6.0).items_center();
    if has_details {
        buttons = buttons.child(
            Button::new(if spec.details_open { "Hide details" } else { "Details" })
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Xs)
                .ink(c.muted_foreground)
                .build(t)
                .key(format!("{}-details", spec.key))
                .h(28.0)
                .px(8.0)
                .on_press(move |s: &mut S| details(s)),
        );
    }
    if let Some(label) = spec.retry {
        buttons = buttons.child(Button::new(label).size(ButtonSize::Xs).build(t).key(format!("{}-retry", spec.key)).px(12.0).on_press(move |s: &mut S| retry(s)));
    }
    buttons = buttons.child(
        Button::icon_only(&i::X, "Dismiss error", ButtonSize::IconXs)
            .variant(ButtonVariant::Ghost)
            .ink(c.muted_foreground)
            .build(t)
            .key(format!("{}-dismiss", spec.key))
            .on_press(move |s: &mut S| dismiss(s)),
    );
    let mut dock = col()
        .key(spec.key.clone())
        .role("alert")
        .w_full()
        .gap(10.0)
        .py(10.0)
        .pl(16.0)
        .pr(10.0)
        .border_sides([0.0, 0.0, 0.5, 0.0], c.border)
        .child(row().w_full().gap(10.0).items_center().flex_wrap().child(message).child(buttons));
    if has_details && spec.details_open {
        let mut well = col().ml(17.0).gap(4.0).px(12.0).py(10.0).radius(8.0).fill(c.background);
        let font = mono(MICRO, 400.0, 18.0);
        if let Some(code) = spec.code {
            well = well.child(text(format!("Code {code}")).font(font).color(c.muted_foreground));
        }
        if let Some(data) = spec.data {
            let pretty = serde_json::to_string_pretty(data).unwrap_or_default();
            well = well.child(col().max_h(160.0).scroll(rune::ScrollSpec { stick_bottom: false }).child(text(pretty).font(font).color(c.muted_foreground).wrap_anywhere()));
        }
        dock = dock.child(well);
    }
    dock
}
