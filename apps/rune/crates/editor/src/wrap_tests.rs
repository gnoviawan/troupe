use crate::wrap::rows;

#[test]
fn a_line_breaks_after_the_last_space_that_fits() {
    assert_eq!(rows("aaaa bbbb cccc".chars(), 5), [0, 5, 10]);
    assert_eq!(rows("aaaa bbbb cccc".chars(), 8), [0, 5, 10]);
    assert_eq!(rows("aaaa bbbb cccc".chars(), 9), [0, 10]);
    assert_eq!(rows("aaaa bbbb cccc".chars(), 10), [0, 10]);
}

#[test]
fn a_word_longer_than_the_row_breaks_inside() {
    assert_eq!(rows("abcdefghij".chars(), 4), [0, 4, 8]);
    assert_eq!(rows("ab abcdefgh".chars(), 4), [0, 3, 7]);
}

#[test]
fn spaces_at_a_row_end_hang() {
    assert_eq!(rows("abcd      efgh".chars(), 4), [0, 10]);
    assert_eq!(rows("".chars(), 4), [0]);
    assert_eq!(rows("short".chars(), 80), [0]);
}
