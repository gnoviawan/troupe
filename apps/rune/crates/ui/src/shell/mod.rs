//! The workspace shell's parts (`components/shell/`): the rail, the main area's top row, the
//! side panel and the tool strip, at the redesign's metrics (the browser's computed rects of
//! the web client at 1280x800). Each builder returns the element without handlers; the app
//! adds keys, presses and drags.
//!
//! The desktop frame is rail | main area | side panel | tool strip, edge to edge on the
//! background surface, split by hairlines. The rail header, the main area's top row and the
//! side panel header share one 44 px top row.

mod parts;
mod rail;

pub use parts::{
    TROUPE_MARK, connection_lamp, launcher_heading, panel_header, panel_skeleton, resize_handle,
    tool_button, top_row,
};
pub use rail::{
    nav_row, project_badge, project_color, project_row, rail_button, rail_header, section_header,
};

/// Height of the shared top row (`h-11`).
pub const TOP_ROW: f32 = 44.0;
/// Width of the tool strip on the window's right edge.
pub const TOOL_STRIP: f32 = 44.0;
/// Width of the collapsed rail (`w-14`): the icon column sits at x = 28 in both densities.
pub const COLLAPSED_RAIL: f32 = 56.0;
/// Below this window width the web client shows its phone shell (`MOBILE_WEB_SHELL_MAX_PX`).
pub const PHONE_MAX_WIDTH: f32 = 767.0;

#[cfg(test)]
mod tests;
