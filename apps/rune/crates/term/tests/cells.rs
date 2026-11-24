//! Feeds the PTY recordings in `fixtures/` to [`Term`] and checks every cell, the cursor and the
//! modes at each checkpoint against the dump `@xterm/headless` made of the same bytes
//! (`tools/parity/depth/term-cells.ts`).

use serde_json::Value;
use troupe_term::{Cell, Color, Term};

fn color(c: Color) -> Value {
    match c {
        Color::Default => Value::from("d"),
        Color::Indexed(i) => Value::from(i),
        Color::Rgb(r, g, b) => Value::from(format!("#{r:02x}{g:02x}{b:02x}")),
    }
}

fn cell(c: &Cell) -> Value {
    serde_json::json!([c.text(), c.width, color(c.style.fg), color(c.style.bg), c.style.flags.0])
}

fn blank(v: &Value) -> bool {
    *v == serde_json::json!(["", 1, "d", "d", 0])
}

fn modes(term: &Term) -> Value {
    let m = term.modes();
    let mouse = match m.mouse {
        troupe_term::MouseProtocol::None => "none",
        troupe_term::MouseProtocol::X10 => "x10",
        troupe_term::MouseProtocol::Vt200 => "vt200",
        troupe_term::MouseProtocol::Drag => "drag",
        troupe_term::MouseProtocol::Any => "any",
    };
    serde_json::json!({
        "applicationCursorKeysMode": m.application_cursor_keys,
        "applicationKeypadMode": m.application_keypad,
        "bracketedPasteMode": m.bracketed_paste,
        "insertMode": m.insert,
        "mouseTrackingMode": mouse,
        "originMode": m.origin,
        "reverseWraparoundMode": m.reverse_wraparound,
        "sendFocusMode": m.send_focus,
        "synchronizedOutputMode": m.synchronized_output,
        "wraparoundMode": m.wraparound,
    })
}

/// Every difference between the terminal and one checkpoint's dump, one line each.
fn diff(term: &Term, want: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let buffer = if term.alt_screen() { "alternate" } else { "normal" };
    if want["buffer"] != buffer {
        out.push(format!("buffer {buffer}, want {}", want["buffer"]));
    }
    let c = term.cursor();
    let cursor = serde_json::json!([c.col, c.row]);
    if want["cursor"] != cursor {
        out.push(format!("cursor {cursor}, want {}", want["cursor"]));
    }
    if want["cursorHidden"] != !c.visible {
        out.push(format!("cursor hidden {}, want {}", !c.visible, want["cursorHidden"]));
    }
    let shape = c.shape.map(|s| format!("{s:?}").to_lowercase());
    if want["cursorStyle"] != serde_json::json!(shape) || want["cursorBlink"] != serde_json::json!(c.blink) {
        out.push(format!("cursor style {shape:?} blink {:?}, want {} {}", c.blink, want["cursorStyle"], want["cursorBlink"]));
    }
    let mut got_modes = modes(term);
    let want_modes = &want["modes"];
    got_modes["mouseTrackingMode"] = Value::from(got_modes["mouseTrackingMode"].as_str().unwrap_or(""));
    for (k, v) in got_modes.as_object().into_iter().flatten() {
        if want_modes[k] != *v {
            out.push(format!("mode {k} {v}, want {}", want_modes[k]));
        }
    }
    let lines = want["lines"].as_array().expect("lines");
    if lines.len() != term.lines().len() {
        out.push(format!("{} lines, want {}", term.lines().len(), lines.len()));
    }
    for (y, (line, want_line)) in term.lines().iter().zip(lines).enumerate() {
        if want_line["wrapped"] != line.wrapped {
            out.push(format!("line {y} wrapped {}, want {}", line.wrapped, want_line["wrapped"]));
        }
        let mut cells: Vec<Value> = line.cells.iter().map(cell).collect();
        while cells.last().is_some_and(blank) {
            cells.pop();
        }
        let want_cells = want_line["cells"].as_array().expect("cells");
        for x in 0..cells.len().max(want_cells.len()) {
            let got = cells.get(x).cloned().unwrap_or(serde_json::json!(["", 1, "d", "d", 0]));
            let exp = want_cells.get(x).cloned().unwrap_or(serde_json::json!(["", 1, "d", "d", 0]));
            if got != exp {
                out.push(format!("line {y} col {x}: {got}, want {exp}"));
            }
        }
    }
    out
}

fn replay(name: &str) {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");
    let bytes = std::fs::read(format!("{dir}/{name}.bin")).expect("recording");
    let dump: Value = serde_json::from_str(&std::fs::read_to_string(format!("{dir}/{name}.cells.json")).expect("dump")).expect("json");
    let cols = dump["cols"].as_u64().expect("cols") as u16;
    let rows = dump["rows"].as_u64().expect("rows") as u16;
    let mut term = Term::new(cols, rows);
    let mut fed = 0;
    let checkpoints = dump["checkpoints"].as_array().expect("checkpoints");
    assert_eq!(checkpoints.len(), 3, "three checkpoints");
    for (k, want) in checkpoints.iter().enumerate() {
        let offset = want["offset"].as_u64().expect("offset") as usize;
        term.advance(&bytes[fed..offset]);
        fed = offset;
        let found = diff(&term, want);
        assert!(found.is_empty(), "{name} checkpoint {k} (byte {offset}): {} differences\n{}", found.len(), found.iter().take(40).cloned().collect::<Vec<_>>().join("\n"));
    }
}

#[test]
fn shell_screens_match_xterm() {
    replay("shell");
}

#[test]
fn vim_matches_xterm() {
    replay("vim");
}

#[test]
fn htop_matches_xterm() {
    replay("htop");
}
