//! Self-targeting protection for Troupe Comu.
//!
//! Blocks any Computer Use observation, input, launch, or termination directed
//! at Troupe's own process, windows, or screen coordinates overlapping Troupe,
//! preventing an agent from clicking Troupe's own consent/permission prompts or
//! typing into Troupe terminals.

use serde_json::Value;
use troupe_comu_engine::WindowOwnerInfo;

const SELF_IDENTIFIERS: &[&str] = &[
    "troupe",
    "troupe.exe",
    "termul-manager",
    "termul-manager.exe",
    "com.termul-manager.app",
];

fn is_self_name(candidate: &str) -> bool {
    let trimmed = candidate.trim();
    if trimmed.is_empty() {
        return false;
    }
    let lowered = trimmed.to_ascii_lowercase();
    if SELF_IDENTIFIERS.contains(&lowered.as_str()) {
        return true;
    }
    // Also check basename of a launch path (e.g. `/Applications/Troupe.app` or `/usr/bin/troupe`).
    if let Some(base) = std::path::Path::new(trimmed)
        .file_name()
        .and_then(|s| s.to_str())
    {
        let base_low = base.to_ascii_lowercase();
        if base_low == "troupe.app" || SELF_IDENTIFIERS.contains(&base_low.as_str()) {
            return true;
        }
    }
    false
}

/// Check whether `tool_name` + `args` targets Troupe itself (`own_pid` or any
/// window owned by `own_pid`). Returns `Err(reason)` if blocked.
///
/// `hit_test_ignored` lists Troupe windows the desktop-coordinate hit-test
/// skips: the click-through screen-edge indicator covers every monitor.
/// Targeting those windows by id or pid stays blocked.
pub fn check_not_self_target(
    own_pid: u32,
    tool_name: &str,
    args: &Value,
    windows: &[WindowOwnerInfo],
    hit_test_ignored: &[i64],
) -> Result<(), String> {
    let own_pid_i64 = i64::from(own_pid);

    // 1. Direct PID check (top-level `pid` or `target.pid`).
    for candidate_pid in extract_pids(args) {
        if candidate_pid == own_pid_i64 {
            return Err("Troupe Comu cannot target Troupe's own process".to_string());
        }
    }

    // 2. Window ID ownership check (top-level `window_id` or `target.window_id`).
    for wid in extract_window_ids(args) {
        if let Some(win) = windows.iter().find(|w| w.window_id == wid) {
            if win.pid == own_pid_i64 || is_self_window(win) {
                return Err("Troupe Comu cannot target Troupe's own window".to_string());
            }
        }
    }

    // 3. App launch / filter / name check.
    for key in ["name", "app", "bundle_id", "path", "launch_path", "aumid"] {
        if let Some(val) = args.get(key).and_then(Value::as_str) {
            if is_self_name(val) {
                return Err("Troupe Comu cannot launch or target Troupe itself".to_string());
            }
        }
    }

    // 4. Desktop-coordinate point hit-test: if a call targets the desktop (no
    //    explicit non-Troupe window target) and supplies screen coordinates,
    //    refuse if any coordinate lands inside a Troupe window.
    if is_desktop_coordinate_call(tool_name, args) {
        for (x, y) in extract_screen_points(args) {
            if windows.iter().any(|w| {
                (w.pid == own_pid_i64 || is_self_window(w))
                    && !hit_test_ignored.contains(&w.window_id)
                    && w.contains_point(x, y)
            }) {
                return Err(
                    "Troupe Comu cannot click or move onto Troupe's own window".to_string(),
                );
            }
        }
    }

    // 5. Recursively validate every step in `comu_run_steps`.
    if tool_name == "comu_run_steps" || tool_name == "run_steps" {
        let inherited_pid = args.get("pid").cloned();
        let inherited_wid = args.get("window_id").cloned();
        if let Some(steps) = args.get("steps").and_then(Value::as_array) {
            for step in steps {
                let step_tool = step
                    .get("tool")
                    .and_then(Value::as_str)
                    .unwrap_or("comu_click");
                let mut merged = step
                    .get("args")
                    .cloned()
                    .unwrap_or_else(|| Value::Object(serde_json::Map::new()));
                if let Some(obj) = merged.as_object_mut() {
                    if !obj.contains_key("pid") {
                        if let Some(p) = &inherited_pid {
                            obj.insert("pid".to_string(), p.clone());
                        }
                    }
                    if !obj.contains_key("window_id") {
                        if let Some(w) = &inherited_wid {
                            obj.insert("window_id".to_string(), w.clone());
                        }
                    }
                }
                check_not_self_target(own_pid, step_tool, &merged, windows, hit_test_ignored)?;
            }
        }
    }

    Ok(())
}

fn is_self_window(win: &WindowOwnerInfo) -> bool {
    is_self_name(&win.app_name)
        || win
            .bundle_id
            .as_deref()
            .is_some_and(is_self_name)
}

fn extract_pids(args: &Value) -> Vec<i64> {
    let mut out = Vec::new();
    if let Some(pid) = args.get("pid").and_then(Value::as_i64) {
        out.push(pid);
    }
    if let Some(pid) = args.pointer("/target/pid").and_then(Value::as_i64) {
        out.push(pid);
    }
    out
}

fn extract_window_ids(args: &Value) -> Vec<i64> {
    let mut out = Vec::new();
    if let Some(wid) = args.get("window_id").and_then(Value::as_i64) {
        out.push(wid);
    }
    if let Some(wid) = args.pointer("/target/window_id").and_then(Value::as_i64) {
        out.push(wid);
    }
    out
}

fn is_desktop_coordinate_call(tool_name: &str, args: &Value) -> bool {
    let pointer_tool = matches!(
        tool_name,
        "comu_click"
            | "click"
            | "comu_double_click"
            | "double_click"
            | "comu_right_click"
            | "right_click"
            | "comu_drag"
            | "drag"
            | "comu_scroll"
            | "scroll"
            | "comu_move_cursor"
            | "move_cursor"
    );
    if !pointer_tool {
        return false;
    }
    if args.pointer("/target/kind").and_then(Value::as_str) == Some("desktop") {
        return true;
    }
    // Legacy desktop-coordinate call with neither window target nor element_token.
    args.get("target").is_none()
        && args.get("window_id").is_none()
        && args.get("pid").is_none()
        && args.get("element_token").is_none()
}

fn extract_screen_points(args: &Value) -> Vec<(f64, f64)> {
    let mut pts = Vec::new();
    if let (Some(x), Some(y)) = (
        args.get("x").and_then(Value::as_f64),
        args.get("y").and_then(Value::as_f64),
    ) {
        pts.push((x, y));
    }
    if let (Some(fx), Some(fy)) = (
        args.get("from_x").and_then(Value::as_f64),
        args.get("from_y").and_then(Value::as_f64),
    ) {
        pts.push((fx, fy));
    }
    if let (Some(tx), Some(ty)) = (
        args.get("to_x").and_then(Value::as_f64),
        args.get("to_y").and_then(Value::as_f64),
    ) {
        pts.push((tx, ty));
    }
    pts
}
