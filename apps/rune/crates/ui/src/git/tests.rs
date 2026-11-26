use super::*;

#[test]
fn a_path_splits_into_its_name_and_folder() {
    assert_eq!(split_path("src/lib/a.rs"), ("a.rs", "src/lib"));
    assert_eq!(split_path("README.md"), ("README.md", ""));
    assert_eq!(split_path("dir/"), ("dir/", "dir"));
}

#[test]
fn statuses_map_to_the_explorer_letters() {
    assert_eq!(status_letter("untracked"), ("U", "Untracked"));
    assert_eq!(status_letter("staged"), ("M", "Modified"));
    assert_eq!(status_letter("deleted").0, "D");
}
