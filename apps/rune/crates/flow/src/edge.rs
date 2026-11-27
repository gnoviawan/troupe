//! The curve an edge draws from an output port on a card's right edge to an input port on
//! another card's left edge: React Flow's `getBezierPath` with its default curvature.

/// A cubic Bézier: start, two control points, end.
pub type Curve = [[f32; 2]; 4];

const CURVATURE: f32 = 0.25;
/// Points per curve when it is flattened for drawing.
const SEGMENTS: usize = 32;

/// How far a control point reaches out of its port (`calculateControlOffset`).
fn control_offset(distance: f32) -> f32 {
    if distance >= 0.0 {
        0.5 * distance
    } else {
        CURVATURE * 25.0 * (-distance).sqrt()
    }
}

/// The curve from a right-facing `from` port to a left-facing `to` port.
pub fn bezier(from: [f32; 2], to: [f32; 2]) -> Curve {
    let reach = control_offset(to[0] - from[0]);
    [from, [from[0] + reach, from[1]], [to[0] - reach, to[1]], to]
}

pub fn point(c: &Curve, t: f32) -> [f32; 2] {
    let u = 1.0 - t;
    let (a, b, cc, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    [
        a * c[0][0] + b * c[1][0] + cc * c[2][0] + d * c[3][0],
        a * c[0][1] + b * c[1][1] + cc * c[2][1] + d * c[3][1],
    ]
}

/// Where an edge's label and its "+" sit: the curve at its middle.
pub fn midpoint(c: &Curve) -> [f32; 2] {
    point(c, 0.5)
}

/// The curve as a polyline.
pub fn flatten(c: &Curve) -> Vec<[f32; 2]> {
    (0..=SEGMENTS)
        .map(|i| point(c, i as f32 / SEGMENTS as f32))
        .collect()
}

/// The most dash steps one line takes. A line far from the origin (a card at x = 1e9) loses
/// the precision to step at all; past this the rest of the line is not drawn.
const MAX_DASH_STEPS: usize = 4096;

/// The pieces of `line` a dash pattern of `on` then `off` points draws.
pub fn dashes(line: &[[f32; 2]], on: f32, off: f32) -> Vec<Vec<[f32; 2]>> {
    let mut out: Vec<Vec<[f32; 2]>> = Vec::new();
    let mut current: Vec<[f32; 2]> = Vec::new();
    let mut drawing = true;
    let mut left = on;
    let mut steps = 0;
    if !(on > 0.0 && off >= 0.0) {
        return out;
    }
    for pair in line.windows(2) {
        let (mut a, b) = (pair[0], pair[1]);
        let mut length = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
        while length > 0.0 && length.is_finite() {
            steps += 1;
            if steps > MAX_DASH_STEPS {
                return out;
            }
            let step = left.min(length);
            let t = step / length;
            let next = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
            if drawing {
                if current.is_empty() {
                    current.push(a);
                }
                current.push(next);
            }
            left -= step;
            length -= step;
            a = next;
            if left <= 0.0 {
                if drawing && current.len() >= 2 {
                    out.push(std::mem::take(&mut current));
                }
                current.clear();
                drawing = !drawing;
                left = if drawing { on } else { off };
            }
        }
    }
    if drawing && current.len() >= 2 {
        out.push(current);
    }
    out
}

/// How far `p` is from the flattened curve, for picking an edge under the pointer.
pub fn distance(c: &Curve, p: [f32; 2]) -> f32 {
    flatten(c)
        .windows(2)
        .map(|s| segment_distance(s[0], s[1], p))
        .fold(f32::INFINITY, f32::min)
}

fn segment_distance(a: [f32; 2], b: [f32; 2], p: [f32; 2]) -> f32 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 {
        0.0
    } else {
        (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2).clamp(0.0, 1.0)
    };
    let (x, y) = (a[0] + dx * t, a[1] + dy * t);
    ((p[0] - x).powi(2) + (p[1] - y).powi(2)).sqrt()
}

#[cfg(test)]
#[path = "edge_tests.rs"]
mod tests;
