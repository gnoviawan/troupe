//! Troupe's chat Markdown (`components/chat/chat-agent-prose.tsx` and its `chat-markdown-*`
//! plugins) on rune-kit's streaming [`Markdown`]: prose in the chat's type, code fences in a
//! framed card with Download and Copy, GFM tables in a framed card with Copy, Download and a
//! full-window view, images from the app's cache, bare file paths that open the file, and
//! `termul-plan` fences hidden.
//!
//! A reply renders as segments: runs of prose and runs of table lines, each its own
//! `Markdown` keyed by its index. A stream only grows the last segment or adds one after it,
//! so earlier segments keep their keys, and a byte-at-a-time stream ends on the same element
//! tree as the whole reply drawn at once (`stream_tests.rs`).

mod fences;
mod highlight;
mod links;
mod style;
mod table;

#[cfg(test)]
mod fences_tests;
#[cfg(test)]
mod links_tests;
#[cfg(test)]
mod stream_tests;

use std::rc::Rc;

use rune::{El, Image, col, el, row, text};
use rune_kit::Markdown;
use theme::Tokens;
use theme::icons as i;

pub use fences::{PLAN_LANGUAGE, agent_prose, normalize_plan_fence_boundary, strip_empty_fences, strip_plan_fences};
pub use highlight::{highlighter, recut};
pub use links::{FILE_SCHEME, LinkTarget, classify, find_paths, link_file_paths};
pub use style::{markdown_style, syntax_colour, syntax_theme};
pub use table::{TABLE_ZOOM, table_zoom};

use crate::type_::mono;
use crate::{IconActionButton, IconActionSize, icon_action_group};

/// A press on a link: a file to open, a URL or a fragment.
pub type OnLink<S> = Rc<dyn Fn(&mut S, LinkTarget, &mut rune::Tx)>;
/// A request for a table's full-window view, with the table's Markdown.
pub type OnTableZoom<S> = Rc<dyn Fn(&mut S, String)>;
/// The app's picture for an image source, if it has one.
pub type Images = Rc<dyn Fn(&str) -> Option<Image>>;

/// What a reply's Markdown hands back to the app.
pub struct Hooks<S> {
    pub on_link: OnLink<S>,
    pub on_table_zoom: OnTableZoom<S>,
    pub images: Images,
}

impl<S> Clone for Hooks<S> {
    fn clone(&self) -> Self {
        Hooks {
            on_link: self.on_link.clone(),
            on_table_zoom: self.on_table_zoom.clone(),
            images: self.images.clone(),
        }
    }
}

impl<S> Default for Hooks<S> {
    fn default() -> Self {
        Hooks {
            on_link: Rc::new(|_, _, _| {}),
            on_table_zoom: Rc::new(|_, _| {}),
            images: Rc::new(|_| None),
        }
    }
}

/// One run of a reply: prose, or the lines of one GFM table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Segment {
    Prose(String),
    Table(String),
}

/// Splits a reply into prose and table runs. A line inside a fence is prose; a line that
/// starts with `|` outside one is a table line. Blank runs are dropped.
pub fn segments(src: &str) -> Vec<Segment> {
    let mut out: Vec<Segment> = Vec::new();
    let mut fence: Option<(u8, usize)> = None;
    for line in src.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let marker = fences::fence_marker(trimmed);
        fence = match (fence, marker) {
            (None, Some(open)) => Some(open),
            (Some((ch, n)), Some((mch, m))) if ch == mch && m >= n && trimmed[m..].trim().is_empty() => None,
            (open, _) => open,
        };
        let table = fence.is_none() && marker.is_none() && trimmed.starts_with('|');
        match (out.last_mut(), table) {
            (Some(Segment::Table(buf)), true) | (Some(Segment::Prose(buf)), false) => buf.push_str(line),
            (_, true) => out.push(Segment::Table(line.to_owned())),
            (_, false) => out.push(Segment::Prose(line.to_owned())),
        }
    }
    out.retain(|s| match s {
        Segment::Prose(t) | Segment::Table(t) => !t.trim().is_empty(),
    });
    out
}

/// An agent reply's prose: `raw` is the message text as the store holds it (plan fences and
/// all); `streaming` while the reply is still arriving. `key` names this reply's root.
pub fn prose<S: 'static>(t: &'static Tokens, key: &str, raw: &str, streaming: bool, hooks: &Hooks<S>) -> El {
    let src = link_file_paths(&agent_prose(raw, streaming));
    let parts = segments(&src);
    let last = parts.len().saturating_sub(1);
    let blocks = parts.into_iter().enumerate().map(|(n, part)| {
        let live = streaming && n == last;
        match part {
            Segment::Prose(src) => prose_run(t, src, live, hooks).key(format!("seg{n}")),
            Segment::Table(src) => table::framed(t, &src, live, hooks).key(format!("seg{n}")),
        }
    });
    col().key(key.to_owned()).w_full().min_w(0.0).children(blocks)
}

/// The rune-kit `Markdown` for one prose run, with Troupe's hooks.
pub(crate) fn markdown<S: 'static>(_t: &'static Tokens, src: String, streaming: bool, hooks: &Hooks<S>) -> Markdown {
    let link = hooks.on_link.clone();
    let images = hooks.images.clone();
    Markdown::new(src)
        .streaming(streaming)
        .on_link_tx(move |s: &mut S, url: &str, tx: &mut rune::Tx| link(s, classify(url), tx))
        .images(move |src| images(src))
        .highlighter(highlighter())
}

fn prose_run<S: 'static>(t: &'static Tokens, src: String, streaming: bool, hooks: &Hooks<S>) -> El {
    markdown(t, src, streaming, hooks)
        .code_frame(move |card, frame| {
            if frame.language == Some(PLAN_LANGUAGE) {
                return el().a11y_hidden();
            }
            code_card::<S>(t, card, frame.language.unwrap_or(""), frame.source.to_owned(), frame.open)
        })
        .diagram_frame(move |card, frame| code_card::<S>(t, card, "mermaid", frame.source.to_owned(), frame.open))
        .render(&markdown_style(t))
}

/// A press that copies `text` to the clipboard.
pub(crate) fn copies<S: 'static>(text: String) -> impl Fn(&mut S, &mut rune::Tx) + 'static {
    move |_: &mut S, tx: &mut rune::Tx| tx.copy(text.clone())
}

/// Streamdown's `CodeBlock`: a rounded card with the language and the Download and Copy
/// buttons over the code (or a diagram), which sits in its own inset panel.
fn code_card<S: 'static>(
    t: &'static Tokens,
    code: El,
    language: &str,
    source: String,
    open: bool,
) -> El {
    let c = &t.colors;
    let buttons = vec![
        IconActionButton::new(&i::DOWNLOAD, "Download")
            .size(IconActionSize::Sm)
            .disabled(open)
            .build(t),
        IconActionButton::new(&i::COPY, "Copy")
            .size(IconActionSize::Sm)
            .disabled(open)
            .build(t)
            .on_press_tx(copies::<S>(source)),
    ];
    col()
        .w_full()
        .my(16.0)
        .radius(12.0)
        .fill(c.card)
        .border(1.0, c.border)
        .padding(8.0)
        .child(
            row()
                .h(32.0)
                .pl(4.0)
                .items_center()
                .child(
                    text(if language.is_empty() { "text".to_owned() } else { language.to_owned() })
                        .font(mono(12.0, 400.0, 16.0))
                        .color(c.muted_foreground)
                        .grow(1.0),
                )
                .child(icon_action_group(t, false, buttons)),
        )
        .child(el().h(8.0))
        .child(col().w_full().radius(6.0).fill(c.background).border(1.0, c.border).clip_rounded(6.0).child(code))
}
