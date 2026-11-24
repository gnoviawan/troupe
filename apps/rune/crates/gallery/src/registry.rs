//! The scene table: one row per `design-system` registry entry, in the registry's order, with
//! its frame size. `registry.json` (written by `apps/rune/tools/dump-registry.ts`) is the web
//! side's list; a test keeps the two equal.

use rune::El;

use crate::{Ctx, State, components as c, foundations as f, overlays as o, patterns as p};

pub struct Scene {
    pub id: &'static str,
    pub name: &'static str,
    pub width: f32,
    pub height: f32,
    pub build: fn(&Ctx) -> El,
}

macro_rules! scenes {
    ($($id:literal, $name:literal, $w:literal x $h:literal => $build:expr;)*) => {
        pub static SCENES: &[Scene] = &[$(Scene { id: $id, name: $name, width: $w, height: $h, build: $build }),*];
        /// `AppSpec::scenes`: picking one shows that scene alone.
        pub static SETTERS: &[(&str, fn(&mut State))] = &[$(($id, |s: &mut State| s.show($id))),*];
    };
}

scenes! {
    "color.surface", "Surface tokens", 480.0 x 600.0 => f::surface;
    "color.text", "Text tokens", 480.0 x 390.0 => f::text_tokens;
    "color.accent", "Accent tokens", 480.0 x 442.0 => f::accent;
    "color.status", "Status tokens", 480.0 x 495.0 => f::status;
    "color.brand", "Brand scale", 560.0 x 316.0 => f::brand;
    "color.line", "Line and focus tokens", 480.0 x 232.0 => f::line;
    "color.diff", "Diff tokens", 480.0 x 390.0 => f::diff;
    "color.sidebar", "Sidebar tokens", 480.0 x 442.0 => f::sidebar;
    "color.project", "Project tokens", 480.0 x 495.0 => f::project;
    "color.status-bar", "Status bar tokens", 480.0 x 547.0 => f::status_bar;
    "themes.palettes", "Bundled themes", 720.0 x 784.0 => f::palettes;
    "themes.syntax", "Syntax colors", 560.0 x 348.0 => f::syntax;
    "type.scale", "Type scale", 640.0 x 380.0 => f::type_scale;
    "type.families", "Font families", 560.0 x 252.0 => f::families;
    "type.labels", "Label roles", 480.0 x 192.0 => f::labels;
    "space.scale", "Spacing scale", 520.0 x 376.0 => f::spacing;
    "space.radius", "Radius scale", 560.0 x 196.0 => f::radius;
    "space.shadow", "Shadows", 560.0 x 304.0 => f::shadow;
    "space.elevation", "Elevation ladder", 600.0 x 410.0 => f::elevation;
    "motion.durations", "Durations", 560.0 x 276.0 => f::durations;
    "motion.easings", "Easings", 640.0 x 304.0 => f::easings;
    "motion.tokens", "Distance, scale, blur", 480.0 x 228.0 => f::motion_tokens;
    "icons.grid", "Icon set", 880.0 x 1248.0 => f::icons;
    "button.variants", "Button variants", 560.0 x 456.0 => c::button_variants;
    "button.sizes", "Button sizes", 560.0 x 356.0 => c::button_sizes;
    "button.states", "Button states", 560.0 x 196.0 => c::button_states;
    "toggle", "Toggle and Switch", 520.0 x 288.0 => c::toggles;
    "switch.states", "Switch states", 560.0 x 292.0 => c::switch_states;
    "button.emboss", "Emboss and fill states", 560.0 x 260.0 => c::button_emboss;
    "icon-actions", "Icon actions", 480.0 x 144.0 => c::icon_actions;
    "badges", "Badges and counts", 520.0 x 184.0 => c::badges;
    "loading", "Loading", 520.0 x 236.0 => c::loading;
    "text-motion", "Text motion", 480.0 x 156.0 => c::text_motion;
    "write-gate", "Write gate", 480.0 x 188.0 => c::write_gates;
    "text-fields", "Input and Textarea", 520.0 x 300.0 => c::text_fields;
    "select", "Select", 480.0 x 196.0 => o::select;
    "panel-styles", "Panel styles", 560.0 x 272.0 => c::panel_styles;
    "accordion", "Accordion", 480.0 x 340.0 => c::accordion;
    "collapsible", "Collapsible", 480.0 x 304.0 => c::collapsible;
    "floating", "Tooltip, HoverCard, Popover, Lightbox", 560.0 x 252.0 => o::floating;
    "scroll-area", "ScrollArea", 420.0 x 252.0 => c::scroll_area;
    "resizable", "Resizable panels", 560.0 x 268.0 => c::resizable;
    "bubble", "Bubble variants", 520.0 x 384.0 => c::bubbles;
    "marker", "Marker variants", 480.0 x 132.0 => c::markers;
    "message", "Message scroller", 560.0 x 432.0 => c::messages;
    "chat.status-rail", "Status rail in a chat", 760.0 x 420.0 => p::status_rail;
    "chat.status-rail.states", "Status rail segment states", 320.0 x 200.0 => p::status_rail_states;
    "chat.model-selector.v1", "Model selector v1 (current)", 720.0 x 520.0 => p::model_selector;
    "chat.model-selector.v2", "Model selector v2 (flag)", 1120.0 x 680.0 => p::model_selector;
    "chat.model-selector.v2-rail", "Model selector v2 rail (flag)", 1200.0 x 680.0 => p::model_selector;
    "chat.model-selector.v2-many-agents", "Model selector v2, 14 agents (flag)", 1200.0 x 680.0 => p::model_selector;
    "chat.model-selector.v2-composer", "Model selector v2 in the chat composer (flag)", 820.0 x 600.0 => p::model_selector;
    "dialog", "Dialog and AlertDialog", 480.0 x 140.0 => o::dialogs;
    "sheet", "Sheet", 480.0 x 116.0 => o::sheets;
    "confirm-dialog", "ConfirmDialog", 480.0 x 120.0 => o::confirms;
    "menus", "DropdownMenu and ContextMenu", 480.0 x 180.0 => o::menus;
    "command", "Command", 520.0 x 372.0 => o::commands;
    "toast", "Toasts", 560.0 x 216.0 => o::toasts;
    "status", "Status", 560.0 x 220.0 => p::surface;
    "connection", "Connection states", 520.0 x 348.0 => p::surface;
    "form", "Settings form", 480.0 x 532.0 => p::surface;
    "project-icon", "ProjectIcon", 480.0 x 220.0 => p::surface;
    "brand-mark", "TermulMark", 420.0 x 192.0 => p::surface;
    "sidebar-rows", "Sidebar project rows", 320.0 x 276.0 => p::surface;
    "status-bar", "Status bar anatomy", 680.0 x 272.0 => p::surface;
}

pub fn find(id: &str) -> Option<&'static Scene> {
    SCENES.iter().find(|s| s.id == id)
}
