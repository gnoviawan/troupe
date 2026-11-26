use super::*;

#[test]
fn a_forward_edge_bends_halfway_out_of_each_port() {
    assert_eq!(
        bezier([0.0, 0.0], [100.0, 50.0]),
        [[0.0, 0.0], [50.0, 0.0], [50.0, 50.0], [100.0, 50.0]]
    );
}

#[test]
fn a_backward_edge_loops_out_by_the_square_root_rule() {
    // React Flow: 0.25 * 25 * sqrt(100) = 62.5.
    let c = bezier([100.0, 0.0], [0.0, 0.0]);
    assert_eq!(c[1], [162.5, 0.0]);
    assert_eq!(c[2], [-62.5, 0.0]);
}

#[test]
fn the_label_sits_at_the_middle_of_the_curve() {
    assert_eq!(midpoint(&bezier([0.0, 0.0], [100.0, 50.0])), [50.0, 25.0]);
}

#[test]
fn dashes_alternate_on_and_off_along_the_line() {
    let pieces = dashes(&[[0.0, 0.0], [20.0, 0.0]], 5.0, 4.0);
    let spans: Vec<(f32, f32)> = pieces
        .iter()
        .map(|p| (p[0][0], p[p.len() - 1][0]))
        .collect();
    assert_eq!(spans, [(0.0, 5.0), (9.0, 14.0), (18.0, 20.0)]);
}

#[test]
fn the_distance_to_a_curve_is_zero_on_it() {
    let c = bezier([0.0, 0.0], [100.0, 0.0]);
    assert!(distance(&c, [50.0, 0.0]) < 1e-3);
    assert!((distance(&c, [50.0, 10.0]) - 10.0).abs() < 1e-3);
}
