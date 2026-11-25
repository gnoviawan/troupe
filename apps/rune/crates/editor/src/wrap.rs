//! Soft wrap of one line at a column width, as the React editor's `EditorView.lineWrapping`
//! (`white-space: pre-wrap`, `overflow-wrap: anywhere`) breaks it in a monospace face: after
//! the last space that fits, else inside the word; spaces at a row's end hang past the edge.
//! Every character is one column, which holds for Paper Mono's Latin text; wide characters
//! wrap late (a known gap).

/// The char offsets where each visual row of `line` starts; the first is always 0. `columns`
/// is at least 1.
pub fn rows(line: impl IntoIterator<Item = char>, columns: usize) -> Vec<usize> {
    let columns = columns.max(1);
    let mut starts = vec![0];
    let mut row_start = 0;
    // The offset after the last space run in this row: a break there keeps the word whole.
    let mut opportunity: Option<usize> = None;
    let mut prev_space = false;
    for (i, c) in line.into_iter().enumerate() {
        let space = c == ' ' || c == '\t';
        if !space && prev_space {
            opportunity = Some(i);
        }
        prev_space = space;
        if space || i - row_start < columns {
            continue;
        }
        let at = opportunity.filter(|o| *o > row_start).unwrap_or(i);
        starts.push(at);
        row_start = at;
        opportunity = None;
    }
    starts
}
