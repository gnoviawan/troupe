//! The terminal pane's context menu (`ConnectedTerminal`'s `ContextMenuContent`): Copy, Paste,
//! Select All, Explain with AI, Fix Command with AI and Restart Terminal. The two AI rows open
//! the inline assist card (`TerminalAssistPanel`) at the pane's bottom right; its suggested
//! commands are inserted at the prompt for review, never run.

use std::sync::LazyLock;

use model::workspace::{Event as WorkspaceEvent, Tab};
use rune::{El, PointerPress, Tx, col, icon, row, text};
use theme::Tokens;
use ui::overlay::{Menu, MenuItem};
use ui::type_::{mono, sans};

use crate::State;

pub const MENU: &str = "terminal-menu";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssistKind {
    Explain,
    Fix,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AssistStatus {
    Loading,
    Done(String),
    Error(String),
}

/// The assist card over one terminal (`TerminalAssistPanelState`).
#[derive(Clone, Debug, PartialEq)]
pub struct Assist {
    pub terminal: String,
    pub kind: AssistKind,
    pub status: AssistStatus,
}

/// What the Rune client says while it cannot run the one-shot agent session React's
/// `assistTerminal` opens (`model::acp::oneshot` has the run; the app does not drive it yet).
pub const ASSIST_UNAVAILABLE: &str = "Terminal assist is not available in the Rune client yet";

static FENCED: LazyLock<regex_lite::Regex> =
    LazyLock::new(|| regex_lite::Regex::new(r"```(?:sh|shell|bash)?\s*\n((?s:.)*?)```").expect("valid pattern"));

/// `extractSuggestedCommands`: the bodies of the reply's `sh`, `shell`, `bash` or bare fenced
/// blocks that are one line with no control character, so inserting one cannot run it.
pub fn suggested_commands(markdown: &str) -> Vec<String> {
    FENCED
        .captures_iter(markdown)
        .filter_map(|c| c.get(1).map(|m| m.as_str().trim().to_owned()))
        .filter(|body| !body.is_empty() && !body.chars().any(|ch| (ch as u32) < 0x20 || ch == '\u{7f}'))
        .collect()
}

/// Whether `text` may go to a PTY as typed text: no C0 or C1 control, no DEL, and no Unicode
/// format character (bidi overrides, zero-width), as React's foreign-insert sanitizer refuses.
fn insertable(text: &str) -> bool {
    !text.chars().any(|c| {
        c.is_control()
            || matches!(c, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2060}'..='\u{2069}' | '\u{feff}' | '\u{061c}')
    })
}

/// `SHORTCUT_MOD`.
fn shortcut(letter: char) -> String {
    let m = if rune::KeyPlatform::current() == rune::KeyPlatform::Mac { "⌘" } else { "Ctrl" };
    format!("{m}+{letter}")
}

fn glyph(name: &str) -> &'static rune::Icon {
    theme::icons::named(name).unwrap_or(&theme::icons::X)
}

/// The open context menu, pushed onto `layer`.
pub fn build(s: &State, t: &'static Tokens, layer: &mut Vec<El>) {
    let Some((id, at)) = s.terms.menu.clone().filter(|_| s.overlays.is_open(MENU)) else { return };
    let selected = s.terminal_selection_text(&id).is_some();
    let item = |label: &str, run: fn(&mut State, &str, &mut Tx)| {
        let id = id.clone();
        MenuItem::new(label.to_owned()).on_select_tx(move |s: &mut State, tx: &mut Tx| {
            s.close(MENU);
            run(s, &id, tx);
        })
    };
    let menu = Menu::new(MENU)
        .label("Terminal")
        .width(160.0)
        .at(Some(at))
        .item(item("Copy", |s, id, tx| {
            s.terminal_copy(id, tx);
        })
        .shortcut(shortcut('C'))
        .disabled(!selected))
        .item(item("Paste", |s, id, tx| s.terminal_paste_clipboard(id, tx)).shortcut(shortcut('V')))
        .separator()
        .item(item("Select All", |s, id, _| s.terminal_select_all(id)).shortcut(shortcut('A')))
        .separator()
        .item(item("Explain with AI", |s, id, _| s.terminal_assist(id, AssistKind::Explain)).disabled(!selected))
        .item(item("Fix Command with AI", |s, id, _| s.terminal_assist(id, AssistKind::Fix)).disabled(!selected))
        .separator()
        .item(item("Restart Terminal", |s, id, _| s.restart_terminal(id)).icon(glyph("RefreshCcw")))
        .on_close(|s: &mut State| s.close(MENU));
    let _ = menu.build(t, layer, rune::el());
}

/// The assist card over terminal `id`'s pane, when it shows one.
pub fn assist_card(s: &State, t: &'static Tokens, id: &str) -> Option<El> {
    let assist = s.terms.assist.as_ref().filter(|a| a.terminal == id)?;
    let c = &t.colors;
    let title = match assist.kind {
        AssistKind::Fix => "Fix command",
        AssistKind::Explain => "Explain output",
    };
    let header = row()
        .w_full()
        .px(12.0)
        .py(8.0)
        .gap(8.0)
        .items_center()
        .border_sides([0.0, 0.0, 1.0, 0.0], c.border.alpha(0.6))
        .child(icon(glyph("Sparkles"), 14.0).color(c.muted_foreground).a11y_hidden())
        .child(text(title).font(sans(12.0, 500.0, 16.0)).color(c.foreground).grow(1.0))
        .child(
            row()
                .key(format!("terminal-assist-close-{id}"))
                .role("button")
                .label("Close terminal assist")
                .padding(4.0)
                .radius(4.0)
                .hover(c.accent)
                .child(icon(glyph("X"), 14.0).color(c.muted_foreground))
                .on_press(|s: &mut State| s.terms.assist = None),
        );
    let body = match &assist.status {
        AssistStatus::Loading => text("Asking the configured agent…")
            .font(sans(12.0, 400.0, 16.0))
            .color(c.muted_foreground)
            .role("status"),
        AssistStatus::Error(message) => {
            text(message.clone()).font(sans(12.0, 400.0, 16.0)).color(c.destructive).role("alert")
        }
        AssistStatus::Done(reply) => {
            let hooks = ui::markdown::Hooks {
                on_link: std::rc::Rc::new(|s: &mut State, target, _tx: &mut Tx| {
                    if let ui::markdown::LinkTarget::Url(url) = target {
                        s.request_link_open(&url);
                    }
                }),
                ..ui::markdown::Hooks::default()
            };
            ui::markdown::prose(t, &format!("terminal-assist-reply-{id}"), reply, false, &hooks)
        }
    };
    let mut card = col()
        .key(format!("terminal-assist-{id}"))
        .role("complementary")
        .label(format!("Terminal AI assist — {title}"))
        .w(420.0)
        .radius(8.0)
        .clip_rounded(8.0)
        .fill(c.card)
        .border(1.0, c.border.alpha(0.7))
        .child(header)
        .child(col().w_full().max_h(288.0).px(12.0).py(8.0).child(body).scroll(rune::ScrollSpec { stick_bottom: false }));
    let commands = match &assist.status {
        AssistStatus::Done(reply) => suggested_commands(reply),
        _ => Vec::new(),
    };
    if !commands.is_empty() {
        let mut list = col().w_full().px(12.0).py(8.0).gap(4.0).border_sides([1.0, 0.0, 0.0, 0.0], c.border.alpha(0.6)).child(
            text("Suggested commands — inserted for review, never run")
                .font(sans(12.0, 500.0, 16.0))
                .color(c.muted_foreground),
        );
        for (i, command) in commands.into_iter().enumerate() {
            let (terminal, insert) = (id.to_owned(), command.clone());
            list = list.child(
                row()
                    .key(format!("terminal-assist-command-{id}-{i}"))
                    .role("button")
                    .label(format!("Insert suggested command {}", i + 1))
                    .w_full()
                    .px(8.0)
                    .py(4.0)
                    .radius(4.0)
                    .border(1.0, c.border.alpha(0.6))
                    .hover(c.accent)
                    .child(text(command).font(mono(11.0, 400.0, 16.0)).color(c.foreground).truncate())
                    .on_press(move |s: &mut State| s.insert_assist_command(&terminal, &insert)),
            );
        }
        card = card.child(list);
    }
    Some(ui::paint::shadows(card, &t.shadows.elevation_lg))
}

impl State {
    /// A secondary press on terminal `id`: its context menu opens at the pointer.
    pub(crate) fn open_terminal_menu(&mut self, id: &str, at: PointerPress) {
        self.terms.focused = Some(id.to_owned());
        self.terms.menu = Some((id.to_owned(), at));
        self.overlays.open(MENU);
    }

    /// "Explain with AI" or "Fix Command with AI" (`runTerminalAssist`).
    pub(crate) fn terminal_assist(&mut self, id: &str, kind: AssistKind) {
        let selection = self.terminal_selection_text(id).map(|t| t.trim().to_owned()).unwrap_or_default();
        let status = if selection.is_empty() {
            AssistStatus::Error("Select some terminal output first".into())
        } else {
            log::warn!("[term] terminal assist ({kind:?}) asked for {id}: {ASSIST_UNAVAILABLE}");
            AssistStatus::Error(ASSIST_UNAVAILABLE.into())
        };
        self.terms.assist = Some(Assist { terminal: id.to_owned(), kind, status });
    }

    /// Closes the assist card (Escape, ✕). False when none was open.
    pub(crate) fn close_terminal_assist(&mut self) -> bool {
        self.terms.assist.take().is_some()
    }

    /// An assist card's Insert: the command lands at the prompt with no Return.
    pub(crate) fn insert_assist_command(&mut self, id: &str, command: &str) {
        if !insertable(command) {
            log::warn!("[term] refused to insert an assist command with control or format characters into {id}");
            return;
        }
        log::info!("[term] inserting an assist command into {id}");
        self.terminal_write(id, command);
    }

    /// "Restart Terminal" (`restartTerminal`): a fresh shell takes the tab's place in its pane
    /// and keeps its name; the old PTY is killed once the tab moves to the new one.
    pub(crate) fn restart_terminal(&mut self, id: &str) {
        let pane = model::workspace::leaves(self.workspace.root())
            .into_iter()
            .find(|leaf| leaf.tabs.iter().any(|tab| matches!(tab, Tab::Terminal { terminal_id, .. } if terminal_id == id)))
            .map(|leaf| leaf.id.clone());
        let Some(pane) = pane else {
            log::warn!("[term] restart asked for {id}, which no pane shows");
            return;
        };
        log::info!("[term] restarting {id}");
        if let Some(call) = self.spawn_terminal(Some(pane)) {
            self.terms.restarts.insert(call, id.to_owned());
        }
    }

    /// The new PTY of a restart arrived: the tab moves to it.
    pub(crate) fn terminal_restarted(&mut self, old: &str, new: &str) {
        let map = [(old.to_owned(), new.to_owned())].into_iter().collect();
        self.workspace.step(WorkspaceEvent::RemapTerminalTabs(map));
        if let Some(name) = self.terms.names.get(old).cloned() {
            self.terms.names.insert(new.to_owned(), name);
        }
        if self.terms.focused.as_deref() == Some(old) {
            self.terms.focused = Some(new.to_owned());
        }
    }
}

#[cfg(test)]
#[path = "term_menu_tests.rs"]
mod tests;
