//! The escape sequence dispatch: `vte` parses the bytes, and each control, CSI, ESC and OSC
//! sequence runs the operation xterm.js registers for it (`InputHandler`'s constructor).

use vte::{Params, Perform};

use crate::buffer::{Buffer, Charset};
use crate::cell::{Cell, Color, Flags, Style, Underline};
use crate::term::{CursorShape, Emu, Modes, MouseEncoding, MouseProtocol};

/// The first value of each parameter, a missing one read as 0, at least one.
fn firsts(params: &Params) -> Vec<u16> {
    let mut out: Vec<u16> = params.iter().map(|p| p.first().copied().unwrap_or(0)).collect();
    if out.is_empty() {
        out.push(0);
    }
    out
}

impl Perform for Emu {
    fn print(&mut self, c: char) {
        Emu::print(self, c);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            0x08 => self.backspace(),
            0x09 => self.tab(),
            0x0a..=0x0c => self.line_feed(),
            0x0d => self.buf_mut().x = 0,
            0x0e => self.glevel = 1,
            0x0f => self.glevel = 0,
            _ => {}
        }
        self.preceding = 0;
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: char) {
        if !ignore {
            self.csi(params, intermediates, action);
        }
        self.preceding = 0;
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        if !ignore {
            self.esc(intermediates, byte);
        }
        self.preceding = 0;
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        match params {
            [kind, rest @ ..] if matches!(*kind, b"0" | b"2") => {
                let parts: Vec<String> = rest.iter().map(|p| String::from_utf8_lossy(p).into_owned()).collect();
                self.title = parts.join(";");
            }
            [b"133", rest @ ..] => self.command_mark(rest),
            _ => {}
        }
        self.preceding = 0;
    }

    fn unhook(&mut self) {
        self.preceding = 0;
    }
}

impl Emu {
    fn csi(&mut self, params: &Params, intermediates: &[u8], action: char) {
        let p = firsts(params);
        let n = usize::from(p[0].max(1));
        let (cols, rows) = (self.cols, self.rows);
        match (intermediates, action) {
            ([], '@') => {
                self.restrict_cursor(cols - 1);
                let (x, y, fill) = (self.buf().x, self.buf().y, self.erase_style());
                self.row(y).insert_cells(x, n, fill);
            }
            ([b' '], '@') => self.shift_columns(0, n, false, true),
            ([b' '], 'A') => self.shift_columns(0, n, true, true),
            ([b'\''], '}') => self.shift_columns(self.buf().x, n, true, true),
            ([b'\''], '~') => self.shift_columns(self.buf().x, n, false, true),
            ([], 'A') => {
                let b = self.buf();
                let to_top = b.y as isize - b.top as isize;
                let d = if to_top >= 0 { to_top.min(n as isize) } else { n as isize };
                self.move_cursor(0, -d);
            }
            ([], 'B') => self.cursor_down(n),
            ([], 'C' | 'a') => self.move_cursor(n as isize, 0),
            ([], 'D') => self.move_cursor(-(n as isize), 0),
            ([], 'E') => {
                self.cursor_down(n);
                self.buf_mut().x = 0;
            }
            ([], 'F') => {
                let b = self.buf();
                let to_top = b.y as isize - b.top as isize;
                let d = if to_top >= 0 { to_top.min(n as isize) } else { n as isize };
                self.move_cursor(0, -d);
                self.buf_mut().x = 0;
            }
            ([], 'G' | '`') => {
                let y = self.buf().y as isize;
                self.set_cursor(n as isize - 1, y);
            }
            ([], 'H' | 'f') => {
                let col = if p.len() >= 2 { p[1].max(1) as isize - 1 } else { 0 };
                self.set_cursor(col, n as isize - 1);
            }
            ([], 'I') => {
                if self.buf().x < cols {
                    for _ in 0..n {
                        let b = self.buf_mut();
                        b.x = b.next_stop(cols);
                    }
                }
            }
            ([], 'Z') => {
                if self.buf().x < cols {
                    for _ in 0..n {
                        let b = self.buf_mut();
                        b.x = b.prev_stop(cols);
                    }
                }
            }
            ([] | [b'?'], 'J') => self.erase_in_display(p[0]),
            ([] | [b'?'], 'K') => {
                self.restrict_cursor(cols);
                let (x, y) = (self.buf().x, self.buf().y);
                match p[0] {
                    0 => self.erase_in_line(y, x, cols, x == 0),
                    1 => self.erase_in_line(y, 0, x + 1, false),
                    2 => self.erase_in_line(y, 0, cols, true),
                    _ => {}
                }
            }
            ([], 'L' | 'M') => {
                self.restrict_cursor(cols - 1);
                let (y, top, bottom) = (self.buf().y, self.buf().top, self.buf().bottom);
                if y <= bottom && y >= top {
                    let fill = self.erase_style();
                    self.shift_region(y, n.min(bottom - y + 1), action == 'M', fill);
                    self.buf_mut().x = 0;
                }
            }
            ([], 'P') => {
                self.restrict_cursor(cols - 1);
                let (x, y, fill) = (self.buf().x, self.buf().y, self.erase_style());
                self.row(y).delete_cells(x, n, fill);
            }
            ([], 'S') => {
                let (top, bottom, fill) = (self.buf().top, self.buf().bottom, self.erase_style());
                self.shift_region(top, n.min(bottom - top + 1), true, fill);
            }
            ([], 'T' | '^') => {
                let (top, bottom) = (self.buf().top, self.buf().bottom);
                self.shift_region(top, n.min(bottom - top + 1), false, Style::default());
            }
            ([], 'X') => {
                self.restrict_cursor(cols - 1);
                let (x, y, fill) = (self.buf().x, self.buf().y, self.erase_style());
                self.row(y).replace_cells(x, x + n, fill);
            }
            ([], 'b') => self.repeat(n),
            ([], 'c') if p[0] == 0 => self.reply("\x1b[?1;2c"),
            ([b'>'], 'c') if p[0] == 0 => self.reply("\x1b[>0;276;0c"),
            ([], 'd') => {
                let x = self.buf().x as isize;
                self.set_cursor(x, n as isize - 1);
            }
            ([], 'e') => self.move_cursor(0, n as isize),
            ([], 'g') => {
                let x = self.buf().x;
                let tabs = &mut self.buf_mut().tabs;
                match p[0] {
                    0 => {
                        if let Some(t) = tabs.get_mut(x) {
                            *t = false;
                        }
                    }
                    3 => tabs.fill(false),
                    _ => {}
                }
            }
            ([], 'h' | 'l') => {
                for &m in &p {
                    if m == 4 {
                        self.modes.insert = action == 'h';
                    }
                }
            }
            ([b'?'], 'h' | 'l') => {
                for &m in &p {
                    self.dec_mode(m, action == 'h');
                }
            }
            ([], 'm') => self.sgr(params),
            ([], 'n') => match p[0] {
                5 => self.reply("\x1b[0n"),
                6 => {
                    let (x, y) = (self.buf().x, self.buf().y);
                    self.reply(&format!("\x1b[{};{}R", y + 1, x + 1));
                }
                _ => {}
            },
            ([b'?'], 'n') if p[0] == 6 => {
                let (x, y) = (self.buf().x, self.buf().y);
                self.reply(&format!("\x1b[?{};{}R", y + 1, x + 1));
            }
            ([b'!'], 'p') => self.soft_reset(),
            ([b' '], 'q') => {
                let param = if params.is_empty() { 1 } else { p[0] };
                if param == 0 {
                    self.cursor_shape = None;
                    self.cursor_blink = None;
                } else {
                    self.cursor_shape = match param {
                        1 | 2 => Some(CursorShape::Block),
                        3 | 4 => Some(CursorShape::Underline),
                        5 | 6 => Some(CursorShape::Bar),
                        _ => self.cursor_shape,
                    };
                    self.cursor_blink = Some(param % 2 == 1);
                }
            }
            ([], 'r') => {
                let top = usize::from(p[0].max(1));
                let mut bottom = p.get(1).map_or(rows, |&b| usize::from(b));
                if p.len() < 2 || bottom > rows || bottom == 0 {
                    bottom = rows;
                }
                if bottom > top {
                    let b = self.buf_mut();
                    b.top = top - 1;
                    b.bottom = bottom - 1;
                    self.set_cursor(0, 0);
                }
            }
            ([], 's') => self.save_cursor(),
            ([], 'u') => self.restore_cursor(),
            _ => {}
        }
    }

    fn cursor_down(&mut self, n: usize) {
        let b = self.buf();
        let to_bottom = b.bottom as isize - b.y as isize;
        let d = if to_bottom >= 0 { to_bottom.min(n as isize) } else { n as isize };
        self.move_cursor(0, d);
    }

    /// `scrollLeft`, `scrollRight`, `insertColumns` and `deleteColumns`: every row of the
    /// region shifts its cells from `at`.
    fn shift_columns(&mut self, at: usize, n: usize, insert: bool, clear_wrap: bool) {
        let (y, top, bottom) = (self.buf().y, self.buf().top, self.buf().bottom);
        if y > bottom || y < top {
            return;
        }
        let fill = self.erase_style();
        for row in top..=bottom {
            let line = self.row(row);
            if insert {
                line.insert_cells(at, n, fill);
            } else {
                line.delete_cells(at, n, fill);
            }
            if clear_wrap {
                line.wrapped = false;
            }
        }
    }

    fn erase_in_display(&mut self, kind: u16) {
        let (cols, rows) = (self.cols, self.rows);
        self.restrict_cursor(cols);
        let (x, y) = (self.buf().x, self.buf().y);
        match kind {
            0 => {
                self.erase_in_line(y, x, cols, x == 0);
                for j in y + 1..rows {
                    self.reset_line(j);
                }
            }
            1 => {
                self.erase_in_line(y, 0, x + 1, true);
                if x + 1 >= cols {
                    // xterm.js reads this index as absolute, not as a screen row.
                    if let Some(line) = self.buf_mut().lines.get_mut(y + 1) {
                        line.wrapped = false;
                    }
                }
                for j in (0..y).rev() {
                    self.reset_line(j);
                }
            }
            2 => {
                for j in (0..rows).rev() {
                    self.reset_line(j);
                }
            }
            3 => {
                let b = self.buf_mut();
                let extra = b.lines.len().saturating_sub(rows);
                b.lines.drain(..extra);
            }
            _ => {}
        }
    }

    /// `REP`: the last printed character again `n` times.
    fn repeat(&mut self, n: usize) {
        let w = usize::from(self.preceding);
        if w == 0 {
            return;
        }
        let (x, y) = (self.buf().x, self.buf().y);
        let Some(at) = x.checked_sub(w) else { return };
        let b = self.buf();
        let text = b.lines[b.base(self.rows) + y].cells.get(at).map(Cell::text).unwrap_or_default();
        for _ in 0..n.min(self.cols * self.rows) {
            for c in text.chars() {
                self.print(c);
            }
        }
    }

    fn dec_mode(&mut self, mode: u16, on: bool) {
        match mode {
            1 => self.modes.application_cursor_keys = on,
            2 if on => self.charsets = [Charset::Ascii; 4],
            6 => {
                self.modes.origin = on;
                self.set_cursor(0, 0);
            }
            7 => self.modes.wraparound = on,
            45 => self.modes.reverse_wraparound = on,
            66 => self.modes.application_keypad = on,
            9 | 1000 | 1002 | 1003 => {
                self.modes.mouse = match (on, mode) {
                    (false, _) => MouseProtocol::None,
                    (true, 9) => MouseProtocol::X10,
                    (true, 1000) => MouseProtocol::Vt200,
                    (true, 1002) => MouseProtocol::Drag,
                    _ => MouseProtocol::Any,
                }
            }
            1004 => self.modes.send_focus = on,
            1006 => self.modes.mouse_encoding = if on { MouseEncoding::Sgr } else { MouseEncoding::Default },
            1016 => self.modes.mouse_encoding = if on { MouseEncoding::SgrPixels } else { MouseEncoding::Default },
            25 => self.cursor_hidden = !on,
            1048 => {
                if on {
                    self.save_cursor();
                } else {
                    self.restore_cursor();
                }
            }
            47 | 1047 | 1049 => {
                if on {
                    if mode == 1049 {
                        self.save_cursor();
                    }
                    self.activate_alt();
                } else {
                    self.activate_normal();
                    if mode == 1049 {
                        self.restore_cursor();
                    }
                }
            }
            2004 => self.modes.bracketed_paste = on,
            2026 => self.modes.synchronized_output = on,
            _ => {}
        }
    }

    fn soft_reset(&mut self) {
        let rows = self.rows;
        self.cursor_hidden = false;
        self.pen = Style::default();
        let (mouse, mouse_encoding) = (self.modes.mouse, self.modes.mouse_encoding);
        self.modes = Modes { mouse, mouse_encoding, ..Modes::default() };
        self.cursor_shape = None;
        self.cursor_blink = None;
        self.charsets = [Charset::Ascii; 4];
        self.glevel = 0;
        let b = self.buf_mut();
        b.top = 0;
        b.bottom = rows - 1;
        b.saved.x = 0;
        b.saved.y = b.base(rows);
        b.saved.style = Style::default();
        b.saved.charsets = [Charset::Ascii; 4];
        b.saved.glevel = 0;
    }

    fn esc(&mut self, intermediates: &[u8], byte: u8) {
        match (intermediates, byte) {
            ([], b'7') => self.save_cursor(),
            ([], b'8') => self.restore_cursor(),
            ([], b'D') => self.index(),
            ([], b'E') => {
                self.buf_mut().x = 0;
                self.index();
            }
            ([], b'H') => {
                let x = self.buf().x;
                if let Some(t) = self.buf_mut().tabs.get_mut(x) {
                    *t = true;
                }
            }
            ([], b'M') => self.reverse_index(),
            ([], b'=') => self.modes.application_keypad = true,
            ([], b'>') => self.modes.application_keypad = false,
            ([], b'c') => self.full_reset(),
            ([], b'n' | b'}') => self.glevel = 2,
            ([], b'o' | b'|') => self.glevel = 3,
            ([], b'~') => self.glevel = 1,
            ([b'%'], b'@' | b'G') => {
                self.glevel = 0;
                self.charsets[0] = Charset::Ascii;
            }
            ([slot @ (b'(' | b')' | b'*' | b'+' | b'-' | b'.')], flag) => {
                let index = match slot {
                    b'(' => 0,
                    b')' | b'-' => 1,
                    b'*' | b'.' => 2,
                    _ => 3,
                };
                self.charsets[index] = match flag {
                    b'0' => Charset::DecGraphics,
                    b'A' => Charset::Uk,
                    _ => Charset::Ascii,
                };
            }
            ([b'#'], b'8') => self.alignment_pattern(),
            _ => {}
        }
    }

    fn alignment_pattern(&mut self) {
        let pen = self.pen;
        self.set_cursor(0, 0);
        let (rows, y) = (self.rows, self.buf().y);
        let cell = Cell { ch: 'E', marks: None, width: 1, style: pen };
        for offset in 0..rows {
            let b: &mut Buffer = self.buf_mut();
            let i = b.base(rows) + y + offset;
            if let Some(line) = b.lines.get_mut(i) {
                line.fill(cell.clone());
                line.wrapped = false;
                line.touch();
            }
        }
        self.set_cursor(0, 0);
    }

    /// `charAttributes`.
    fn sgr(&mut self, params: &Params) {
        let groups: Vec<&[u16]> = params.iter().collect();
        if groups.is_empty() {
            self.pen = Style::default();
            return;
        }
        let mut i = 0;
        while i < groups.len() {
            let g = groups[i];
            let p = g.first().copied().unwrap_or(0);
            let pen = &mut self.pen;
            match p {
                0 => *pen = Style::default(),
                1 => pen.flags.set(Flags::BOLD, true),
                2 => pen.flags.set(Flags::DIM, true),
                3 => pen.flags.set(Flags::ITALIC, true),
                4 => underline(pen, g.get(1).map_or(1, |&s| i32::from(s))),
                5 => pen.flags.set(Flags::BLINK, true),
                7 => pen.flags.set(Flags::INVERSE, true),
                8 => pen.flags.set(Flags::INVISIBLE, true),
                9 => pen.flags.set(Flags::STRIKE, true),
                21 => underline(pen, 2),
                22 => {
                    pen.flags.set(Flags::BOLD, false);
                    pen.flags.set(Flags::DIM, false);
                }
                23 => pen.flags.set(Flags::ITALIC, false),
                24 => underline(pen, 0),
                25 => pen.flags.set(Flags::BLINK, false),
                27 => pen.flags.set(Flags::INVERSE, false),
                28 => pen.flags.set(Flags::INVISIBLE, false),
                29 => pen.flags.set(Flags::STRIKE, false),
                30..=37 => pen.fg = Color::Indexed((p - 30) as u8),
                39 => pen.fg = Color::Default,
                40..=47 => pen.bg = Color::Indexed((p - 40) as u8),
                49 => pen.bg = Color::Default,
                53 => pen.flags.set(Flags::OVERLINE, true),
                55 => pen.flags.set(Flags::OVERLINE, false),
                90..=97 => pen.fg = Color::Indexed((p - 90 + 8) as u8),
                100..=107 => pen.bg = Color::Indexed((p - 100 + 8) as u8),
                221 => pen.flags.set(Flags::BOLD, false),
                222 => pen.flags.set(Flags::DIM, false),
                38 | 48 | 58 => {
                    let (target, mode, rgb, advance) = extract_color(&groups, i);
                    let color = match mode {
                        2 => Some(Color::Rgb(rgb[0], rgb[1], rgb[2])),
                        5 => Some(Color::Indexed(rgb[0])),
                        _ => None,
                    };
                    if let Some(color) = color {
                        match target {
                            38 => self.pen.fg = color,
                            48 => self.pen.bg = color,
                            _ => {}
                        }
                    }
                    i += advance;
                }
                _ => {}
            }
            i += 1;
        }
    }
}

/// `_processUnderline`: 0 turns it off, an unknown style reads as single.
fn underline(pen: &mut Style, style: i32) {
    let style = if !(0..=5).contains(&style) { 1 } else { style };
    pen.underline = match style {
        2 => Underline::Double,
        3 => Underline::Curly,
        4 => Underline::Dotted,
        5 => Underline::Dashed,
        _ => Underline::Single,
    };
    pen.flags.set(Flags::UNDERLINE, style != 0);
}

/// `_extractColor`: reads `38;5;n`, `38;2;r;g;b` and their colon forms from parameter `pos`,
/// returning the target (38, 48 or 58), the colour mode, its values (an index or r, g, b) and
/// how many more parameters it took.
fn extract_color(groups: &[&[u16]], pos: usize) -> (i32, i32, [u8; 3], usize) {
    let mut accu = [0i32, 0, -1, 0, 0, 0];
    let mut cspace = 0usize;
    let mut advance = 0usize;
    loop {
        let g = groups[pos + advance];
        accu[advance + cspace] = i32::from(g.first().copied().unwrap_or(0));
        if g.len() > 1 {
            let sub = &g[1..];
            let mut k = 0;
            loop {
                if accu[1] == 5 {
                    cspace = 1;
                }
                let at = advance + k + 1 + cspace;
                if at < accu.len() {
                    accu[at] = i32::from(sub[k]);
                }
                k += 1;
                if !(k < sub.len() && k + advance + 1 + cspace < accu.len()) {
                    break;
                }
            }
            break;
        }
        if (accu[1] == 5 && advance + cspace >= 2) || (accu[1] == 2 && advance + cspace >= 5) {
            break;
        }
        if accu[1] != 0 {
            cspace = 1;
        }
        advance += 1;
        if !(advance + pos < groups.len() && advance + cspace < accu.len()) {
            break;
        }
    }
    for v in accu.iter_mut().skip(2) {
        if *v == -1 {
            *v = 0;
        }
    }
    let byte = |v: i32| (v & 0xff) as u8;
    let values = if accu[1] == 5 { [byte(accu[3]), 0, 0] } else { [byte(accu[3]), byte(accu[4]), byte(accu[5])] };
    (accu[0], accu[1], values, advance)
}
