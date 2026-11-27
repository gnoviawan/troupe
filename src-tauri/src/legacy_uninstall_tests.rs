use super::*;
use std::path::Path;

#[test]
fn install_dir_strips_the_template_quotes() {
    assert_eq!(
        legacy_install_dir(r#""C:\Users\me\AppData\Local\Old App""#, None).as_deref(),
        Some(r"C:\Users\me\AppData\Local\Old App")
    );
    assert_eq!(
        legacy_install_dir(r"C:\Program Files\Old App", None).as_deref(),
        Some(r"C:\Program Files\Old App")
    );
}

#[test]
fn install_dir_is_none_when_empty_or_quoted_inside() {
    assert_eq!(legacy_install_dir("", None), None);
    assert_eq!(legacy_install_dir("  ", None), None);
    assert_eq!(legacy_install_dir(r#""""#, None), None);
    assert_eq!(legacy_install_dir(r#"C:\a" /D=C:\b"#, None), None);
}

#[test]
fn install_dir_is_none_when_it_is_troupes_own_folder() {
    let own = Path::new(r"C:\Users\me\AppData\Local\Old App\");
    assert_eq!(
        legacy_install_dir(r#""c:/users/ME/AppData/Local/Old App""#, Some(own)),
        None
    );
    let own = Path::new(r"\\?\C:\Users\me\AppData\Local\Old App");
    assert_eq!(
        legacy_install_dir(r"C:\Users\me\AppData\Local\Old App", Some(own)),
        None
    );
    let own = Path::new(r"C:\Users\me\AppData\Local\Troupe");
    assert_eq!(
        legacy_install_dir(r"C:\Users\me\AppData\Local\Old App", Some(own)).as_deref(),
        Some(r"C:\Users\me\AppData\Local\Old App")
    );
}

#[test]
fn uninstaller_runs_silently_in_place_with_an_unquoted_folder_last() {
    let dir = r"C:\Program Files\Old App";
    assert_eq!(uninstaller_args(dir), r"/S _?=C:\Program Files\Old App");
    assert!(legacy_uninstaller(dir).ends_with("uninstall.exe"));
    // The same flags the PREINSTALL hook passes.
    assert!(HOOKS.contains(r#"/S _?=$TroupeLegacyDir"#));
}

#[test]
fn hook_string_reads_a_quoted_value() {
    let hooks = "; comment\n  !define NAME \"Old App\"\n!define NAME \"second\"\n";
    assert_eq!(hook_string(hooks, "!define NAME "), Some("Old App"));
    assert_eq!(hook_string("!define NAME Old", "!define NAME "), None);
    assert_eq!(hook_string("!define NAME \"\"", "!define NAME "), None);
    assert_eq!(hook_string(hooks, "!define OTHER "), None);
}

#[test]
fn uninstall_key_and_fallback_binary_match_the_installer_hook() {
    let product = hook_string(HOOKS, "!define TROUPE_LEGACY_PRODUCTNAME ").expect("product name");
    let key = legacy_uninstall_key(HOOKS).expect("uninstall key");
    // The hook builds its key from the same root and product name.
    let hook_key =
        hook_string(HOOKS, "!define TROUPE_LEGACY_UNINSTKEY ").expect("hook uninstall key");
    assert_eq!(
        key,
        hook_key.replace("${TROUPE_LEGACY_PRODUCTNAME}", product)
    );
    assert_eq!(
        key,
        format!(r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{product}")
    );
    let binary = legacy_default_binary(HOOKS).expect("fallback binary");
    assert!(binary.ends_with(".exe"));
    assert!(!binary.contains('\\'));
}

#[test]
fn binary_name_falls_back_like_the_hook() {
    assert_eq!(legacy_binary_name(None, "old-app.exe"), "old-app.exe");
    assert_eq!(legacy_binary_name(Some(" "), "old-app.exe"), "old-app.exe");
    assert_eq!(
        legacy_binary_name(Some("renamed.exe"), "old-app.exe"),
        "renamed.exe"
    );
}

#[test]
fn tasklist_output_is_read_case_insensitively() {
    let running = "\"old-app.exe\",\"1234\",\"Console\",\"1\",\"120,000 K\"\r\n";
    assert!(tasklist_shows(running, "old-app.exe"));
    assert!(tasklist_shows(&running.to_uppercase(), "old-app.exe"));
    let none = "INFO: No tasks are running which match the specified criteria.\r\n";
    assert!(!tasklist_shows(none, "old-app.exe"));
    assert!(!tasklist_shows("", "old-app.exe"));
    let other = "\"old-app.exe.bak\",\"1\"\r\n\"troupe.exe\",\"2\"\r\n";
    assert!(!tasklist_shows(other, "old-app.exe"));
}

#[test]
fn preuninstall_unlinks_without_recursing_and_keeps_data_on_failure() {
    let start = HOOKS
        .find("!macro NSIS_HOOK_PREUNINSTALL")
        .expect("hook exists");
    let hook = &HOOKS[start..];
    let hook = &hook[..hook.find("!macroend").unwrap()];
    // Same gate as the template's app data deletion.
    assert!(hook.contains("${If} $DeleteAppDataCheckboxState = 1"));
    assert!(hook.contains("${AndIf} $UpdateMode <> 1"));
    assert!(hook.contains(r#"Push "$APPDATA\${BUNDLEID}""#));
    assert!(hook.contains(r#"Push "$LOCALAPPDATA\${BUNDLEID}""#));
    assert!(hook.contains("StrCpy $DeleteAppDataCheckboxState 0"));
    // The running-app check comes before the first link is removed.
    let check = hook
        .find(r#"!insertmacro CheckIfAppIsRunning "$INSTDIR\${MAINBINARYNAME}.exe""#)
        .expect("running-app check");
    let sweep = hook.find("Call un.TroupeUnlinkAppData").expect("sweep");
    assert!(check < sweep);
    assert!(!hook.contains("Call un.TroupeUnlinkDirLinks"));

    // A root that is itself a link is unlinked, never entered.
    let start = HOOKS
        .find("Function un.TroupeUnlinkAppData")
        .expect("root check exists");
    let root = &HOOKS[start..];
    let root = &root[..root.find("FunctionEnd").unwrap()];
    assert!(root.contains("& 0x400"));
    assert!(root.contains(r#"RMDir "$R0""#));
    assert!(!root.to_lowercase().contains("rmdir /r"));

    let start = HOOKS
        .find("Function un.TroupeUnlinkDirLinks")
        .expect("walker exists");
    let walker = &HOOKS[start..];
    let walker = &walker[..walker.find("FunctionEnd").unwrap()];
    assert!(walker.contains("& 0x400"));
    assert!(walker.contains(r#"RMDir "$R3""#));
    assert!(!walker.to_lowercase().contains("rmdir /r"));
}

#[test]
fn resolved_dir_matches_other_spellings_of_the_same_folder() {
    let root = tempfile::tempdir().unwrap();
    let own = root.path().join("Troupe");
    let other = root.path().join("Old App");
    std::fs::create_dir(&own).unwrap();
    std::fs::create_dir(&other).unwrap();
    assert_eq!(same_resolved_dir(&own.join("."), &own), Some(true));
    assert_eq!(
        same_resolved_dir(&root.path().join("Old App").join("..").join("Troupe"), &own),
        Some(true)
    );
    assert_eq!(same_resolved_dir(&other, &own), Some(false));
    // A folder that cannot be resolved is never treated as separate.
    assert_eq!(same_resolved_dir(&root.path().join("missing"), &own), None);
}

#[cfg(unix)]
#[test]
fn resolved_dir_follows_a_link_to_the_folder() {
    let root = tempfile::tempdir().unwrap();
    let own = root.path().join("Troupe");
    std::fs::create_dir(&own).unwrap();
    let link = root.path().join("link");
    std::os::unix::fs::symlink(&own, &link).unwrap();
    assert_eq!(same_resolved_dir(&link, &own), Some(true));
}
