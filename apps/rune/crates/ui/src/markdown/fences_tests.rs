use super::fences::*;

#[test]
fn a_complete_plan_fence_leaves_the_reply() {
    let text = "Plan below.\n```termul-plan\n{\"entries\":[]}\n```\nDone.";
    assert_eq!(strip_plan_fences(text), "Plan below.\nDone.");
    assert_eq!(strip_plan_fences("```termul-plan\n[]\n```\nNext"), "Next");
}

#[test]
fn an_open_plan_fence_and_a_longer_info_string_stay() {
    assert_eq!(strip_plan_fences("a\n```termul-plan\n[1,"), "a\n```termul-plan\n[1,");
    assert_eq!(strip_plan_fences("```termul-plan-v2\nx\n```"), "```termul-plan-v2\nx\n```");
}

#[test]
fn empty_fences_go_and_an_open_one_waits_for_the_stream_to_end() {
    assert_eq!(strip_empty_fences("a\n```ts\n\n```\nb", false), "a\nb");
    assert_eq!(strip_empty_fences("a\n```ts\nx\n```", false), "a\n```ts\nx\n```");
    assert_eq!(strip_empty_fences("a\n```ts\n", true), "a\n```ts\n");
    assert_eq!(strip_empty_fences("a\n```ts\n", false), "a");
    assert_eq!(strip_empty_fences("````\n```\n````", false), "````\n```\n````", "a shorter inner fence is content");
}

#[test]
fn a_glued_plan_opener_moves_to_its_own_line() {
    assert_eq!(normalize_plan_fence_boundary("prose```termul-plan\n[]"), "prose\n```termul-plan\n[]");
    assert_eq!(normalize_plan_fence_boundary("  ```termul-plan\n"), "  ```termul-plan\n");
    assert_eq!(normalize_plan_fence_boundary("x```termul-plan-v2"), "x```termul-plan-v2");
}

#[test]
fn agent_prose_runs_all_three() {
    let raw = "A\n```termul-plan\n[]\n```\n```sh\n\n```\nB```termul-plan\n[";
    assert_eq!(agent_prose(raw, true), "A\nB\n```termul-plan\n[");
}
