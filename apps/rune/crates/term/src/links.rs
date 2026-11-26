//! Web links in output, as xterm.js's `WebLinksAddon` finds them: an `http` or `https` URL
//! within one wrapped run of rows, without the trailing punctuation that usually ends a
//! sentence.

use std::collections::VecDeque;
use std::sync::OnceLock;

use regex_lite::Regex;

use crate::Line;
use crate::search::Match;
use crate::text::{Logical, Point, logical_bounds};

/// `WebLinksAddon`'s `strictUrlRegex`.
const URL: &str = r#"(https?|HTTPS?)://[^\s"'!*(){}|\\\^<>`]*[^\s"':,.!?{}|\\\^~\[\]`()<>]"#;

fn url_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(URL).expect("the link pattern compiles"))
}

/// A link: the cells it covers and its URL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    pub at: Match,
    pub url: String,
}

/// The link under cell `at`, if one covers it.
pub fn link_at(lines: &VecDeque<Line>, at: Point) -> Option<Link> {
    if at.line >= lines.len() {
        return None;
    }
    let (first, last) = logical_bounds(lines, at.line);
    let logical = Logical::read(lines, first, last);
    url_regex().find_iter(&logical.text).find_map(|m| {
        let found = Match { start: logical.cell_at(m.start()), end: logical.cell_at(m.end()) };
        (found.start <= at && at < found.end).then(|| Link { at: found, url: m.as_str().to_owned() })
    })
}
