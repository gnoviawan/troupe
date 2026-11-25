use super::{clamp_tab_zoom, MAX_TAB_ZOOM, MIN_TAB_ZOOM};

#[test]
fn clamp_tab_zoom_respects_bounds_and_rounds() {
    assert_eq!(clamp_tab_zoom(0.1), MIN_TAB_ZOOM);
    assert_eq!(clamp_tab_zoom(5.0), MAX_TAB_ZOOM);
    assert_eq!(clamp_tab_zoom(1.254), 1.25);
    assert_eq!(clamp_tab_zoom(0.9), 0.9);
    assert_eq!(clamp_tab_zoom(f64::NAN), 1.0);
    assert_eq!(clamp_tab_zoom(f64::INFINITY), 1.0);
    assert_eq!(clamp_tab_zoom(f64::NEG_INFINITY), 1.0);
}
