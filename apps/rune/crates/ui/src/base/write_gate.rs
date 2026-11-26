//! `WriteGate` (`components/ui/write-gate.tsx`): wraps a write control that a read-only server
//! disables, so the reason still reaches the user. The wrapper is a tab stop whose help text
//! and description carry [`READ_ONLY_SERVER_REASON`]; an ungated control passes through.

use rune::{El, row};
use theme::Tokens;

/// `lib/write-gate.ts`.
pub const READ_ONLY_SERVER_REASON: &str = "This server is read-only for remote clients.";

pub fn write_gate(t: &Tokens, gated: bool, control: El) -> El {
    if !gated {
        return control;
    }
    row()
        .role("group")
        .radius(4.0)
        .focusable(true)
        .focus_ring(2.0, 0.0, t.colors.focus_ring)
        .help(READ_ONLY_SERVER_REASON)
        .description(READ_ONLY_SERVER_REASON)
        .child(control.inert())
}
