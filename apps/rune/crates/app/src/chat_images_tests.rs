use super::data_url_bytes;

#[test]
fn a_base64_data_url_decodes_and_other_sources_do_not() {
    assert_eq!(data_url_bytes("data:image/png;base64,iVBORw0KGgo="), Some(vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]));
    assert_eq!(data_url_bytes("data:image/svg+xml,<svg/>"), None);
    assert_eq!(data_url_bytes("docs/chart.png"), None);
}
