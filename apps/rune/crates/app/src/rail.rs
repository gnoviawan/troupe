//! What the rail's overlays hold while open, and the one place an overlay closes. Each overlay
//! is a key on the back stack ([`Overlays`](ui::overlay::Overlays)); the project, chat or group
//! it acts on and the text its fields hold live here, so a back press, Escape, the scrim and a
//! Cancel button all close it the same way.

use rune::{PointerPress, TextEdit};

use crate::State;

/// The project row's context menu (`ProjectContextMenuContent`).
pub const PROJECT_MENU: &str = "project-menu";
/// Its "Move to group" submenu.
pub const MOVE_MENU: &str = "project-move-menu";
/// Its "Default shell" submenu; the detected shells list ports with the terminal (R7).
pub const SHELL_MENU: &str = "project-shell-menu";
/// The colour swatches (`ColorPickerPopover`).
pub const COLOUR: &str = "colour-picker";
pub const DELETE_PROJECT: &str = "delete-project";
/// The Projects header's `+` menu (`AddProjectMenu`).
pub const ADD_MENU: &str = "add-project-menu";
pub const NEW_PROJECT: &str = "new-project";
pub const NEW_GROUP: &str = "new-group";
/// A group row's context menu (`GroupContextMenuContent`).
pub const GROUP_MENU: &str = "group-menu";
pub const DELETE_GROUP: &str = "delete-group";
/// A chat row's "More actions" menu (`RailChatMenu`).
pub const CHAT_MENU: &str = "chat-menu";
pub const DELETE_CHAT: &str = "delete-chat";

/// The swatches `ColorPickerPopover` offers, in its order (`availableColors`).
pub const COLOURS: [&str; 9] = [
    "blue", "purple", "pink", "red", "orange", "yellow", "green", "cyan", "gray",
];

/// What the colour picker recolours.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Paint {
    Project(String),
    Group(String),
}

/// The inline rename field on a project or group row.
#[derive(Clone, Debug)]
pub struct Rename {
    pub target: Paint,
    pub edit: TextEdit,
}

#[derive(Default)]
pub struct RailUi {
    /// The project a menu, the colour picker or the delete confirm acts on.
    pub project: Option<String>,
    /// The group a group menu or the group delete confirm acts on; the bool asks to delete
    /// its projects too.
    pub group: Option<(String, bool)>,
    /// The chat a chat menu or the chat delete confirm acts on.
    pub chat: Option<String>,
    /// Where the secondary press opened the context menu; the colour picker opens there too.
    pub at: Option<PointerPress>,
    pub paint: Option<Paint>,
    pub rename: Option<Rename>,
    pub project_path: TextEdit,
    pub project_name: TextEdit,
    pub project_colour: Option<&'static str>,
    pub group_name: TextEdit,
    /// The project "Create new group…" files into the new group.
    pub group_for: Option<String>,
    /// The chat row under the pointer: it shows its "More actions" button.
    pub hovered_chat: Option<String>,
}

impl State {
    /// Closes `key` and what opened above it, and drops what the closed overlays held.
    pub fn close(&mut self, key: &str) {
        for closed in self.overlays.close(key) {
            self.forget(&closed);
        }
    }

    /// Closes the top overlay: what Escape and a back press do. False when none was open.
    pub fn close_top(&mut self) -> bool {
        let Some(top) = self.overlays.pop() else {
            return false;
        };
        self.forget(&top);
        true
    }

    fn forget(&mut self, key: &str) {
        let r = &mut self.rail;
        match key {
            PROJECT_MENU | MOVE_MENU | SHELL_MENU => r.at = r.at.filter(|_| self.overlays.is_open(COLOUR)),
            COLOUR => r.paint = None,
            NEW_PROJECT => {
                r.project_path = TextEdit::new("");
                r.project_name = TextEdit::new("");
                r.project_colour = None;
            }
            NEW_GROUP => {
                r.group_name = TextEdit::new("");
                r.group_for = None;
            }
            GROUP_MENU | DELETE_GROUP => r.group = None,
            CHAT_MENU | DELETE_CHAT => r.chat = None,
            // A closed sheet drops the pick a busy chat held, as the desktop menu does.
            crate::model_sheet::MODEL_SHEET => {
                if let Some(host) = self.composer.sheet_host.clone() {
                    self.draft_mut(&host).switch_confirm = None;
                }
            }
            crate::theme_picker::KEY => self.picker_closed(),
            crate::keys::MENU | crate::settings_view::APP => self.keys.recording = None,
            crate::settings_view::SELECT => self.settings_ui.select = None,
            crate::phone::MORE | crate::phone::PROJECTS | crate::phone::RECENTS => {
                self.phone.drag = ui::overlay::SheetDrag::default();
                if key == crate::phone::RECENTS {
                    self.phone.chat = None;
                }
            }
            _ => {}
        }
    }

    /// Opens `key` on top, first closing the menu that chose it.
    pub fn open_from_menu(&mut self, menu: &str, key: &str) {
        let at = self.rail.at;
        self.close(menu);
        self.rail.at = at;
        self.overlays.open(key);
    }
}
