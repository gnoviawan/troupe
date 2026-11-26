//! `DiffStatCounts` (`components/ui/diff-stat-counts.tsx`): `+N` in success ink and `−M`
//! (U+2212) in destructive ink, one space apart, as one run of text. The caller owns the font.

use rune::{El, Span, TextStyle, rich};
use theme::Tokens;

/// The text and its two coloured ranges; None when `omit_zero` leaves nothing to show.
pub fn diff_stat_counts(
    t: &Tokens,
    added: u64,
    removed: u64,
    omit_zero: bool,
    font: TextStyle,
) -> Option<El> {
    let show_added = !omit_zero || added > 0;
    let show_removed = !omit_zero || removed > 0;
    let mut s = String::new();
    let mut spans = Vec::new();
    if show_added {
        let start = s.len();
        s.push_str(&format!("+{added}"));
        spans.push(Span::new(start, s.len()).color(t.colors.success));
    }
    if show_removed {
        if show_added {
            s.push(' ');
        }
        let start = s.len();
        s.push_str(&format!("\u{2212}{removed}"));
        spans.push(Span::new(start, s.len()).color(t.colors.destructive));
    }
    (!s.is_empty()).then(|| rich(s, spans).font(font))
}
