use super::ids::normalize_path;

#[test]
fn keeps_relative_paths_relative_when_resolving_dot_segments() {
    assert_eq!(normalize_path("./x"), "x");
    assert_eq!(normalize_path("a/../b"), "b");
    assert_eq!(normalize_path("../x"), "../x");
    assert_eq!(normalize_path("a/./b"), "a/b");
    assert_eq!(normalize_path("./a/b/../c"), "a/c");
}

#[test]
fn keeps_rooted_dot_segment_behavior() {
    assert_eq!(normalize_path("/a/../../b"), "/b");
    assert_eq!(normalize_path("E:/a/./b/../c"), "e:/a/c");
    assert_eq!(normalize_path("//srv/share/../../x"), "//srv/share/x");
}
