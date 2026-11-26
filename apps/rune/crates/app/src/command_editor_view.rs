//! The command editor's strip under a terminal's screen (`TerminalInputSurface`'s editor
//! block): the folder and git branch chips, the draft in the terminal's font with its caret,
//! selection and completion ghost, and the completion list above the caret.

use rune::{Cursor, El, Phase, col, el, icon, row, text};
use theme::Tokens;
use troupe_editor::{Command, Selection, line_len, wrap};
use ui::type_::sans;

use crate::State;
use crate::command_editor::field_key;

/// The editor's padding (`p-2`) and the block's top padding (`pt-2`).
const PAD: f32 = 8.0;
/// The most completion rows shown at once.
const ROWS_SHOWN: usize = 8;
/// Touch surfaces keep a 16px floor (`TOUCH_MIN_FONT_SIZE`).
const TOUCH_MIN_FONT_SIZE: f32 = 16.0;

/// CodeMirror's `.cm-content` padding above and below the lines.
const CONTENT_PAD: f32 = 4.0;

/// The editor's cell and font (`commandEditorFontSize`): the terminal's size, floored on touch,
/// at the font's own advance and `TERMINAL_LINE_HEIGHT`, as CodeMirror lays out text. xterm.js's
/// whole-pixel cell is the screen's, not the editor's.
fn metrics(s: &State, touch: bool) -> ui::terminal::Metrics {
    let size = s.terms.metrics().style.size;
    let size = if touch { size.max(TOUCH_MIN_FONT_SIZE) } else { size };
    *s.terms.editor.metrics[usize::from(touch)].get_or_init(|| {
        let probe = rune::TextStyle::new(size, 400.0).mono();
        let cell_w = rune::text::advance(&"W".repeat(64), probe) / 64.0;
        let cell_h = size * ui::terminal::LINE_HEIGHT;
        ui::terminal::Metrics { cell_w, cell_h, style: probe.lh(cell_h) }
    })
}

fn chip(t: &'static Tokens, glyph: &str, label: &str, what: &str, max_w: f32) -> El {
    let c = &t.colors;
    row()
        .role("note")
        .label(format!("{what}: {label}"))
        .items_center()
        .gap(4.0)
        .min_w(0.0)
        .max_w(max_w)
        .px(8.0)
        .py(4.0)
        .radius(6.0)
        .fill(c.muted.alpha(0.5))
        .border(1.0, c.border.alpha(0.6))
        .child(icon(theme::icons::named(glyph).unwrap_or(&theme::icons::X), 12.0).color(c.muted_foreground))
        .child(text(label.to_owned()).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground).truncate())
}

/// The rows each line wraps into at `columns` (0 is no wrap): (line, first char of the row).
fn layout(editor: &troupe_editor::Editor, columns: usize) -> Vec<(usize, usize, usize)> {
    let rope = editor.rope();
    let mut rows = Vec::new();
    for line in 0..rope.len_lines() {
        let len = line_len(rope, line);
        let start = rope.line_to_char(line);
        let starts = if columns > 0 { wrap::rows(rope.slice(start..start + len).chars(), columns) } else { vec![0] };
        for (i, s) in starts.iter().enumerate() {
            let end = starts.get(i + 1).copied().unwrap_or(len);
            rows.push((line, start + s, start + end));
        }
    }
    rows
}

/// The strip for terminal `id`, when its editor is mounted.
pub fn strip(s: &State, t: &'static Tokens, id: &str, touch: bool) -> Option<El> {
    let line = s.terms.editor.lines.get(id)?;
    let c = &t.colors;
    let m = metrics(s, touch);
    let (cell_w, cell_h) = (m.cell_w, m.cell_h);
    let editor = &line.editor;
    let sel = editor.selection();
    let columns = s.terms.editor.columns.get(id).copied().unwrap_or(0);
    let rows = layout(editor, columns);
    let caret_row = rows.iter().rposition(|(_, from, to)| *from <= sel.head && sel.head <= *to).unwrap_or(0);
    let rope = editor.rope();
    let focused = s.terms.focused.as_deref() == Some(id);
    let mut body = el().w_full().h(rows.len() as f32 * cell_h).min_w(0.0);
    for (r, (_, from, to)) in rows.iter().enumerate() {
        let y = r as f32 * cell_h;
        let range = sel.range();
        if !sel.is_empty() && range.start <= *to && range.end >= *from {
            let a = range.start.max(*from) - from;
            let b = range.end.min(*to) - from;
            let w = (b.saturating_sub(a)) as f32 * cell_w + if range.end > *to { cell_w } else { 0.0 };
            if w > 0.0 {
                body = body.child(el().absolute(a as f32 * cell_w, y).size(w, cell_h).fill(c.foreground.alpha(0.14)));
            }
        }
        let piece = rope.slice(*from..*to).to_string();
        if !piece.is_empty() {
            body = body.child(text(piece).font(m.style).color(c.foreground).absolute(0.0, y));
        }
    }
    let (_, row_from, _) = rows.get(caret_row).copied().unwrap_or((0, 0, 0));
    let caret_x = (sel.head - row_from) as f32 * cell_w;
    let caret_y = caret_row as f32 * cell_h;
    if let Some(ghost) = s.ghost(id) {
        body = body.child(text(ghost).font(m.style).color(c.muted_foreground).absolute(caret_x, caret_y));
    }
    if focused {
        body = body.child(el().absolute(caret_x, caret_y).size(2.0, cell_h).fill(c.foreground).ime_cursor());
        if let Some(preedit) = s.terms.editor.preedit.get(id) {
            body = body.child(
                text(preedit.clone())
                    .font(m.style)
                    .color(c.foreground)
                    .spans(vec![rune::Span { underline: Some(c.foreground), ..rune::Span::new(0, preedit.len()) }])
                    .fill(c.card)
                    .absolute(caret_x, caret_y),
            );
        }
    }
    let pressed = id.to_owned();
    let measured = id.to_owned();
    let rows_for_press = rows.clone();
    let body = body
        .cursor(Cursor::Text)
        .on_drag(move |s: &mut State, d, _tx| {
            let Some(line) = s.terms.editor.lines.get(&pressed) else { return };
            let r = ((d.y / cell_h).floor().max(0.0) as usize).min(rows_for_press.len().saturating_sub(1));
            let Some((_, from, to)) = rows_for_press.get(r).copied() else { return };
            let at = (from + (d.x / cell_w).round().max(0.0) as usize).min(to);
            let anchor = match d.phase {
                Phase::Down => at,
                Phase::Move => line.editor.selection().anchor,
                Phase::Up => return,
            };
            s.terms.focused = Some(pressed.clone());
            s.command_apply(&pressed, Command::Select(Selection { anchor, head: at }));
        })
        .on_layout(move |s: &mut State, [_, _, w, _]| {
            let columns = (w / cell_w).floor().max(1.0) as usize;
            if s.terms.editor.columns.get(&measured) != Some(&columns) {
                s.terms.editor.columns.insert(measured.clone(), columns);
                if let Some(line) = s.terms.editor.lines.get_mut(&measured) {
                    line.editor.set_wrap(columns);
                }
            }
        });
    let keyed = id.to_owned();
    let mut field = col()
        .key(field_key(id))
        .role("textbox")
        .label("Command editor".to_owned())
        .focusable(true)
        .no_focus_ring()
        .w_full()
        .px(PAD)
        .py(PAD + CONTENT_PAD)
        .on_key(move |s: &mut State, key, tx| s.command_editor_key(&keyed, key, tx))
        .child(body);
    // CodeMirror anchors the list at the completed word's start, not at the caret.
    let word_x = s.completion_from(id).map_or(caret_x, |from| caret_x - sel.head.saturating_sub(from) as f32 * cell_w).max(0.0);
    if let Some(list) = completion_list(s, t, m, id, word_x + PAD, caret_y + PAD + CONTENT_PAD) {
        field = field.child(list);
    }
    let mut chips = row().items_center().gap(6.0).px(PAD).min_w(0.0);
    let session = s.terms.sessions.get(id);
    if let Some(cwd) = session.map(|s| s.cwd.clone()).filter(|c| !c.is_empty()) {
        chips = chips.child(chip(t, "Folder", &cwd, "Working directory", 420.0));
    }
    if let Some(branch) = s.terms.editor.branches.get(id) {
        chips = chips.child(chip(t, "GitBranch", branch, "Git branch", 280.0));
    }
    Some(
        col()
            .key(format!("command-editor-block-{id}"))
            .w_full()
            .gap(4.0)
            .pt(PAD)
            .fill(c.card)
            .border_sides([1.0, 0.0, 0.0, 0.0], c.border)
            .child(chips)
            .child(field),
    )
}

/// CodeMirror's empty icon slot before each label (`.cm-completionIcon`: 0.8em and 0.6em of
/// padding at 14px), plus the row's own 8px.
const LABEL_INSET: f32 = 28.0;

/// The completion list over the caret (`autocompletion` with `aboveCursor`, `termulTooltipThemeSpec`):
/// each word with its typed part bold, the lit one tinted; a press accepts it.
fn completion_list(s: &State, t: &'static Tokens, m: ui::terminal::Metrics, id: &str, x: f32, caret_y: f32) -> Option<El> {
    let shown = s.terms.editor.shown.get(id)?;
    let c = &t.colors;
    let first = shown.lit.saturating_sub(ROWS_SHOWN - 1);
    let visible: Vec<(usize, &model::terminal::words::Row)> = shown.rows.iter().enumerate().skip(first).take(ROWS_SHOWN).collect();
    let row_h = m.cell_h + 8.0;
    let height = visible.len() as f32 * row_h + 10.0;
    let typed = s.completion_typed(id);
    let mut bold = m.style;
    bold.weight = 700.0;
    let mut list = col()
        .key(format!("command-completions-{id}"))
        .role("listbox")
        .label("Completions".to_owned())
        .absolute(x, caret_y - height - 2.0)
        .min_w(120.0)
        .px(4.0)
        .py(4.0)
        .radius(10.0)
        .fill(c.popover)
        .border(1.0, c.border);
    for (i, r) in visible {
        let lit = i == shown.lit;
        let pressed = id.to_owned();
        let color = if lit { c.foreground } else { c.popover_foreground };
        let (matched, rest) = match r.text.strip_prefix(typed.as_str()) {
            Some(rest) if !typed.is_empty() => (typed.as_str(), rest),
            _ => ("", r.text.as_str()),
        };
        let mut option = row()
            .key(format!("command-completion-{id}-{i}"))
            .role("option")
            .label(r.text.clone())
            .h(row_h)
            .pl(LABEL_INSET)
            .pr(8.0)
            .items_center()
            .radius(6.0)
            .fill(if lit { c.foreground.alpha(0.06) } else { c.popover });
        if !matched.is_empty() {
            option = option.child(text(matched.to_owned()).font(bold).color(color));
        }
        list = list.child(
            option
                .child(text(rest.to_owned()).font(m.style).color(color))
                .on_press(move |s: &mut State| s.accept_completion(&pressed, Some(i))),
        );
    }
    Some(list)
}
