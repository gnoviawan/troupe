//! An edit's diff as the transcript draws it: `diffLines`, `diffLineCounts` and `firstChange`
//! from `tool-call-format.ts` (trim the shared edges, exact LCS of the middle when it fits
//! `MAX_LCS_CELLS`, else chain anchors, then fold context to 3 lines around each change), and
//! the two views, `EditDiffBlock` (inline under an edit) and `DiffPreview` (inside a card).

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::rc::Rc;

use rune::{El, col, el, icon, row, text};
use rune_kit::syntax::{self, Highlight};
use theme::Tokens;
use theme::icons as i;

use crate::type_::{MICRO, mono, sans};

const MAX_LCS_CELLS: usize = 2_000_000;
const MAX_ANCHOR_DEPTH: usize = 24;
const CONTEXT_LINES: usize = 3;
/// `COLLAPSED_ROWS`: rows the folded block shows before its fade.
pub const COLLAPSED_ROWS: usize = 4;
/// `···`: the marker for folded context.
pub const FOLD: &str = "\u{b7}\u{b7}\u{b7}";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Added,
    Removed,
    Context,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub kind: Kind,
    pub text: String,
    pub old_line: Option<usize>,
    pub new_line: Option<usize>,
}

impl Line {
    fn new(kind: Kind, text: &str, old_line: Option<usize>, new_line: Option<usize>) -> Self {
        Line { kind, text: text.to_owned(), old_line, new_line }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Op {
    Keep,
    Remove,
    Insert,
}

struct EditOp<'a> {
    op: Op,
    text: &'a str,
    old: usize,
    new: usize,
}

fn split_lines(text: &str) -> Vec<&str> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut parts: Vec<&str> = text.split('\n').collect();
    if parts.last() == Some(&"") {
        parts.pop();
    }
    parts.into_iter().map(|l| l.strip_suffix('\r').unwrap_or(l)).collect()
}

fn dump<'a>(a: &[&'a str], b: &[&'a str], (a0, a1): (usize, usize), (b0, b1): (usize, usize), ops: &mut Vec<EditOp<'a>>) {
    for (i, line) in a.iter().enumerate().take(a1).skip(a0) {
        ops.push(EditOp { op: Op::Remove, text: line, old: i + 1, new: b0 });
    }
    for (j, line) in b.iter().enumerate().take(b1).skip(b0) {
        ops.push(EditOp { op: Op::Insert, text: line, old: a1, new: j + 1 });
    }
}

fn lcs<'a>(a: &[&'a str], b: &[&'a str], (a0, a1): (usize, usize), (b0, b1): (usize, usize), ops: &mut Vec<EditOp<'a>>) {
    let (m, n) = (a1 - a0, b1 - b0);
    let stride = n + 1;
    let mut table = vec![0u32; (m + 1) * stride];
    for i in 1..=m {
        for j in 1..=n {
            table[i * stride + j] = if a[a0 + i - 1] == b[b0 + j - 1] {
                table[(i - 1) * stride + j - 1] + 1
            } else {
                table[i * stride + j - 1].max(table[(i - 1) * stride + j])
            };
        }
    }
    let mut tail = Vec::new();
    let (mut i, mut j) = (m, n);
    while i > 0 || j > 0 {
        if i > 0 && j > 0 && a[a0 + i - 1] == b[b0 + j - 1] {
            tail.push(EditOp { op: Op::Keep, text: a[a0 + i - 1], old: a0 + i, new: b0 + j });
            i -= 1;
            j -= 1;
        } else if j > 0 && (i == 0 || table[i * stride + j - 1] >= table[(i - 1) * stride + j]) {
            tail.push(EditOp { op: Op::Insert, text: b[b0 + j - 1], old: a0 + i, new: b0 + j });
            j -= 1;
        } else {
            tail.push(EditOp { op: Op::Remove, text: a[a0 + i - 1], old: a0 + i, new: b0 + j });
            i -= 1;
        }
    }
    ops.extend(tail.into_iter().rev());
}

fn anchors<'a>(a: &[&'a str], b: &[&'a str], (a0, a1): (usize, usize), (b0, b1): (usize, usize), ops: &mut Vec<EditOp<'a>>, depth: usize) {
    let mut count_a: HashMap<&str, usize> = HashMap::new();
    for line in &a[a0..a1] {
        *count_a.entry(line).or_default() += 1;
    }
    let mut count_b: HashMap<&str, usize> = HashMap::new();
    for line in &b[b0..b1] {
        *count_b.entry(line).or_default() += 1;
    }
    let unique_b: HashMap<&str, usize> = (b0..b1)
        .filter(|&j| count_b.get(b[j]) == Some(&1) && count_a.get(b[j]) == Some(&1))
        .map(|j| (b[j], j))
        .collect();
    let mut chain = Vec::new();
    let mut last_b: Option<usize> = None;
    for (i, line) in a.iter().enumerate().take(a1).skip(a0) {
        if count_a.get(line) != Some(&1) {
            continue;
        }
        if let Some(&bi) = unique_b.get(line)
            && last_b.is_none_or(|l| bi > l)
        {
            chain.push((i, bi));
            last_b = Some(bi);
        }
    }
    if chain.is_empty() {
        dump(a, b, (a0, a1), (b0, b1), ops);
        return;
    }
    let (mut pa, mut pb) = (a0, b0);
    for (ai, bi) in chain {
        range(a, b, (pa, ai), (pb, bi), ops, depth + 1);
        ops.push(EditOp { op: Op::Keep, text: a[ai], old: ai + 1, new: bi + 1 });
        (pa, pb) = (ai + 1, bi + 1);
    }
    range(a, b, (pa, a1), (pb, b1), ops, depth + 1);
}

fn range<'a>(a: &[&'a str], b: &[&'a str], (mut a0, a1): (usize, usize), (mut b0, b1): (usize, usize), ops: &mut Vec<EditOp<'a>>, depth: usize) {
    let mut head = 0;
    while a0 + head < a1 && b0 + head < b1 && a[a0 + head] == b[b0 + head] {
        ops.push(EditOp { op: Op::Keep, text: a[a0 + head], old: a0 + head + 1, new: b0 + head + 1 });
        head += 1;
    }
    a0 += head;
    b0 += head;
    let mut tail = 0;
    while a1 - tail > a0 && b1 - tail > b0 && a[a1 - 1 - tail] == b[b1 - 1 - tail] {
        tail += 1;
    }
    let (a_end, b_end) = (a1 - tail, b1 - tail);
    if a_end == a0 || b_end == b0 {
        dump(a, b, (a0, a_end), (b0, b_end), ops);
    } else if (a_end - a0) * (b_end - b0) <= MAX_LCS_CELLS {
        lcs(a, b, (a0, a_end), (b0, b_end), ops);
    } else if depth < MAX_ANCHOR_DEPTH {
        anchors(a, b, (a0, a_end), (b0, b_end), ops, depth);
    } else {
        dump(a, b, (a0, a_end), (b0, b_end), ops);
    }
    for k in 0..tail {
        ops.push(EditOp { op: Op::Keep, text: a[a_end + k], old: a_end + k + 1, new: b_end + k + 1 });
    }
}

fn fold(ops: &[EditOp]) -> Vec<Line> {
    let mut keep = vec![false; ops.len()];
    for (k, op) in ops.iter().enumerate() {
        if op.op != Op::Keep {
            let hi = (k + CONTEXT_LINES).min(ops.len() - 1);
            keep[k.saturating_sub(CONTEXT_LINES)..=hi].iter_mut().for_each(|v| *v = true);
        }
    }
    let mut out = Vec::new();
    let mut last: Option<usize> = None;
    for (k, op) in ops.iter().enumerate().filter(|(k, _)| keep[*k]) {
        if last.is_some_and(|l| k > l + 1) {
            out.push(Line::new(Kind::Context, FOLD, None, None));
        }
        out.push(match op.op {
            Op::Keep => Line::new(Kind::Context, op.text, Some(op.old), Some(op.new)),
            Op::Remove => Line::new(Kind::Removed, op.text, Some(op.old), None),
            Op::Insert => Line::new(Kind::Added, op.text, None, Some(op.new)),
        });
        last = Some(k);
    }
    out
}

/// `diffLines`: a new file is all additions, an emptied one all removals, else the changed
/// lines with 3 lines of context and `···` between distant hunks.
pub fn lines(old_text: Option<&str>, new_text: &str) -> Vec<Line> {
    let new = split_lines(new_text);
    let Some(old_text) = old_text.filter(|t| !t.is_empty()) else {
        return new.iter().enumerate().map(|(n, l)| Line::new(Kind::Added, l, None, Some(n + 1))).collect();
    };
    let old = split_lines(old_text);
    if new_text.is_empty() {
        return old.iter().enumerate().map(|(n, l)| Line::new(Kind::Removed, l, Some(n + 1), None)).collect();
    }
    let mut ops = Vec::new();
    range(&old, &new, (0, old.len()), (0, new.len()), &mut ops, 0);
    fold(&ops)
}

/// Diffs and colours kept between frames: a card rebuilds on every streamed chunk, and its
/// texts rarely change. Each cache starts over when it reaches this many entries.
const CACHE_CAP: usize = 256;

thread_local! {
    static LINES: RefCell<HashMap<u64, Rc<Vec<Line>>>> = RefCell::new(HashMap::new());
    static COLOURS: RefCell<HashMap<u64, Rc<Colours>>> = RefCell::new(HashMap::new());
}

fn hash_of(parts: &[Option<&str>]) -> u64 {
    let mut h = DefaultHasher::new();
    parts.hash(&mut h);
    h.finish()
}

fn cached<T>(cache: &'static std::thread::LocalKey<RefCell<HashMap<u64, Rc<T>>>>, key: u64, make: impl FnOnce() -> T) -> Rc<T> {
    if let Some(hit) = cache.with_borrow(|c| c.get(&key).cloned()) {
        return hit;
    }
    let made = Rc::new(make());
    cache.with_borrow_mut(|c| {
        if c.len() >= CACHE_CAP {
            c.clear();
        }
        c.insert(key, made.clone());
    });
    made
}

/// [`lines`] for a pair of texts, computed once per pair.
pub fn lines_of(old_text: Option<&str>, new_text: &str) -> Rc<Vec<Line>> {
    cached(&LINES, hash_of(&[old_text, Some(new_text)]), || lines(old_text, new_text))
}

/// Added and removed rows among `lines`.
pub fn counts(lines: &[Line]) -> (usize, usize) {
    lines.iter().fold((0, 0), |(a, r), l| match l.kind {
        Kind::Added => (a + 1, r),
        Kind::Removed => (a, r + 1),
        Kind::Context => (a, r),
    })
}

/// `firstChange`: the first changed row's index and the 1-based new-file line it sits at.
pub fn first_change(lines: &[Line]) -> Option<(usize, usize)> {
    let mut last_new = 0;
    for (index, l) in lines.iter().enumerate() {
        match l.kind {
            Kind::Added => return Some((index, l.new_line.unwrap_or(last_new + 1))),
            Kind::Removed => return Some((index, last_new + 1)),
            Kind::Context => last_new = l.new_line.unwrap_or(last_new),
        }
    }
    None
}

/// `baseName`.
pub fn base_name(path: &str) -> &str {
    let trimmed = path.trim_end_matches(['/', '\\']);
    trimmed.rsplit(['/', '\\']).next().unwrap_or(trimmed)
}

/// Whole-file colours for one side of the diff, cut into its lines (`useDiffHighlight`).
fn painted(t: &Tokens, lang: Option<&str>, code: &str) -> Vec<Vec<rune::Span>> {
    let highlights: Vec<Highlight> = crate::markdown::recut(code, syntax::builtin(lang, code));
    let theme = crate::markdown::syntax_theme(t);
    let mut out = Vec::new();
    let (mut start, mut k) = (0, 0);
    for line in code.split('\n') {
        let end = start + line.len();
        while k < highlights.len() && highlights[k].0.end <= start {
            k += 1;
        }
        let mut spans = Vec::new();
        for (r, p) in highlights[k..].iter().take_while(|(r, _)| r.start < end) {
            let (s, e) = (r.start.max(start), r.end.min(end));
            if e > s {
                spans.extend(theme.spans(&[(s - start..e - start, *p)], 0));
            }
        }
        out.push(spans);
        start = end + 1;
    }
    out
}

/// Both sides' colours: removed rows read the old file, the rest the new one.
pub struct Colours {
    old: Vec<Vec<rune::Span>>,
    new: Vec<Vec<rune::Span>>,
}

impl Colours {
    /// Both sides' colours, highlighted once per theme, path and texts.
    pub fn of_texts(t: &'static Tokens, path: &str, old_text: Option<&str>, new_text: &str) -> Rc<Self> {
        let theme = format!("{:p}", t);
        let key = hash_of(&[Some(&theme), Some(path), old_text, Some(new_text)]);
        cached(&COLOURS, key, || Colours::new(t, path, old_text, new_text))
    }

    pub fn new(t: &Tokens, path: &str, old_text: Option<&str>, new_text: &str) -> Self {
        let lang = syntax::lang_for_path(path);
        Colours { old: painted(t, lang, old_text.unwrap_or("")), new: painted(t, lang, new_text) }
    }

    fn of(&self, line: &Line) -> Vec<rune::Span> {
        let (side, n) = match line.kind {
            Kind::Removed => (&self.old, line.old_line),
            _ => (&self.new, line.new_line.or(line.old_line)),
        };
        n.and_then(|n| side.get(n - 1)).cloned().unwrap_or_default()
    }
}

const ROW: rune::TextStyle = mono(12.0, 400.0, 19.5);

/// `DiffRows`: the gutter number, the sign, and the line in its colours, over the red or
/// green tint.
pub fn rows(t: &Tokens, lines: &[Line], colours: &Colours) -> Vec<El> {
    let c = &t.colors;
    lines
        .iter()
        .map(|line| {
            let fold = line.kind == Kind::Context && line.text == FOLD;
            let (fill, ink, sign) = match line.kind {
                Kind::Added => (Some(c.diff_added.alpha(0.1)), c.diff_added_foreground, "+"),
                Kind::Removed => (Some(c.destructive.alpha(0.1)), c.destructive, "\u{2212}"),
                Kind::Context if fold => (None, c.muted_foreground.alpha(0.5), " "),
                Kind::Context => (None, c.muted_foreground, " "),
            };
            let number = line.new_line.or(line.old_line).map(|n| n.to_string()).unwrap_or_default();
            let r = row()
                .w_full()
                .items_start()
                .child(
                    row()
                        .w(40.0)
                        .pr(8.0)
                        .justify_end()
                        .child(text(number).font(ROW).color(c.muted_foreground.alpha(0.5)))
                        .a11y_hidden(),
                )
                .child(row().w(16.0).child(text(sign).font(ROW).color(ink.alpha(0.6))).a11y_hidden())
                .child(text(line.text.clone()).font(ROW).color(ink).spans(colours.of(line)).wrap().min_w(0.0).grow(1.0));
            match fill {
                Some(fill) => r.fill(fill),
                None => r,
            }
        })
        .collect()
}

/// The `+N −M` pair, green then red; the red only when something was removed.
pub fn stat(t: &Tokens, added: usize, removed: usize, always_removed: bool, font: rune::TextStyle) -> El {
    let c = &t.colors;
    let mut r = row().child(text(format!("+{added}")).font(font).color(c.success));
    if removed > 0 || always_removed {
        r = r.child(text(format!(" \u{2212}{removed}")).font(font).color(c.destructive));
    }
    r
}

/// What an edit block shows and how it answers.
pub struct Block<'a> {
    pub key: String,
    pub path: &'a str,
    pub old_text: Option<&'a str>,
    pub new_text: &'a str,
    pub running: bool,
    pub failed: bool,
    pub expanded: bool,
}

/// `EditDiffBlock`: the file's header (a press opens it at the first change), then four rows
/// from just above the first change fading out; a press on them shows the whole diff.
pub fn edit_block<S: 'static>(
    t: &'static Tokens,
    b: Block,
    on_open: Option<impl Fn(&mut S, String, usize) + 'static>,
    on_expand: impl Fn(&mut S) + 'static,
) -> El {
    let c = &t.colors;
    let lines = lines_of(b.old_text, b.new_text);
    let (added, removed) = counts(&lines);
    let change = first_change(&lines);
    let name = base_name(b.path).to_owned();
    let start = change.map_or(0, |(index, _)| index.saturating_sub(1));
    let more_below = lines.len() > start + COLLAPSED_ROWS;
    let collapsed = (start > 0 || more_below) && !b.expanded;
    let visible = if collapsed { &lines[start..(start + COLLAPSED_ROWS).min(lines.len())] } else { &lines[..] };
    let open_line = change.map_or(1, |(_, line)| line);
    let name_ink = if b.failed { c.destructive } else { c.foreground };
    let mut header = row()
        .key(format!("{}-head", b.key))
        .w_full()
        .min_w(0.0)
        .gap(8.0)
        .px(10.0)
        .py(6.0)
        .items_center()
        .child(icon(theme::icons::file(&name, true), 14.0))
        .child(text(name.clone()).font(sans(12.0, 400.0, 16.0)).color(if b.running { c.muted_foreground } else { name_ink }).single_line().truncate().min_w(0.0))
        .child(stat(t, added, removed, false, sans(MICRO, 400.0, 18.0)));
    if b.failed {
        header = header.child(icon(&i::ALERT_CIRCLE, 12.0).color(c.destructive));
    }
    if let Some(open) = on_open {
        let path = b.path.to_owned();
        header = header
            .role("button")
            .label(format!("Open {name} at line {open_line}"))
            .focusable(true)
            .hover(c.muted.alpha(0.5))
            .on_press(move |s: &mut S| open(s, path.clone(), open_line));
    }
    let colours = Colours::of_texts(t, b.path, b.old_text, b.new_text);
    let mut block = col()
        .key(b.key.clone())
        .w_full()
        .my(4.0)
        .radius(6.0)
        .clip_rounded(6.0)
        .border(1.0, c.border.alpha(0.5))
        .fill(c.background.alpha(0.5))
        .child(header);
    if !lines.is_empty() {
        let body = col().w_full().padding(8.0).children(rows(t, visible, &colours));
        let body = if collapsed {
            // `mask-image: linear-gradient(to bottom, black 45%, transparent)`.
            let frame = if more_below { el().w_full().child(body).clip_fade_linear(0.0, 0.0, 0.0, 52.0) } else { el().w_full().child(body) };
            frame
                .key(format!("{}-rows", b.key))
                .role("button")
                .label(format!("Show full diff of {name}"))
                .focusable(true)
                .on_press(move |s: &mut S| on_expand(s))
        } else {
            col().key(format!("{}-rows", b.key)).w_full().max_h(192.0).scroll(rune::ScrollSpec { stick_bottom: false }).child(body)
        };
        block = block.child(el().w_full().h(1.0).fill(c.border.alpha(0.4))).child(body);
    }
    block
}

/// `DiffPreview`: the path, `new` for a created file and `+N −M`, over every changed row.
pub fn preview(t: &'static Tokens, path: &str, old_text: Option<&str>, new_text: &str) -> El {
    let c = &t.colors;
    let lines = lines_of(old_text, new_text);
    let (added, removed) = counts(&lines);
    let mut head = row()
        .w_full()
        .gap(8.0)
        .px(8.0)
        .py(4.0)
        .items_center()
        .border_sides([0.0, 0.0, 1.0, 0.0], c.border.alpha(0.4))
        .child(icon(&i::FILE_DIFF, 12.0).color(c.muted_foreground))
        .child(text(path.to_owned()).font(mono(MICRO, 400.0, 18.0)).color(c.foreground).single_line().truncate().min_w(0.0));
    if old_text.is_none() {
        head = head.child(row().px(4.0).radius(4.0).fill(c.success.alpha(0.15)).child(text("new").font(sans(MICRO, 400.0, 18.0)).color(c.success)));
    }
    head = head.child(el().grow(1.0)).child(stat(t, added, removed, true, mono(MICRO, 400.0, 18.0)));
    let colours = Colours::of_texts(t, path, old_text, new_text);
    col()
        .w_full()
        .radius(4.0)
        .clip_rounded(4.0)
        .border(1.0, c.border.alpha(0.5))
        .fill(c.background.alpha(0.5))
        .child(head)
        .child(col().w_full().max_h(192.0).scroll(rune::ScrollSpec { stick_bottom: false }).child(col().w_full().padding(8.0).children(rows(t, &lines, &colours))))
}
