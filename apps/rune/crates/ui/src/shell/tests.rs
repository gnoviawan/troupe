use super::*;

#[test]
fn project_colours_follow_their_names_and_fall_back_to_gray() {
    let t = theme::tokens("termul", true);
    assert_eq!(project_color(t, "blue"), t.colors.project_blue);
    assert_eq!(project_color(t, "orange"), t.colors.project_orange);
    assert_eq!(project_color(t, "teal"), t.colors.project_gray);
}

#[test]
fn the_mark_is_a_square_icon() {
    assert_eq!(TROUPE_MARK.view, 569.0);
    assert!(TROUPE_MARK.body.contains("currentColor"));
}
