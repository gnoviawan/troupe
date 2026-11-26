use super::{APP_PAGES, Owner, PROJECT_PAGES, Page, resolve_app_page};

/// The `id: '...'` of each page entry in a `categories.tsx`, in file order.
fn ids_in(file: &str) -> Vec<String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../src/renderer/pages")
        .join(file);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let list = text.split("SettingsCategory[] = [").nth(1).expect("the page list");
    let list = list.split("\n]").next().expect("its end");
    list.lines()
        .filter_map(|line| line.trim().strip_prefix("id: '"))
        .map(|rest| rest.split('\'').next().unwrap_or_default().to_owned())
        .collect()
}

fn listed(pages: &[Page]) -> Vec<String> {
    pages.iter().map(|p| p.id.to_owned()).collect()
}

#[test]
fn every_app_page_has_a_view_or_a_later_owner() {
    assert_eq!(listed(&APP_PAGES), ids_in("app-preferences/categories.tsx"));
}

#[test]
fn every_project_page_has_a_view_or_a_later_owner() {
    assert_eq!(listed(&PROJECT_PAGES), ids_in("project-settings/categories.tsx"));
}

#[test]
fn the_browser_pages_wait_for_r11() {
    for pages in [&APP_PAGES[..], &PROJECT_PAGES[..]] {
        let browser = pages.iter().find(|p| p.id == "browser").expect("a browser page");
        assert_eq!(browser.owner, Owner::Later("R11"));
    }
}

#[test]
fn old_page_ids_open_their_new_page() {
    assert_eq!(resolve_app_page("updates"), "about");
    assert_eq!(resolve_app_page("shell"), "terminal");
    assert_eq!(resolve_app_page("skills"), "skills");
}
