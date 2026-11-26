//! Expand `$VAR`, `${VAR}` and `%VAR%` references in a project env value
//! against the base (process) environment only.
//!
//! Project env values reach the spawn backend unexpanded — the renderer has
//! no process env to expand against — so the backend expands them here,
//! where the real process env lives. Expansion is single-pass and base-only:
//! a reference never resolves against another project var, matching the
//! renderer's `expandEnvReferences` contract in
//! `src/renderer/lib/env-parser.ts`.

use std::collections::HashMap;

/// Expand variable references in `value` against `base`.
///
/// Supported forms: `$VAR`, `${VAR}`, `%VAR%`. An unknown variable expands
/// to the empty string. Text that does not form a complete reference (a
/// trailing `$`, an unterminated `${`, a lone `%`) is left literal, and
/// substituted text is never re-scanned, so expansion is exactly one pass.
///
/// With `case_insensitive` (Windows), `Path` and `PATH` name the same
/// variable.
pub(crate) fn expand_env_references(
    value: &str,
    base: &HashMap<String, String>,
    case_insensitive: bool,
) -> String {
    let bytes = value.as_bytes();
    let mut out = String::with_capacity(value.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'$' => {
                // `${VAR}` braced form.
                if bytes.get(i + 1) == Some(&b'{') {
                    match bytes[i + 2..].iter().position(|b| *b == b'}') {
                        Some(rel_end) => {
                            let end = i + 2 + rel_end;
                            let name = &value[i + 2..end];
                            if is_valid_name(name) {
                                out.push_str(lookup(base, name, case_insensitive));
                            } else {
                                // `${...}` with an invalid name is literal.
                                out.push_str(&value[i..=end]);
                            }
                            i = end + 1;
                        }
                        None => {
                            // Unterminated `${`: literal, keep scanning after `$`.
                            out.push('$');
                            i += 1;
                        }
                    }
                    continue;
                }
                // `$VAR` bare form: maximal `[A-Za-z_][A-Za-z0-9_]*`.
                let (name, end) = scan_name(value, bytes, i + 1);
                if name.is_empty() {
                    out.push('$');
                    i += 1;
                    continue;
                }
                // A `{` right after the name is not a reference (the
                // renderer's `$VAR` pattern excludes it).
                if bytes.get(end) == Some(&b'{') {
                    out.push_str(&value[i..end]);
                    i = end;
                    continue;
                }
                out.push_str(lookup(base, name, case_insensitive));
                i = end;
            }
            b'%' => {
                // `%VAR%` Windows form.
                let (name, end) = scan_name(value, bytes, i + 1);
                if !name.is_empty() && bytes.get(end) == Some(&b'%') {
                    out.push_str(lookup(base, name, case_insensitive));
                    i = end + 1;
                } else {
                    out.push('%');
                    i += 1;
                }
            }
            _ => {
                // Copy one UTF-8 char verbatim (`$`/`%` are ASCII, so a
                // multi-byte char can never hide a reference).
                let ch_len = utf8_len(bytes[i]);
                out.push_str(&value[i..i + ch_len]);
                i += ch_len;
            }
        }
    }
    out
}

/// Whether `value` contains a reference to the variable named `name`
/// (`$NAME`, `${NAME}` or `%NAME%`), using the same recognition rules as
/// `expand_env_references`: a bare `$NAME` needs the complete name (so
/// `$NAME2` does not count), and a `{` right after a bare name is not a
/// reference. With `case_insensitive` (Windows), `Path` and `PATH` match.
pub(crate) fn value_references_var(value: &str, name: &str, case_insensitive: bool) -> bool {
    let bytes = value.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'$' => {
                // `${NAME}` braced form.
                if bytes.get(i + 1) == Some(&b'{') {
                    match bytes[i + 2..].iter().position(|b| *b == b'}') {
                        Some(rel_end) => {
                            let end = i + 2 + rel_end;
                            if names_equal(&value[i + 2..end], name, case_insensitive) {
                                return true;
                            }
                            i = end + 1;
                        }
                        None => {
                            // Unterminated `${`: literal, keep scanning after `$`.
                            i += 1;
                        }
                    }
                    continue;
                }
                // `$NAME` bare form.
                let (found, end) = scan_name(value, bytes, i + 1);
                if !found.is_empty()
                    && bytes.get(end) != Some(&b'{')
                    && names_equal(found, name, case_insensitive)
                {
                    return true;
                }
                // Skip past the scanned name (or past `$` when there is none).
                i = end.max(i + 1);
            }
            b'%' => {
                // `%NAME%` Windows form.
                let (found, end) = scan_name(value, bytes, i + 1);
                if !found.is_empty() && bytes.get(end) == Some(&b'%') {
                    if names_equal(found, name, case_insensitive) {
                        return true;
                    }
                    // A complete reference: advance past its closing `%`
                    // so it cannot be reused as the opening `%` of
                    // another one (`%FOO%PATH%` references FOO only).
                    i = end + 1;
                } else {
                    i = end.max(i + 1);
                }
            }
            _ => {
                i += utf8_len(bytes[i]);
            }
        }
    }
    false
}

fn names_equal(a: &str, b: &str, case_insensitive: bool) -> bool {
    if case_insensitive {
        a.eq_ignore_ascii_case(b)
    } else {
        a == b
    }
}

/// Expand every value in `custom` against `base` only, returning a new map.
/// All values are expanded before any result is visible to another value,
/// so a reference never resolves against a sibling custom var.
pub(crate) fn expand_custom_env(
    custom: HashMap<String, String>,
    base: &HashMap<String, String>,
    case_insensitive: bool,
) -> HashMap<String, String> {
    custom
        .into_iter()
        .map(|(key, value)| {
            let expanded = expand_env_references(&value, base, case_insensitive);
            (key, expanded)
        })
        .collect()
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn is_valid_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && is_name_start(bytes[0])
        && bytes[1..].iter().all(|b| is_name_char(*b))
}

/// Scan a variable name starting at `start`: `[A-Za-z_][A-Za-z0-9_]*`.
/// Returns the name and the index just past it.
fn scan_name<'a>(value: &'a str, bytes: &[u8], start: usize) -> (&'a str, usize) {
    if start >= bytes.len() || !is_name_start(bytes[start]) {
        return ("", start);
    }
    let mut end = start + 1;
    while end < bytes.len() && is_name_char(bytes[end]) {
        end += 1;
    }
    (&value[start..end], end)
}

fn lookup<'a>(base: &'a HashMap<String, String>, name: &str, case_insensitive: bool) -> &'a str {
    if case_insensitive {
        base.iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
            .unwrap_or("")
    } else {
        base.get(name).map(String::as_str).unwrap_or("")
    }
}

/// Byte length of the UTF-8 char starting with `first` (`value` is a valid
/// `&str`, so the lead byte always determines the length).
fn utf8_len(first: u8) -> usize {
    if first < 0x80 {
        1
    } else if first < 0xE0 {
        2
    } else if first < 0xF0 {
        3
    } else {
        4
    }
}

#[cfg(test)]
mod env_expand_tests;
