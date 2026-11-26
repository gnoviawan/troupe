//! The icon catalog answers the lookups the web client makes.

use theme::icons;

#[test]
fn app_icons_by_export_name() {
    assert_eq!(icons::names().count(), 180);
    let branch = icons::named("GitBranch").expect("GitBranch");
    assert_eq!((branch.name, branch.view), ("GitBranch", 24.0));
    assert!(branch.body.contains("stroke=\"currentColor\""));
    // Aliases share the canonical icon.
    assert!(std::ptr::eq(
        icons::named("ChevronDownIcon").unwrap(),
        icons::named("ChevronDown").unwrap()
    ));
    assert!(std::ptr::eq(
        icons::named("Search").unwrap(),
        &icons::SEARCH
    ));
    assert!(icons::named("NoSuchIcon").is_none());
    // Square is drawn by the app, not Hugeicons: its paint lives on the root <svg>.
    assert!(
        icons::SQUARE
            .body
            .starts_with("<g fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.5\"")
    );
}

#[test]
fn file_icons_resolve_like_material_icon_resolver() {
    let file = |name: &str, dark: bool| icons::file(name, dark).name;
    for dark in [true, false] {
        assert_eq!(file("package.json", dark), "nodejs");
        assert_eq!(file("main.rs", dark), "rust");
        assert_eq!(file("index.d.ts", dark), "typescript-def");
        assert_eq!(file("Dockerfile", dark), "docker");
        assert_eq!(file("README.md", dark), "readme");
        assert_eq!(file("notes.unknownext", dark), "file");
        assert_eq!(icons::folder("src", false, false, dark).name, "folder-src");
        assert_eq!(
            icons::folder("src", true, false, dark).name,
            "folder-src-open"
        );
        assert_eq!(icons::folder("src", false, true, dark).name, "folder-src");
        assert_eq!(icons::folder("random", false, false, dark).name, "folder");
        assert_eq!(
            icons::folder("random", true, true, dark).name,
            "folder-root-open"
        );
        assert_eq!(
            icons::folder(".github", false, false, dark).name,
            "folder-github"
        );
    }
    assert_eq!(file("page.jinja", true), "jinja");
    assert_eq!(file("page.jinja", false), "jinja_light");
    assert_eq!(
        icons::folder("jinja", false, false, false).name,
        "folder-jinja_light"
    );
    assert_eq!(icons::file_icons().len(), 1250);
}
