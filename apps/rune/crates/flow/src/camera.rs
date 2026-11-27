//! The canvas camera: a pan in view points and a zoom, held in Rune `Live` values. The canvas
//! binds them with `live_offset` and `live_scale`, so a pan or a zoom draws a new frame
//! without running the view or laying it out. A view point is `pan + zoom * canvas point`.

use rune::Live;

/// React Flow's `minZoom` and `maxZoom` on the automation canvas.
pub const MIN_ZOOM: f32 = 0.2;
pub const MAX_ZOOM: f32 = 2.0;
/// Room left around the content when it is fitted.
const MARGIN: f32 = 24.0;

#[derive(Clone)]
pub struct Camera {
    pub pan: Live<[f32; 2]>,
    pub zoom: Live<f32>,
}

impl Default for Camera {
    fn default() -> Camera {
        Camera {
            pan: Live::new([0.0, 0.0]),
            zoom: Live::new(1.0),
        }
    }
}

impl std::fmt::Debug for Camera {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Camera")
            .field("pan", &self.pan.get())
            .field("zoom", &self.zoom.get())
            .finish()
    }
}

impl Camera {
    pub fn set(&self, pan: [f32; 2], zoom: f32) {
        self.zoom.set(zoom.clamp(MIN_ZOOM, MAX_ZOOM));
        self.pan.set(pan);
    }

    pub fn pan_by(&self, dx: f32, dy: f32) {
        let [x, y] = self.pan.get();
        self.pan.set([x + dx, y + dy]);
    }

    /// Zooms by `factor` about `at` (view points), which stays over the same canvas point.
    pub fn zoom_about(&self, at: [f32; 2], factor: f32) {
        let zoom = self.zoom.get();
        let next = (zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let world = self.to_canvas(at);
        self.zoom.set(next);
        self.pan
            .set([at[0] - world[0] * next, at[1] - world[1] * next]);
    }

    /// The canvas point under the view point `at`.
    pub fn to_canvas(&self, at: [f32; 2]) -> [f32; 2] {
        let ([px, py], zoom) = (self.pan.get(), self.zoom.get());
        [(at[0] - px) / zoom, (at[1] - py) / zoom]
    }

    /// The view point over the canvas point `p`.
    pub fn to_view(&self, p: [f32; 2]) -> [f32; 2] {
        let ([px, py], zoom) = (self.pan.get(), self.zoom.get());
        [px + p[0] * zoom, py + p[1] * zoom]
    }

    /// The first view (`startViewport`): all of `bounds` when it fits at `readable` zoom or
    /// more (never above 1), else `readable` zoom with the content's left edge in view, centred
    /// across on the content when it fits that way, else on `anchor_y`.
    pub fn start(&self, bounds: [f32; 4], anchor_y: f32, size: [f32; 2], readable: f32) {
        let [x, y, w, h] = bounds;
        let fit = 1.0f32
            .min((size[0] - 2.0 * MARGIN) / w)
            .min((size[1] - 2.0 * MARGIN) / h);
        if fit >= readable {
            self.set(
                [
                    (size[0] - w * fit) / 2.0 - x * fit,
                    (size[1] - h * fit) / 2.0 - y * fit,
                ],
                fit,
            );
            return;
        }
        let focus = if h * readable <= size[1] - 2.0 * MARGIN {
            y + h / 2.0
        } else {
            anchor_y
        };
        self.set(
            [MARGIN - x * readable, size[1] / 2.0 - focus * readable],
            readable,
        );
    }

    /// Centres `rect` (canvas points) in the view left of `right_inset`, zooming out when it
    /// does not fit across at `zoom` (`centreOn`).
    pub fn centre_on(&self, rect: [f32; 4], zoom: f32, size: [f32; 2], right_inset: f32) {
        let free = size[0] - right_inset;
        let fit = if free > 0.0 {
            zoom.min(free / rect[2])
        } else {
            zoom
        };
        self.set(
            [
                free / 2.0 - (rect[0] + rect[2] / 2.0) * fit,
                size[1] / 2.0 - (rect[1] + rect[3] / 2.0) * fit,
            ],
            fit,
        );
    }

    /// Whether `rect` shows whole in the view left of `right_inset`.
    pub fn shows(&self, rect: [f32; 4], size: [f32; 2], right_inset: f32) -> bool {
        let [l, t] = self.to_view([rect[0], rect[1]]);
        let [r, b] = self.to_view([rect[0] + rect[2], rect[1] + rect[3]]);
        l >= 0.0 && t >= 0.0 && r <= size[0] - right_inset && b <= size[1]
    }
}

#[cfg(test)]
#[path = "camera_tests.rs"]
mod tests;
