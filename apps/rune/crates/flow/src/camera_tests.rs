use super::*;

#[test]
fn a_zoom_keeps_the_point_under_the_pointer_still() {
    let cam = Camera::default();
    cam.set([40.0, 10.0], 1.0);
    let under = cam.to_canvas([300.0, 200.0]);
    cam.zoom_about([300.0, 200.0], 1.5);
    assert_eq!(cam.zoom.get(), 1.5);
    let [x, y] = cam.to_view(under);
    assert!(
        (x - 300.0).abs() < 1e-3 && (y - 200.0).abs() < 1e-3,
        "{x} {y}"
    );
}

#[test]
fn zoom_stays_between_the_limits() {
    let cam = Camera::default();
    cam.zoom_about([0.0, 0.0], 100.0);
    assert_eq!(cam.zoom.get(), MAX_ZOOM);
    cam.zoom_about([0.0, 0.0], 0.0001);
    assert_eq!(cam.zoom.get(), MIN_ZOOM);
}

#[test]
fn a_small_graph_starts_fitted_and_centred_at_most_life_size() {
    let cam = Camera::default();
    cam.start([0.0, 0.0, 400.0, 100.0], 32.0, [800.0, 600.0], 0.75);
    assert_eq!(cam.zoom.get(), 1.0);
    assert_eq!(cam.pan.get(), [200.0, 250.0]);
}

#[test]
fn a_wide_graph_starts_readable_from_its_left_edge() {
    let cam = Camera::default();
    cam.start([-10.0, 0.0, 4000.0, 100.0], 32.0, [800.0, 600.0], 0.75);
    assert_eq!(cam.zoom.get(), 0.75);
    assert_eq!(cam.pan.get(), [24.0 + 7.5, 300.0 - 50.0 * 0.75]);
}

#[test]
fn centring_a_card_zooms_out_only_when_it_does_not_fit() {
    let cam = Camera::default();
    cam.centre_on([100.0, 100.0, 192.0, 64.0], 1.0, [800.0, 600.0], 340.0);
    assert_eq!(cam.zoom.get(), 1.0);
    assert_eq!(cam.pan.get(), [230.0 - 196.0, 300.0 - 132.0]);
    assert!(cam.shows([100.0, 100.0, 192.0, 64.0], [800.0, 600.0], 340.0));
    cam.centre_on([0.0, 0.0, 400.0, 64.0], 1.0, [500.0, 600.0], 300.0);
    assert_eq!(cam.zoom.get(), 0.5);
}
