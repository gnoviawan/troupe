use super::links::*;

fn found(text: &str) -> Vec<&str> {
    find_paths(text).into_iter().map(|r| &text[r]).collect()
}

#[test]
fn bare_paths_are_found_and_prose_slashes_are_not() {
    assert_eq!(found("built once in src/renderer/App.tsx with it"), ["src/renderer/App.tsx"]);
    assert_eq!(found("see ./a/b.rs:12:3, then"), ["./a/b.rs:12:3"]);
    assert_eq!(found("(src/lib/x.ts)"), ["src/lib/x.ts"]);
    assert_eq!(found("and/or 2.5/3.0 text/text"), Vec::<&str>::new());
    assert_eq!(found("https://example.com/a/b.html"), Vec::<&str>::new());
    assert_eq!(found("C:\\repo\\main.rs done"), ["C:\\repo\\main.rs"]);
}

#[test]
fn the_rewrite_links_prose_paths_and_leaves_code_and_links_alone() {
    let src = "Open src/a_b.ts and `src/c.ts`.\n```\nsrc/d.ts\n```\n[docs](src/e.md)";
    assert_eq!(
        link_file_paths(src),
        "Open [src/a\\_b.ts](termul-file:src/a_b.ts) and `src/c.ts`.\n```\nsrc/d.ts\n```\n[docs](src/e.md)"
    );
}

#[test]
fn links_classify_as_files_urls_or_fragments() {
    assert_eq!(classify("termul-file:src/a%20b.ts"), LinkTarget::File("src/a b.ts".into()));
    assert_eq!(classify("src/e.md#L12"), LinkTarget::File("src/e.md:12".into()));
    assert_eq!(classify("App.tsx:12"), LinkTarget::File("App.tsx:12".into()));
    assert_eq!(classify("file:///tmp/x.txt"), LinkTarget::File("/tmp/x.txt".into()));
    assert_eq!(classify("https://reactrouter.com/x"), LinkTarget::Url("https://reactrouter.com/x".into()));
    assert_eq!(classify("#usage"), LinkTarget::Fragment("usage".into()));
}
