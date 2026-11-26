//! Where a dragged tab would land. The web client hit-tests five invisible zones over each
//! pane's content (`DropZoneOverlay`) and each tab's halves (`computeTabPosition`); here the
//! view records the same rects from the last layout and the drag reads them.

use model::workspace::Position;

/// Which half of a tab the pointer is over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Before,
    After,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DropTarget {
    /// A pane's content: an edge splits it, the centre moves the tab into it.
    Zone { pane: String, position: Position },
    /// A pane's tab bar, over one of its tabs (`at`) or its empty space.
    Bar {
        pane: String,
        at: Option<(String, Side)>,
    },
}

/// The rects (`[x, y, w, h]` in window points) the drag tests, from the view's last layout.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Geometry {
    pub contents: Vec<(String, [f32; 4])>,
    pub bars: Vec<(String, [f32; 4])>,
    /// `(pane, tab, rect)` for every tab on screen.
    pub tabs: Vec<(String, String, [f32; 4])>,
}

fn inside([x, y, w, h]: [f32; 4], [px, py]: [f32; 2]) -> bool {
    px >= x && px < x + w && py >= y && py < y + h
}

/// The zone of a pane's content under `point`: a quarter of the width on each side, a
/// quarter of the height above and below the middle half, the centre otherwise.
#[must_use]
pub fn zone_at([x, y, w, h]: [f32; 4], [px, py]: [f32; 2]) -> Position {
    let (fx, fy) = ((px - x) / w, (py - y) / h);
    if fx < 0.25 {
        Position::Left
    } else if fx >= 0.75 {
        Position::Right
    } else if fy < 0.25 {
        Position::Top
    } else if fy >= 0.75 {
        Position::Bottom
    } else {
        Position::Center
    }
}

/// The highlight of a zone as `[left, top, right, bottom]` fractions of the pane
/// (`ZONE_RECT`): an edge zone is the quarter it splits off, the centre the middle half.
#[must_use]
pub fn zone_rect(position: Position) -> [f32; 4] {
    match position {
        Position::Left => [0.0, 0.0, 0.25, 1.0],
        Position::Right => [0.75, 0.0, 1.0, 1.0],
        Position::Top => [0.25, 0.0, 0.75, 0.25],
        Position::Bottom => [0.25, 0.75, 0.75, 1.0],
        Position::Center => [0.25, 0.25, 0.75, 0.75],
    }
}

impl Geometry {
    /// The target under `point`, if any. `fullscreen` panes take no zone drops, as the web
    /// client mounts no overlay on them.
    #[must_use]
    pub fn target(&self, point: [f32; 2], fullscreen: Option<&str>) -> Option<DropTarget> {
        if let Some((pane, _)) = self.bars.iter().find(|(_, rect)| inside(*rect, point)) {
            let at = self
                .tabs
                .iter()
                .find(|(owner, _, rect)| owner == pane && inside(*rect, point))
                .map(|(_, tab, [x, _, w, _])| {
                    let side = if point[0] - x < w / 2.0 {
                        Side::Before
                    } else {
                        Side::After
                    };
                    (tab.clone(), side)
                });
            return Some(DropTarget::Bar {
                pane: pane.clone(),
                at,
            });
        }
        self.contents
            .iter()
            .filter(|(pane, _)| Some(pane.as_str()) != fullscreen)
            .find(|(_, rect)| inside(*rect, point))
            .map(|(pane, rect)| DropTarget::Zone {
                pane: pane.clone(),
                position: zone_at(*rect, point),
            })
    }
}
