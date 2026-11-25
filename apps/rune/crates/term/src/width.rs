//! How many columns a character takes, by the React terminal's table (xterm.js's Unicode 6
//! provider, generated into `width_table.rs`).

use crate::width_table::{WIDE, ZERO};

fn within(table: &[(u32, u32)], cp: u32) -> bool {
    table
        .binary_search_by(|&(lo, hi)| {
            if hi < cp {
                std::cmp::Ordering::Less
            } else if lo > cp {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

/// 0 for a combining mark, 2 for a wide (CJK) character, else 1.
pub fn width(c: char) -> u8 {
    let cp = c as u32;
    if cp < 0x20 {
        0
    } else if cp < 0x7f {
        1
    } else if within(ZERO, cp) {
        0
    } else if within(WIDE, cp) {
        2
    } else {
        1
    }
}
