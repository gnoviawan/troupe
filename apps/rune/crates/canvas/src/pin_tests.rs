//! The desktop editor is the figcraft build `src-tauri` pins: one tag, in `canvas/figcraft.pin`.

const MANIFEST: &str = include_str!("../Cargo.toml");
const LOCK: &str = include_str!("../../../Cargo.lock");

#[test]
fn every_figcraft_dependency_uses_the_pinned_tag() {
    let deps: Vec<&str> = MANIFEST.lines().filter(|l| l.starts_with("figcraft-")).collect();
    assert_eq!(deps.len(), 5, "{deps:?}");
    for dep in deps {
        assert!(dep.contains(&format!("tag = \"{}\"", crate::PIN_TAG)), "{dep} is not on {}", crate::PIN_TAG);
    }
    assert_eq!(crate::PIN_TAG, "termul-v0.6.0");
}

#[test]
fn the_reported_engine_version_is_the_locked_one() {
    let block = LOCK.split("[[package]]").find(|p| p.contains("name = \"figcraft-engine\"")).expect("figcraft-engine is locked");
    assert!(block.contains(&format!("version = \"{}\"", crate::ENGINE_VERSION)), "{block}");
    assert!(block.contains(&format!("?tag={}#", crate::PIN_TAG)), "{block}");
}
