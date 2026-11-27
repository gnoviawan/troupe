//! Removes a leftover pre-rename (0.4.x) install on Windows startup (#2156).
//!
//! Troupe and the old product share the identifier folder, and the old
//! uninstaller cannot be patched: with its "Delete the application data" box
//! ticked it runs `RmDir /r` over the folder, walks into the account junctions
//! and deletes the user's real `~/.claude` and `~/.codex` history. The
//! `NSIS_HOOK_PREINSTALL` hook (#1764) already removes the old install when
//! Troupe is installed, but that can fail and leave it installed. So on every
//! startup a still-registered old install is uninstalled the same way the
//! hook does it: silently, which never shows the checkbox and keeps app data.
//!
//! The old product name and binary name come from `installer-hooks.nsh`, the
//! one place that names the old product, so the hook and this code always use
//! the same uninstall key and fallback binary.

// Only the Windows startup path uses these outside the tests.
#![cfg_attr(not(windows), allow(dead_code))]

use std::path::{Path, PathBuf};

/// The NSIS hooks bundled into the installer.
const HOOKS: &str = include_str!("../windows/installer-hooks.nsh");

/// Parent of every Apps & features entry, under HKCU or HKLM.
const UNINSTALL_ROOT: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";

/// The quoted value on the first line of `hooks` that starts with `prefix`,
/// e.g. `!define NAME "value"` with prefix `!define NAME `.
pub(crate) fn hook_string<'a>(hooks: &'a str, prefix: &str) -> Option<&'a str> {
    hooks.lines().find_map(|line| {
        let value = line.trim().strip_prefix(prefix)?.trim();
        let value = value.strip_prefix('"')?.strip_suffix('"')?;
        (!value.is_empty() && !value.contains('"')).then_some(value)
    })
}

/// The old product's Apps & features entry (`TROUPE_LEGACY_UNINSTKEY`), under
/// HKCU (per user, the released builds) or HKLM (per machine).
pub(crate) fn legacy_uninstall_key(hooks: &str) -> Option<String> {
    let product = hook_string(hooks, "!define TROUPE_LEGACY_PRODUCTNAME ")?;
    Some(format!(r"{UNINSTALL_ROOT}\{product}"))
}

/// Binary name used when the entry has no `MainBinaryName`, as in the hook.
pub(crate) fn legacy_default_binary(hooks: &str) -> Option<&str> {
    hook_string(hooks, "StrCpy $TroupeLegacyBinary ")
}

/// The old install folder from a raw `InstallLocation` value, or
/// `None` when there is nothing to uninstall. The template writes the value
/// wrapped in quotes. A folder that is Troupe's own (`own_dir`) is never
/// returned, and neither is one with a quote in it, because the folder is
/// passed to the uninstaller verbatim.
pub(crate) fn legacy_install_dir(install_location: &str, own_dir: Option<&Path>) -> Option<String> {
    let trimmed = install_location.trim();
    let unquoted = trimmed
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(trimmed)
        .trim();
    if unquoted.is_empty() || unquoted.contains('"') {
        return None;
    }
    if own_dir.is_some_and(|own| same_windows_dir(unquoted, &own.to_string_lossy())) {
        return None;
    }
    Some(unquoted.to_string())
}

/// Case-insensitive, separator- and trailing-slash-insensitive comparison of
/// two Windows folder paths.
fn same_windows_dir(a: &str, b: &str) -> bool {
    fn normalize(path: &str) -> String {
        let path = path.strip_prefix(r"\\?\").unwrap_or(path);
        path.replace('/', "\\")
            .trim_end_matches('\\')
            .to_lowercase()
    }
    normalize(a) == normalize(b)
}

/// The uninstaller in an old install folder.
pub(crate) fn legacy_uninstaller(dir: &str) -> PathBuf {
    Path::new(dir).join("uninstall.exe")
}

/// The raw command line for the uninstaller, the same flags as the hook:
/// `/S` runs it silently, and `_?=` runs it in place so waiting on it really
/// waits. NSIS reads everything after `_?=` as the folder, so it must be the
/// last argument and must not be quoted.
pub(crate) fn uninstaller_args(dir: &str) -> String {
    format!("/S _?={dir}")
}

/// The old binary name from a raw `MainBinaryName` value, else `default`.
pub(crate) fn legacy_binary_name(raw: Option<&str>, default: &str) -> String {
    match raw.map(str::trim) {
        Some(name) if !name.is_empty() => name.to_string(),
        _ => default.to_string(),
    }
}

/// Whether `tasklist /FO CSV /NH` output lists a process named `image`. The
/// "no tasks" notice is localized but never holds the quoted image name.
pub(crate) fn tasklist_shows(output: &str, image: &str) -> bool {
    let needle = format!("\"{}\"", image.to_lowercase());
    output
        .lines()
        .any(|line| line.trim_start().to_lowercase().starts_with(&needle))
}

/// Uninstall a still-registered old install in the background. Release
/// builds only: a dev build never removes an installed app.
#[cfg(windows)]
pub(crate) fn spawn_remove_legacy_install() {
    if cfg!(debug_assertions) {
        return;
    }
    if let Err(error) = std::thread::Builder::new()
        .name("legacy-uninstall".into())
        .spawn(remove_legacy_install)
    {
        log::warn!("[legacy-uninstall] could not start: {:?}", error.kind());
    }
}

#[cfg(windows)]
fn remove_legacy_install() {
    use std::os::windows::process::CommandExt as _;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use winreg::RegKey;

    let (Some(key), Some(default_binary)) =
        (legacy_uninstall_key(HOOKS), legacy_default_binary(HOOKS))
    else {
        log::error!("[legacy-uninstall] installer hooks do not name the old product");
        return;
    };
    let read = |hive, name: &str| {
        RegKey::predef(hive)
            .open_subkey(&key)
            .ok()?
            .get_value::<String, _>(name)
            .ok()
    };
    // HKCU first, then HKLM, as the hook reads them.
    let Some((location, binary)) = [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE]
        .into_iter()
        .find_map(|hive| {
            let location = read(hive, "InstallLocation").filter(|v| !v.trim().is_empty())?;
            Some((location, read(hive, "MainBinaryName")))
        })
    else {
        return;
    };
    let own_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf));
    let Some(dir) = legacy_install_dir(&location, own_dir.as_deref()) else {
        log::info!("[legacy-uninstall] the old install entry has no separate install folder");
        return;
    };
    let uninstaller = legacy_uninstaller(&dir);
    if !uninstaller.is_file() {
        log::info!("[legacy-uninstall] the old install entry has no uninstaller");
        return;
    }

    // A silent uninstall closes the running old app without asking. Leave it
    // for a launch where it is not running.
    let binary = legacy_binary_name(binary.as_deref(), default_binary);
    let mut tasklist = std::process::Command::new("tasklist");
    crate::process_util::hide_console(&mut tasklist);
    let running = tasklist
        .args([
            "/FI",
            &format!("IMAGENAME eq {binary}"),
            "/FO",
            "CSV",
            "/NH",
        ])
        .output()
        .map(|out| {
            !out.status.success() || tasklist_shows(&String::from_utf8_lossy(&out.stdout), &binary)
        })
        .unwrap_or(true);
    if running {
        log::info!("[legacy-uninstall] the old app is running (or unknown); trying next launch");
        return;
    }

    log::info!("[legacy-uninstall] removing the old install");
    match std::process::Command::new(&uninstaller)
        .raw_arg(uninstaller_args(&dir))
        .status()
    {
        Ok(status) if status.success() => {
            // Run in place, the uninstaller cannot delete itself.
            let _ = std::fs::remove_file(&uninstaller);
            let _ = std::fs::remove_dir(&dir);
            log::info!("[legacy-uninstall] old install removed");
        }
        Ok(status) => {
            log::warn!(
                "[legacy-uninstall] old uninstaller failed (exit code {:?})",
                status.code()
            );
        }
        Err(error) => {
            log::warn!(
                "[legacy-uninstall] could not run the old uninstaller: {:?}",
                error.kind()
            );
        }
    }
}

#[cfg(test)]
#[path = "legacy_uninstall_tests.rs"]
mod tests;
