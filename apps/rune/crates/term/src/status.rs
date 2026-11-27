//! The command status marks Troupe's integrated bash and zsh write in band
//! (`command-status-gutter.ts`): `OSC 133;A;termul` where a prompt starts, and
//! `OSC 133;D;<exit>;termul` after a user command, before the next prompt. The finish closes the
//! latest prompt: its command row (the first non-blank row at or below the mark, the spacer rows
//! aside) gets the exit code, which the view draws as a dot in the left padding.
//!
//! The marks are not authenticated; any program output can forge one, so the status is only
//! drawn, never acted on. Plain `133;A` / `133;D` from user prompt frameworks are ignored, so a
//! command never gets two dots, and marks on the alternate screen are ignored.

use crate::term::Emu;

/// How far below the prompt mark the command echo may sit (the host's spacer rows).
const MAX_LEADING_BLANK_ROWS: usize = 2;

impl Emu {
    /// The parameters after `133` of an OSC 133 sequence.
    pub(crate) fn command_mark(&mut self, rest: &[&[u8]]) {
        if self.alt_active {
            return;
        }
        match rest {
            [b"A", b"termul"] => {
                let b = &self.normal;
                let wrap = usize::from(b.x >= self.cols);
                self.prompt_mark = Some(b.dropped + b.base(self.rows) + b.y + wrap);
            }
            [b"D", code, b"termul"] => {
                let Some(exit_code) = exit_code(code) else { return };
                let Some(mark) = self.prompt_mark.take() else { return };
                self.close_prompt(mark, exit_code);
            }
            _ => {}
        }
    }

    fn close_prompt(&mut self, mark: usize, exit_code: i64) {
        let b = &mut self.normal;
        let Some(first) = mark.checked_sub(b.dropped) else { return };
        let cursor = b.base(self.rows) + b.y;
        let last = cursor.min(first + MAX_LEADING_BLANK_ROWS);
        // None when the command erased its own echo (an inline TUI clearing from its row down).
        let Some(row) = (first..=last).find(|&i| b.lines.get(i).is_some_and(|l| !l.is_blank())) else { return };
        let line = &mut b.lines[row];
        line.status = Some(exit_code);
        line.touch();
    }
}

/// `-?\d{1,10}`, as the shell writes `$?`.
fn exit_code(code: &[u8]) -> Option<i64> {
    let digits = code.strip_prefix(b"-").unwrap_or(code);
    if digits.is_empty() || digits.len() > 10 || !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    std::str::from_utf8(code).ok()?.parse().ok()
}
