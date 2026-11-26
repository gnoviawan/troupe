use serde_json::json;

use super::js;

#[test]
fn serialized_length_counts_numbers_as_javascript_prints_them() {
    // JSON.stringify([1, 2.5, 'a"b', { k: null }]) is `[1,2.5,"a\"b",{"k":null}]`.
    let value = json!([1.0, 2.5, "a\"b", { "k": null }]);
    assert_eq!(js::serialized_len(Some(&value)), 25);
    assert_eq!(js::serialized_len(None), 0);
}

#[test]
fn lengths_and_cuts_count_utf16_units() {
    assert_eq!(js::len("a😀"), 3);
    assert_eq!(js::head("a😀b", 3), "a😀");
    assert_eq!(js::head("a😀b", 2), "a");
}

#[test]
fn truthiness_follows_javascript() {
    for falsy in [json!(null), json!(false), json!(0), json!(0.0), json!("")] {
        assert!(!js::truthy(&falsy), "{falsy} is falsy");
    }
    for truthy in [json!(true), json!(1), json!("x"), json!([]), json!({})] {
        assert!(js::truthy(&truthy), "{truthy} is truthy");
    }
}
