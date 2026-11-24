//! `ToolCallCard`: one tool call in a turn's steps. An edit shows its diffs inline
//! (`EditDiffBlock`); every other call is a row (its kind's icon, the verb and what it acted on,
//! `+N −M` or a line range, the failure mark) that opens to the call's result: its text, a
//! terminal's output, or a diff it carries beside other content. `describeToolCall` and
//! `readableOutput` are ported from `tool-call-summary.ts`.

use rune::{El, col, el, icon, row, text};
use serde_json::Value;
use theme::Tokens;
use theme::icons as i;

use super::diff;
use crate::type_::{MICRO, mono, sans};
use crate::{IconActionButton, IconActionSize};

type Object = model::acp::types::Object;

const PATH_KEYS: [&str; 10] = ["path", "filePath", "file_path", "file", "target_file", "targetFile", "abspath", "absPath", "filename", "fileName"];
const COMMAND_KEYS: [&str; 4] = ["command", "cmd", "script", "commandLine"];
const QUERY_KEYS: [&str; 6] = ["query", "pattern", "q", "search", "searchTerm", "regex"];
const URL_KEYS: [&str; 4] = ["url", "uri", "href", "link"];
/// `READABLE_TEXT_KEYS`.
const TEXT_KEYS: [&str; 6] = ["output", "stdout", "result", "text", "content", "message"];
const FILE_OPEN_KINDS: [&str; 4] = ["read", "edit", "delete", "move"];

/// `firstString`: the first key holding a non-blank string, trimmed.
fn first_string<'a>(obj: Option<&'a Value>, keys: &[&str]) -> Option<&'a str> {
    let obj = obj?.as_object()?;
    keys.iter().filter_map(|k| obj.get(*k)?.as_str()).map(str::trim).find(|s| !s.is_empty())
}

fn first_number(obj: Option<&Value>, keys: &[&str]) -> Option<f64> {
    let obj = obj?.as_object()?;
    keys.iter().find_map(|k| obj.get(*k)?.as_f64())
}

fn kind(tool: &Object) -> &str {
    tool.get("kind").and_then(Value::as_str).unwrap_or("")
}

fn content(tool: &Object) -> &[Value] {
    tool.get("content").and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

fn is_diff(item: &Value) -> bool {
    item.get("type").and_then(Value::as_str) == Some("diff")
}

fn is_subagent(tool: &Object) -> bool {
    tool.get("subagent").is_some_and(|v| !v.is_null())
}

fn is_subagent_control(tool: &Object) -> bool {
    tool.get("subagentControl").is_some_and(|v| !v.is_null())
}

/// `isToolCallRunning`.
pub fn is_running(tool: &Object) -> bool {
    matches!(tool.get("status").and_then(Value::as_str), Some("pending" | "in_progress"))
}

/// `toolCallPath`: the first location, a path in the input, else the first diff's path.
pub fn path(tool: &Object) -> Option<&str> {
    let location = tool.get("locations").and_then(|l| l.get(0)).and_then(|l| l.get("path")).and_then(Value::as_str).filter(|p| !p.is_empty());
    location
        .or_else(|| first_string(tool.get("rawInput"), &PATH_KEYS))
        .or_else(|| content(tool).iter().filter(|c| is_diff(c)).find_map(|c| c.get("path")?.as_str().filter(|p| !p.is_empty())))
}

/// `describeToolCall`'s result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Summary {
    pub verb: String,
    pub primary: String,
    pub detail: Option<String>,
    pub diff_stat: Option<(usize, usize)>,
}

fn verb(kind: &str, running: bool) -> &'static str {
    let (now, done) = match kind {
        "read" => ("Reading", "Read"),
        "edit" => ("Editing", "Edited"),
        "delete" => ("Deleting", "Deleted"),
        "move" => ("Moving", "Moved"),
        "search" => ("Searching", "Searched"),
        "execute" => ("Running", "Ran"),
        "think" => ("Thinking", "Thinking"),
        "fetch" => ("Fetching", "Fetched"),
        "switch_mode" => ("Switching mode", "Switched mode"),
        _ => ("", ""),
    };
    if running { now } else { done }
}

/// "L<start>-<end>" from the common range keys.
fn line_range(input: Option<&Value>) -> Option<String> {
    let start = first_number(input, &["startLine", "start_line", "start", "line", "lineStart"]);
    let end = first_number(input, &["endLine", "end_line", "end", "lineEnd"]);
    if let (Some(s), Some(e)) = (start, end) {
        return Some(format!("L{s}-{e}"));
    }
    let offset = first_number(input, &["offset"])?;
    let limit = first_number(input, &["limit", "count", "lines"]).filter(|l| *l > 0.0)?;
    Some(format!("L{offset}-{}", offset + limit))
}

/// The summed `+N −M` of the call's diffs, else the host's durable `diffStat`.
fn diff_stat(tool: &Object) -> Option<(usize, usize)> {
    let diffs: Vec<&Value> = content(tool).iter().filter(|c| is_diff(c)).collect();
    if diffs.is_empty() {
        let stat = tool.get("diffStat")?;
        let n = |k: &str| stat.get(k)?.as_u64().map(|v| v as usize);
        return Some((n("added")?, n("removed")?));
    }
    Some(diffs.iter().fold((0, 0), |(a, r), d| {
        let lines = diff::lines(d.get("oldText").and_then(Value::as_str), d.get("newText").and_then(Value::as_str).unwrap_or(""));
        let (da, dr) = diff::counts(&lines);
        (a + da, r + dr)
    }))
}

/// `describeToolCall`: the verb, what the call acted on, and its trailing meta.
pub fn describe(tool: &Object) -> Summary {
    let input = tool.get("rawInput");
    let title = tool.get("title").and_then(Value::as_str).map(str::trim).filter(|t| !t.is_empty());
    let k = kind(tool);
    let v = verb(k, is_running(tool)).to_owned();
    let plain = |verb: String, primary: &str| Summary { verb, primary: primary.to_owned(), detail: None, diff_stat: None };
    if is_subagent(tool) {
        let name = tool.get("subagent").and_then(|s| s.get("title")).and_then(Value::as_str).unwrap_or("");
        return plain(String::new(), name);
    }
    if is_subagent_control(tool) {
        return plain(String::new(), title.unwrap_or("Agent command"));
    }
    match k {
        "read" | "delete" | "move" => {
            let primary = path(tool).map_or_else(|| title.unwrap_or("file").to_owned(), |p| diff::base_name(p).to_owned());
            let detail = if k == "read" { line_range(input) } else { None };
            Summary { verb: v, primary, detail, diff_stat: None }
        }
        "edit" => {
            let primary = path(tool).map_or_else(|| title.unwrap_or("file").to_owned(), |p| diff::base_name(p).to_owned());
            let stat = diff_stat(tool);
            let detail = stat.map(|(a, r)| if r > 0 { format!("+{a} \u{2212}{r}") } else { format!("+{a}") });
            Summary { verb: v, primary, detail, diff_stat: stat }
        }
        "execute" => plain(v, first_string(input, &COMMAND_KEYS).or(title).unwrap_or("command")),
        "search" => plain(v, first_string(input, &QUERY_KEYS).or(title).unwrap_or("")),
        "fetch" => plain(v, first_string(input, &URL_KEYS).or(title).unwrap_or("")),
        "think" => plain(v, first_string(input, &["thought", "text", "message"]).or(title).unwrap_or("Thinking")),
        _ => plain(v, title.unwrap_or(if k.is_empty() { "Tool call" } else { k })),
    }
}

/// `readableOutput`: a plain output field, else a unified diff the output carries, else nothing.
pub fn readable_output(value: Option<&Value>) -> String {
    let Some(value) = value.filter(|v| !v.is_null()) else { return String::new() };
    if let Some(s) = value.as_str() {
        return s.trim().to_owned();
    }
    if !value.is_object() {
        return String::new();
    }
    if let Some(direct) = first_string(Some(value), &TEXT_KEYS) {
        return direct.to_owned();
    }
    let meta = value.get("metadata");
    if let Some(d) = first_string(meta, &["diff"]) {
        return d.to_owned();
    }
    let file_diff = meta.and_then(|m| m.get("fileDiff")).or_else(|| value.get("fileDiff"));
    first_string(file_diff, &["patch", "diff"]).unwrap_or("").to_owned()
}

/// `extractItemText`: readable text on an item of an unknown type, one level deep.
fn item_text(item: &Value) -> Option<&str> {
    first_string(Some(item), &TEXT_KEYS).or_else(|| TEXT_KEYS.iter().find_map(|k| item.get(*k).filter(|v| v.is_object()).and_then(|v| first_string(Some(v), &TEXT_KEYS))))
}

/// `ICONS[toolIconName]`.
pub fn glyph(tool: &Object) -> &'static rune::Icon {
    if is_subagent(tool) || is_subagent_control(tool) {
        return &i::BOT;
    }
    match kind(tool) {
        "read" => &i::FILE_TEXT,
        "edit" => &i::FILE_EDIT,
        "delete" => &i::TRASH2,
        "move" => &i::FOLDER_INPUT,
        "search" => &i::SEARCH,
        "execute" => &i::TERMINAL_SQUARE,
        "think" => &i::BRAIN,
        "fetch" => &i::GLOBE,
        "switch_mode" => &i::SHUFFLE,
        _ => &i::WRENCH,
    }
}

/// `RESULT_BLOCK_CLASS`: monospace output in a bordered box that scrolls past 288 points.
fn result_block(t: &Tokens, words: &str, lang: Option<&str>) -> El {
    let c = &t.colors;
    let mut body = text(words.to_owned()).font(mono(12.0, 400.0, 19.5)).color(c.foreground.alpha(0.9)).wrap();
    if let Some(lang) = lang {
        let highlights = crate::markdown::recut(words, rune_kit::syntax::builtin(Some(lang), words));
        body = body.spans(crate::markdown::syntax_theme(t).spans(&highlights, 0));
    }
    col()
        .w_full()
        .max_h(288.0)
        .radius(4.0)
        .border(1.0, c.border.alpha(0.4))
        .fill(c.background.alpha(0.6))
        .scroll(rune::ScrollSpec { stick_bottom: false })
        .child(col().w_full().px(8.0).py(6.0).child(body))
}

fn plain_text(t: &Tokens, words: &str) -> El {
    text(words.to_owned()).font(sans(12.0, 400.0, 16.0)).color(t.colors.foreground.alpha(0.9)).wrap()
}

/// `renderContentItem`.
fn item(t: &Tokens, it: &Value, lang: Option<&str>, terminal: (&str, Option<i64>)) -> Option<El> {
    match it.get("type").and_then(Value::as_str) {
        Some("diff") => {
            let path = it.get("path").and_then(Value::as_str).unwrap_or("");
            Some(diff::preview(t, path, it.get("oldText").and_then(Value::as_str), it.get("newText").and_then(Value::as_str).unwrap_or("")))
        }
        Some("content") => {
            let block = it.get("content")?;
            match block.get("type").and_then(Value::as_str) {
                Some("text") => {
                    let words = block.get("text").and_then(Value::as_str).unwrap_or("");
                    Some(match lang {
                        Some(_) => result_block(t, words, lang),
                        None => plain_text(t, words),
                    })
                }
                // Images and resources: the media row is a later unit.
                _ => None,
            }
        }
        Some("terminal") => {
            let c = &t.colors;
            let id = it.get("terminalId").and_then(Value::as_str);
            let mut label = id.map_or_else(|| "Terminal".to_owned(), |id| format!("Terminal {id}"));
            if let Some(code) = terminal.1 {
                label.push_str(&format!(" \u{b7} exit {code}"));
            }
            let mut out = col().w_full().gap(4.0).child(
                row().w_full().px(8.0).py(4.0).radius(4.0).border(1.0, c.border.alpha(0.4)).child(text(label).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground)),
            );
            if !terminal.0.is_empty() {
                out = out.child(result_block(t, terminal.0, None));
            }
            Some(out)
        }
        _ => item_text(it).map(|words| plain_text(t, words)),
    }
}

/// Shows the `n`-th diff of a card in full.
pub type ExpandDiff<S> = std::rc::Rc<dyn Fn(&mut S, usize)>;
/// Opens a file, at a line when one is given.
pub type OpenFile<S> = std::rc::Rc<dyn Fn(&mut S, String, Option<usize>)>;

/// How a card answers: the reader's toggles and the file opener.
pub struct Actions<S> {
    pub toggle: Box<dyn Fn(&mut S)>,
    pub expand_diff: ExpandDiff<S>,
    pub open_file: Option<OpenFile<S>>,
}

/// What the reader set on this card: its result open, and which diffs show in full.
pub struct Shown<'a> {
    pub open: bool,
    pub expanded: &'a dyn Fn(usize) -> bool,
}

/// The card for `tool` keyed `key`.
pub fn card<S: 'static>(t: &'static Tokens, key: &str, tool: &Object, shown: Shown, actions: Actions<S>) -> El {
    let c = &t.colors;
    let items = content(tool);
    let has_diff = items.iter().any(is_diff);
    let terminal_output = tool.get("terminalOutput").and_then(Value::as_str).unwrap_or("");
    let exit = tool.get("terminalExitCode").and_then(Value::as_i64);
    let result = if items.is_empty() { readable_output(tool.get("rawOutput")) } else { String::new() };
    let status = tool.get("status").and_then(Value::as_str).unwrap_or("pending");
    let failed = status == "failed";
    let running = is_running(tool);
    let detail_items: Vec<&Value> = items.iter().filter(|it| !(has_diff && is_diff(it))).collect();
    let has_detail = !detail_items.is_empty() || (!has_diff && !result.is_empty()) || !terminal_output.is_empty();
    let lang = (kind(tool) == "read").then(|| path(tool).and_then(rune_kit::syntax::lang_for_path)).flatten();

    let diffs = items.iter().enumerate().filter(|(_, it)| is_diff(it)).map(|(n, it)| {
        let expand = actions.expand_diff.clone();
        let open = actions.open_file.clone();
        diff::edit_block(
            t,
            diff::Block {
                key: format!("{key}-diff-{n}"),
                path: it.get("path").and_then(Value::as_str).unwrap_or(""),
                old_text: it.get("oldText").and_then(Value::as_str),
                new_text: it.get("newText").and_then(Value::as_str).unwrap_or(""),
                running,
                failed,
                expanded: (shown.expanded)(n),
            },
            open.map(|open| move |s: &mut S, path: String, line: usize| open(s, path, Some(line))),
            move |s: &mut S| expand(s, n),
        )
    });
    let inline = has_diff.then(|| col().key(format!("{key}-diffs")).w_full().px(4.0).children(diffs.collect::<Vec<_>>()));
    if has_diff && !has_detail {
        return inline.unwrap_or_else(el);
    }

    let summary = describe(tool);
    let label = format!("{} {}", summary.verb, summary.primary).trim().to_owned();
    let font = sans(12.0, 400.0, 16.0);
    let mut line = row().grow(1.0).min_w(0.0).min_h(28.0).gap(8.0).items_center().child(icon(glyph(tool), 13.0).color(if failed { c.destructive } else { c.muted_foreground }));
    let words = if running {
        crate::shimmer_text(t, format!("{label}\u{2026}"), font, false).min_w(0.0)
    } else {
        let mut spans = Vec::new();
        let mut shown = String::new();
        if !summary.verb.is_empty() {
            shown.push_str(&summary.verb);
            shown.push(' ');
        }
        let start = shown.len();
        shown.push_str(&summary.primary);
        spans.push(rune::Span::new(start, shown.len()).color(if failed { c.destructive } else { c.foreground }).weight(500.0));
        text(shown).font(font).color(c.muted_foreground).spans(spans).single_line().truncate().min_w(0.0)
    };
    line = line.child(row().grow(1.0).min_w(0.0).child(words));
    let meta = sans(MICRO, 400.0, 18.0);
    if let Some((a, r)) = summary.diff_stat {
        line = line.child(diff::stat(t, a, r, false, meta));
    } else if let Some(d) = &summary.detail {
        line = line.child(text(d.clone()).font(meta).color(c.muted_foreground));
    }
    if has_detail {
        line = line.key(format!("{key}-toggle")).role("button").label(label.clone()).focusable(true).on_press(move |s: &mut S| (actions.toggle)(s));
    }
    let mut head = row().w_full().min_h(28.0).gap(8.0).px(4.0).items_center().child(line);
    if failed {
        head = head.child(icon(&i::ALERT_CIRCLE, 12.0).color(c.destructive));
    }
    if has_detail {
        // A down chevron, not a rotated right one: a rotated icon stops the CPU canvas (pin).
        head = head.child(icon(if shown.open { &i::CHEVRON_DOWN } else { &i::CHEVRON_RIGHT }, 13.0).color(c.muted_foreground).a11y_hidden());
    }
    let file = FILE_OPEN_KINDS.contains(&kind(tool)).then(|| path(tool)).flatten();
    if let (Some(open), Some(file)) = (actions.open_file.clone(), file) {
        let file = file.to_owned();
        head = head.child(IconActionButton::new(&i::EXTERNAL_LINK, "Open file").size(IconActionSize::Sm).build(t).key(format!("{key}-open")).on_press(move |s: &mut S| open(s, file.clone(), None)));
    }
    let mut body = col().key(key.to_owned()).w_full().child(head);
    if has_detail && shown.open {
        let blocks: Vec<El> = if !detail_items.is_empty() {
            detail_items.iter().filter_map(|it| item(t, it, lang, (terminal_output, exit))).collect()
        } else if !result.is_empty() {
            vec![result_block(t, &result, lang)]
        } else {
            let words = match exit {
                Some(code) => format!("{terminal_output}\nexit {code}"),
                None => terminal_output.to_owned(),
            };
            vec![result_block(t, &words, lang)]
        };
        body = body.child(
            col()
                .key(format!("{key}-detail"))
                .w_full()
                .ml(16.0)
                .gap(6.0)
                .px(8.0)
                .pt(6.0)
                .pb(8.0)
                .border_sides([0.0, 0.0, 0.0, 1.0], c.border.alpha(0.5))
                .children(blocks),
        );
    }
    match inline {
        Some(inline) => col().w_full().child(inline).child(body),
        None => body,
    }
}
