//! What the composer draws (`ChatInputBar` and the launcher's composer, Composer V2): the tray
//! with its raised card (the attachment row, the field, the toolbar with Add, the agent chip
//! and Send) and the foot row (the working copy and Connectors), with the slash or mention menu
//! over the tray (`composer-menu.tsx`). The host keeps the field's `TextEdit` and decides what
//! each press does through [`Hooks`].

use std::rc::Rc;

use rune::{ClipboardContent, El, KeyEvent, PillStyle, TextEdit, Tx, col, el, icon, row, text, text_input};
use theme::Tokens;
use theme::icons as i;

use super::field::pill_of;
pub use super::foot::Foot;
use super::foot::{phone_strip, worktree};
use super::menu::{Item, Row, Section};
use super::{Attachment, Pill};
use crate::overlay::{Menu, MenuItem, Placement};
use crate::paint::{pad, shadows};
use crate::type_::sans;
use crate::{Button, ButtonSize, ButtonVariant};

type Edit<S> = Rc<dyn for<'a> Fn(&'a mut S) -> &'a mut TextEdit>;
type Act<S> = Rc<dyn Fn(&mut S, &mut Tx)>;
type OnKey<S> = Rc<dyn Fn(&mut S, &KeyEvent, &mut Tx) -> bool>;
type OnPaste<S> = Rc<dyn Fn(&mut S, &ClipboardContent, &mut Tx) -> bool>;
type OnPick<S> = Rc<dyn Fn(&mut S, &Item, &mut Tx)>;
type OnIndex<S> = Rc<dyn Fn(&mut S, usize)>;
type OnFlag<S> = Rc<dyn Fn(&mut S, bool)>;
type OnAdd<S> = Rc<dyn Fn(&mut S, &str, &mut Tx)>;
type OnQueue<S> = Rc<dyn Fn(&mut S, QueueAct, &mut Tx)>;

/// A press in the queued-prompts panel (`PromptQueuePanel`), by queue id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueueAct {
    /// Collapse or expand the list.
    Toggle,
    Clear,
    /// Stop the turn and send this one now.
    Steer(String),
    /// Take a text-only prompt back into the draft.
    Edit(String),
    Remove(String),
}

/// One queued prompt's row.
pub struct QueueRow {
    pub id: String,
    pub summary: String,
    pub editable: bool,
}

/// What the composer's presses and keys do. `edit` reaches the field's model.
pub struct Hooks<S> {
    pub edit: Edit<S>,
    pub send: Act<S>,
    pub cancel: Act<S>,
    /// The field changed: the host reads the trigger again.
    pub changed: Act<S>,
    /// A key while the field has focus, before the editor (the open menu's keys).
    pub key: OnKey<S>,
    pub paste: OnPaste<S>,
    pub pick: OnPick<S>,
    pub hover: OnIndex<S>,
    pub back: Act<S>,
    /// The add menu: open it (true) or close it.
    pub add_menu: OnFlag<S>,
    /// The phone's + opens the host's Add to chat sheet instead of the menu.
    pub add_sheet: Act<S>,
    /// An add-menu row: `@`, `/`, a tool id, or `attach`.
    pub add: OnAdd<S>,
    /// The agent chip: open its menu (true) or close it.
    pub agent: OnFlag<S>,
    /// An agent menu row, by config id.
    pub pick_agent: OnAdd<S>,
    pub remove_attachment: OnIndex<S>,
    pub queue: OnQueue<S>,
    /// The mode chip and the context ring.
    pub control: super::controls::OnControl<S>,
}

impl<S> Clone for Hooks<S> {
    fn clone(&self) -> Self {
        Hooks {
            edit: self.edit.clone(),
            send: self.send.clone(),
            cancel: self.cancel.clone(),
            changed: self.changed.clone(),
            key: self.key.clone(),
            paste: self.paste.clone(),
            pick: self.pick.clone(),
            hover: self.hover.clone(),
            back: self.back.clone(),
            add_menu: self.add_menu.clone(),
            add_sheet: self.add_sheet.clone(),
            add: self.add.clone(),
            agent: self.agent.clone(),
            pick_agent: self.pick_agent.clone(),
            remove_attachment: self.remove_attachment.clone(),
            queue: self.queue.clone(),
            control: self.control.clone(),
        }
    }
}

/// The open menu: its rows, the highlighted row and the panel's title (a drilled-in panel).
pub struct MenuView {
    pub sections: Vec<Section>,
    pub highlight: usize,
    pub title: Option<String>,
    /// The Explorer folder under the title, relative to the project.
    pub crumb: String,
    pub empty: &'static str,
}

/// Where the composer sits: the launcher's start screen, a running chat, or the phone's card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Start,
    Chat,
    Phone,
}

/// A row of the agent chip's menu.
pub struct AgentChoice {
    pub id: String,
    pub name: String,
    pub picked: bool,
}

pub struct Spec<'a> {
    /// Keys the composer's parts (`{key}-tray`, `{key}-field`, `{key}-send`).
    pub key: String,
    pub edit: &'a TextEdit,
    pub place: Place,
    pub busy: bool,
    pub disabled: bool,
    pub placeholder: String,
    /// The agent chip's text, when the host shows one.
    pub agent: Option<String>,
    /// The agents the chip's menu offers; none, and the chip only reports its press.
    pub agents: Vec<AgentChoice>,
    pub agent_open: bool,
    /// The working copy under the card.
    pub foot: Foot,
    pub attachments: &'a [Attachment],
    pub menu: Option<MenuView>,
    pub add_open: bool,
    pub tools_open: bool,
    /// The shared-element id the tray flies under: the launcher's composer and the chat it
    /// starts carry the same one, so the card moves from the start block to the chat's dock.
    pub matched: Option<String>,
    /// The chat's queued prompts, sent top to bottom after the running turn.
    pub queue: Vec<QueueRow>,
    pub queue_open: bool,
    /// The approval the chat waits on (`PermissionPrompt`), drawn under the queue.
    pub prompt: Option<El>,
    /// A chat's modes, for the chip right of `+`.
    pub mode: Option<super::controls::ModeView>,
    /// A chat's context window, for the ring.
    pub usage: Option<super::controls::UsageView>,
}

pub const PLACEHOLDER: &str = "Ask anything… (/ for commands, @ for files and chats)";
pub const PHONE_PLACEHOLDER: &str = "Ask anything…";

/// A Troupe tool's glyph (`TERMUL_TOOL_ICONS`).
pub fn tool_icon(id: &str) -> &'static rune::Icon {
    match id {
        "browser" => &i::GLOBE,
        "canvas" => &i::PALETTE,
        "automations" => &i::ZAP,
        _ => &i::BOT,
    }
}

fn pill_look(t: &Tokens, id: &str) -> PillStyle {
    let c = &t.colors;
    let glyph: &'static rune::Icon = match pill_of(id) {
        Some(Pill::Skill { .. }) => &i::SPARKLES,
        Some(Pill::Command { .. }) => &i::TERMINAL,
        Some(Pill::File { abs_path, .. }) if abs_path.ends_with('/') => &i::FOLDER,
        Some(Pill::File { .. }) => &i::FILE_TEXT,
        Some(Pill::Context { .. } | Pill::Conversation { .. }) => &i::MESSAGE_SQUARE,
        Some(Pill::Tool { id }) => tool_icon(&id),
        None => &i::FILE_TEXT,
    };
    let ink = if matches!(pill_of(id), Some(Pill::Skill { .. })) {
        c.primary
    } else {
        c.foreground
    };
    PillStyle::default().bg(c.muted).fg(ink).icon(glyph)
}

fn field<S: 'static>(t: &'static Tokens, spec: &Spec, hooks: &Hooks<S>) -> El {
    let c = &t.colors;
    // The editor's min and max height (`CHAT_START_INPUT_*`, 26 to 78 in a chat,
    // `COMPOSER_PHONE_INPUT_*`); 14 px text on 1.625 lines, 16 px on the phone.
    let (min_h, max_h, font) = match spec.place {
        Place::Start => (40.0, 160.0, sans(14.0, 400.0, 22.75)),
        Place::Chat => (26.0, 78.0, sans(14.0, 400.0, 22.75)),
        Place::Phone => (24.0, 120.0, sans(16.0, 400.0, 24.0)),
    };
    let (edit, change, key, paste, submit) = (
        hooks.edit.clone(),
        hooks.changed.clone(),
        hooks.key.clone(),
        hooks.paste.clone(),
        hooks.send.clone(),
    );
    let input = text_input(spec.edit, font)
        .key(format!("{}-field", spec.key))
        .label("Message")
        .placeholder(spec.placeholder.clone(), c.muted_foreground)
        .color(c.foreground)
        .w_full()
        .pill_style(move |id| pill_look(t, id))
        .on_edit(rune::edit_access(move |s: &mut S| edit(s)))
        .on_input_change(move |s: &mut S, tx| change(s, tx))
        .on_input_key(move |s: &mut S, k, tx| key(s, k, tx))
        .on_input_paste(move |s: &mut S, content, tx| paste(s, content, tx))
        .on_submit(move |s: &mut S, tx| submit(s, tx))
        .disabled(spec.disabled);
    rune::auto_grow(input, 1, 8).min_h(min_h).max_h(max_h)
}

fn send_button<S: 'static>(t: &'static Tokens, spec: &Spec, can_send: bool, hooks: &Hooks<S>) -> El {
    let phone = spec.place == Place::Phone;
    let size = if phone { 36.0 } else { 32.0 };
    let key = format!("{}-send", spec.key);
    if spec.busy && !can_send {
        let cancel = hooks.cancel.clone();
        return Button::icon_only(&i::SQUARE, "Cancel turn", ButtonSize::IconSm)
            .variant(ButtonVariant::Composer)
            .build(t)
            .key(key)
            .w(size)
            .h(size)
            .on_press_tx(move |s: &mut S, tx| cancel(s, tx));
    }
    let send = hooks.send.clone();
    let label = if spec.busy { "Queue message" } else { "Send message" };
    Button::icon_only(&i::ARROW_UP, label, ButtonSize::IconSm)
        .variant(ButtonVariant::Composer)
        .disabled(!can_send)
        .build(t)
        .key(key)
        .w(size)
        .h(size)
        .on_press_tx(move |s: &mut S, tx| send(s, tx))
}

fn add_button<S: 'static>(t: &'static Tokens, spec: &Spec, hooks: &Hooks<S>, layer: &mut Vec<El>) -> El {
    let c = &t.colors;
    if spec.place == Place::Phone {
        // Composer V2 phone (Paper "2.6b"): a 36-point ghost button with a 20-point glyph.
        let sheet = hooks.add_sheet.clone();
        return row()
            .key(format!("{}-add", spec.key))
            .w(36.0)
            .h(36.0)
            .center()
            .radius(10.0)
            .role("button")
            .label("Add to chat")
            .focusable(!spec.disabled)
            .child(icon(&i::PLUS, 20.0).color(c.muted_foreground))
            .on_press_tx(move |s: &mut S, tx| sheet(s, tx));
    }
    let open = hooks.add_menu.clone();
    let trigger = row()
        .key(format!("{}-add", spec.key))
        .w(32.0)
        .h(32.0)
        .center()
        .role("button")
        .label("Add to chat")
        .focusable(!spec.disabled)
        .child(icon(&i::PLUS, 16.0).color(c.muted_foreground))
        .on_press(move |s: &mut S| open(s, true));
    let pick = |id: &'static str| {
        let add = hooks.add.clone();
        move |s: &mut S, tx: &mut Tx| add(s, id, tx)
    };
    let mut tools = Menu::new(format!("{}-tools", spec.key))
        .label("Ask the agent to use")
        .width(256.0)
        .open(spec.tools_open);
    for tool in super::wire::TOOLS {
        tools = tools.item(MenuItem::new(tool.label).on_select_tx(pick(tool.id)));
    }
    let close = hooks.add_menu.clone();
    Menu::new(format!("{}-add-menu", spec.key))
        .name("Add to chat")
        .width(224.0)
        .placement(Placement::TopStart)
        .item(
            MenuItem::new("Add files or photos")
                .icon(&i::PAPERCLIP)
                .on_select_tx(pick("attach")),
        )
        .item(MenuItem::new("Mention").shortcut("@").on_select_tx(pick("@")))
        .item(MenuItem::new("Slash commands").shortcut("/").on_select_tx(pick("/")))
        .separator()
        .item(
            MenuItem::new("Troupe tools")
                .disabled(spec.disabled)
                .submenu(tools)
                .on_select_tx(pick("tools")),
        )
        .open(spec.add_open)
        .on_close(move |s: &mut S| close(s, false))
        .build(t, layer, trigger)
}

fn agent_chip<S: 'static>(t: &'static Tokens, spec: &Spec, label: &str, hooks: &Hooks<S>, layer: &mut Vec<El>) -> El {
    let c = &t.colors;
    let agent = hooks.agent.clone();
    let chip = row()
        .key(format!("{}-agent", spec.key))
        .h(28.0)
        .px(6.0)
        .gap(6.0)
        .items_center()
        .radius(8.0)
        .role("button")
        .label(label.to_owned())
        .hover(c.foreground.alpha(0.06))
        .child(icon(&i::BOT, 12.0).color(c.muted_foreground))
        .child(
            text(label.to_owned())
                .font(sans(12.0, 400.0, 16.0))
                .color(c.muted_foreground)
                .truncate(),
        )
        .child(icon(&i::CHEVRON_DOWN, 12.0).color(c.muted_foreground))
        .on_press(move |s: &mut S| agent(s, true));
    if spec.agents.is_empty() {
        return chip;
    }
    let mut menu = Menu::new(format!("{}-agent-menu", spec.key))
        .name("Agent")
        .width(224.0)
        .placement(if spec.place == Place::Phone { Placement::TopStart } else { Placement::TopEnd });
    for choice in &spec.agents {
        let (pick, id) = (hooks.pick_agent.clone(), choice.id.clone());
        menu = menu.item(
            MenuItem::new(choice.name.clone())
                .checked(choice.picked)
                .on_select_tx(move |s: &mut S, tx: &mut Tx| pick(s, &id, tx)),
        );
    }
    let close = hooks.agent.clone();
    menu.open(spec.agent_open)
        .on_close(move |s: &mut S| close(s, false))
        .build(t, layer, chip)
}

/// A 22-point icon action of a queue row.
fn queue_icon<S: 'static>(t: &Tokens, key: String, glyph: &'static rune::Icon, label: &str, act: QueueAct, hooks: &Hooks<S>) -> El {
    let c = &t.colors;
    let on = hooks.queue.clone();
    row()
        .key(key)
        .w(22.0)
        .h(22.0)
        .center()
        .radius(6.0)
        .role("button")
        .label(label.to_owned())
        .hover(c.foreground.alpha(0.08))
        .child(icon(glyph, 14.0).color(c.muted_foreground))
        .on_press_tx(move |s: &mut S, tx| on(s, act.clone(), tx))
}

/// `PromptQueuePanel`: "N queued" (it collapses the list) and Clear, then a row per prompt
/// with Steer, Edit and Remove, shown on hover. A hairline closes it off from the field.
fn queue_panel<S: 'static>(t: &'static Tokens, spec: &Spec, hooks: &Hooks<S>) -> Option<El> {
    if spec.queue.is_empty() {
        return None;
    }
    let c = &t.colors;
    let key = &spec.key;
    let toggle = hooks.queue.clone();
    let header = row()
        .key(format!("{key}-queue-head"))
        .w_full()
        .min_h(28.0)
        .pr(3.0)
        .gap(6.0)
        .items_center()
        .child(
            row()
                .key(format!("{key}-queue-toggle"))
                .grow(1.0)
                .min_w(0.0)
                .h(28.0)
                .pl(8.0)
                .gap(6.0)
                .items_center()
                .role("button")
                .label(format!("{} queued", spec.queue.len()))
                .child(
                    row().w(16.0).h(16.0).center().child(
                        icon(if spec.queue_open { &i::CHEVRON_DOWN } else { &i::CHEVRON_RIGHT }, 14.0)
                            .color(c.muted_foreground),
                    ),
                )
                .child(
                    text(format!("{} queued", spec.queue.len()))
                        .font(sans(12.0, 500.0, 16.0))
                        .color(c.muted_foreground),
                )
                .on_press_tx(move |s: &mut S, tx| toggle(s, QueueAct::Toggle, tx)),
        )
        .child(queue_icon(t, format!("{key}-queue-clear"), &i::X, "Clear queue", QueueAct::Clear, hooks));
    let mut panel = crate::paint::pad(col().key(format!("{key}-queue")).w_full().gap(2.0), [6.0; 4]).child(header);
    if spec.queue_open {
        for (n, item) in spec.queue.iter().enumerate() {
            let steer = hooks.queue.clone();
            let id = item.id.clone();
            let mut actions = row()
                .gap(2.0)
                .items_center()
                .child(
                    row()
                        .key(format!("{key}-queue-steer-{n}"))
                        .h(22.0)
                        .px(8.0)
                        .center()
                        .radius(6.0)
                        .fill(c.muted)
                        .border(0.5, c.border)
                        .role("button")
                        .label("Steer")
                        .child(text("Steer").font(sans(12.0, 500.0, 16.0)).color(c.foreground))
                        .on_press_tx(move |s: &mut S, tx| steer(s, QueueAct::Steer(id.clone()), tx)),
                );
            if item.editable {
                actions = actions.child(queue_icon(t, format!("{key}-queue-edit-{n}"), &i::EDIT, "Edit", QueueAct::Edit(item.id.clone()), hooks));
            }
            actions = actions.child(queue_icon(t, format!("{key}-queue-remove-{n}"), &i::TRASH2, "Remove", QueueAct::Remove(item.id.clone()), hooks));
            panel = panel.child(
                row()
                    .key(format!("{key}-queue-row-{n}"))
                    .w_full()
                    .min_h(28.0)
                    .pl(8.0)
                    .pr(3.0)
                    .gap(6.0)
                    .items_center()
                    .radius(8.0)
                    .hover(c.foreground.alpha(0.06))
                    .hover_group()
                    .child(el().w(16.0).h(16.0))
                    .child(
                        text(item.summary.clone())
                            .font(sans(12.0, 400.0, 20.0))
                            .color(c.muted_foreground)
                            .truncate()
                            .shrink()
                            .grow(1.0),
                    )
                    .child(actions.group_opacity(0.0, 1.0)),
            );
        }
    }
    Some(col().w_full().child(panel).child(el().w_full().h(0.5).fill(c.border)))
}

fn attachments<S: 'static>(t: &'static Tokens, spec: &Spec, hooks: &Hooks<S>) -> Option<El> {
    if spec.attachments.is_empty() {
        return None;
    }
    let c = &t.colors;
    let chips = spec.attachments.iter().enumerate().map(|(n, a)| {
        let (name, glyph) = match a {
            Attachment::Image { name, .. } => (name.clone(), &i::IMAGE_ICON),
            Attachment::FileRef { name, .. } | Attachment::FileEmbed { name, .. } => (name.clone(), &i::FILE_TEXT),
        };
        let remove = hooks.remove_attachment.clone();
        row()
            .key(format!("{}-attachment-{n}", spec.key))
            .h(32.0)
            .pl(8.0)
            .pr(4.0)
            .gap(6.0)
            .items_center()
            .radius(8.0)
            .fill(c.muted)
            .child(icon(glyph, 14.0).color(c.muted_foreground))
            .child(
                text(name.clone())
                    .font(sans(12.0, 400.0, 16.0))
                    .color(c.foreground)
                    .truncate()
                    .max_w(160.0),
            )
            .child(
                row()
                    .key("remove")
                    .w(20.0)
                    .h(20.0)
                    .center()
                    .role("button")
                    .label(format!("Remove {name}"))
                    .child(icon(&i::X, 12.0).color(c.muted_foreground))
                    .on_press(move |s: &mut S| remove(s, n)),
            )
    });
    Some(
        row()
            .key(format!("{}-attachments", spec.key))
            .w_full()
            .gap(6.0)
            .pt(10.0)
            .px(10.0)
            .wrap()
            .children(chips),
    )
}

fn menu_row<S: 'static>(t: &'static Tokens, key: &str, n: usize, item: &Item, lit: bool, hooks: &Hooks<S>) -> El {
    let c = &t.colors;
    let (glyph, tint, drill): (Option<&'static rune::Icon>, _, bool) = match &item.row {
        Row::Skill(_) => (Some(&i::SPARKLES), c.primary, false),
        Row::Command { .. } => (Some(&i::TERMINAL), c.muted_foreground, false),
        Row::Conversations | Row::Chat(_) => (
            Some(&i::MESSAGE_SQUARE),
            c.muted_foreground,
            matches!(item.row, Row::Conversations),
        ),
        Row::Explorer => (Some(&i::FOLDER), c.muted_foreground, true),
        Row::Dir(_) => (Some(&i::FOLDER), c.muted_foreground, true),
        Row::UseFolder(_) => (Some(&i::FOLDER), c.muted_foreground, false),
        Row::File(_) => (Some(&i::FILE), c.muted_foreground, false),
    };
    let dimmed = matches!(&item.row, Row::Dir(e) | Row::File(e) if e.ignored);
    let tile = matches!(item.row, Row::Conversations | Row::Explorer);
    let mut e = row()
        .key(format!("{key}-row-{n}"))
        .w_full()
        .min_h(44.0)
        .px(8.0)
        .gap(8.0)
        .items_center()
        .radius(8.0)
        .role("option");
    if lit {
        e = e.fill(c.foreground.alpha(0.06));
    }
    if let Some(g) = glyph {
        let glyph = icon(g, 14.0).color(if tile { c.foreground } else { tint });
        e = e.child(if tile {
            row().w(24.0).h(24.0).center().radius(6.0).fill(c.muted).child(glyph)
        } else {
            glyph
        });
    }
    e = e.child(
        text(item.label.clone())
            .font(sans(12.0, 500.0, 16.0))
            .color(if dimmed { c.muted_foreground } else { c.foreground })
            .truncate(),
    );
    if let Row::Command { hint: Some(hint), .. } = &item.row {
        e = e.child(
            text(hint.clone())
                .font(sans(12.0, 400.0, 16.0))
                .color(c.muted_foreground)
                .truncate(),
        );
    }
    if let Some(d) = &item.description {
        e = e.child(
            text(d.clone())
                .font(sans(12.0, 400.0, 16.0))
                .color(c.muted_foreground)
                .truncate()
                .shrink()
                .grow(1.0),
        );
    } else {
        e = e.child(el().grow(1.0));
    }
    if drill {
        e = e.child(icon(&i::CHEVRON_RIGHT, 14.0).color(c.muted_foreground));
    }
    let (pick, hover, picked) = (hooks.pick.clone(), hooks.hover.clone(), item.clone());
    e.on_press_tx(move |s: &mut S, tx| pick(s, &picked, tx))
        .on_hover(move |s: &mut S| hover(s, n))
}

fn menu_surface<S: 'static>(t: &'static Tokens, key: &str, menu: &MenuView, hooks: &Hooks<S>) -> El {
    let c = &t.colors;
    let mut list = col()
        .key(format!("{key}-list"))
        .w_full()
        .max_h(256.0)
        .scroll(rune::ScrollSpec { stick_bottom: false });
    let mut n = 0;
    for (s, section) in menu.sections.iter().enumerate() {
        let mut group = col().key(format!("{key}-section-{s}")).w_full();
        if let Some(h) = &section.heading {
            group = group.child(pad(
                text(h.clone()).font(sans(12.0, 500.0, 16.0)).color(c.muted_foreground),
                [8.0, 8.0, 4.0, 8.0],
            ));
        }
        if let Some(notice) = &section.notice {
            group = group.child(pad(
                text(notice.clone()).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground),
                [6.0, 8.0, 6.0, 8.0],
            ));
        }
        for item in &section.items {
            group = group.child(menu_row(t, key, n, item, n == menu.highlight, hooks));
            n += 1;
        }
        list = list.child(group);
    }
    if n == 0 && menu.sections.iter().all(|s| s.notice.is_none()) {
        list = list.child(pad(
            text(menu.empty).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground),
            [6.0, 8.0, 6.0, 8.0],
        ));
    }
    let mut surface = crate::overlay::popover_surface(
        t,
        pad(col().key(format!("{key}-menu")).role("listbox").w_full(), [4.0; 4]),
        12.0,
    );
    if let Some(title) = &menu.title {
        let back = hooks.back.clone();
        surface = surface.child(
            row()
                .key(format!("{key}-menu-back"))
                .min_h(44.0)
                .px(8.0)
                .gap(6.0)
                .items_center()
                .role("button")
                .label("Back")
                .child(icon(&i::CHEVRON_LEFT, 12.0).color(c.foreground))
                .child(text(title.clone()).font(sans(12.0, 600.0, 16.0)).color(c.foreground))
                .child(
                    text(menu.crumb.clone())
                        .font(sans(12.0, 400.0, 16.0))
                        .color(c.muted_foreground)
                        .truncate()
                        .shrink(),
                )
                .on_press_tx(move |s: &mut S, tx| back(s, tx)),
        );
    }
    surface.child(list)
}

/// The composer: the open menu over the tray, then the tray, with the add menu's surface
/// anchored out of flow (no ancestor of a composer clips).
pub fn composer<S: 'static>(t: &'static Tokens, mut spec: Spec, hooks: &Hooks<S>) -> El {
    let mut layer = Vec::new();
    let layer = &mut layer;
    let c = &t.colors;
    let can_send = !spec.disabled && (!super::doc::js_trim(&spec.edit.text).is_empty() || !spec.attachments.is_empty());
    let (row_pad, toolbar_pad) = match spec.place {
        Place::Start => ([12.0, 8.0, 4.0, 14.0], [0.0, 8.0, 8.0, 8.0]),
        Place::Chat => ([16.0, 10.0, 10.0, 18.0], [0.0, 8.0, 8.0, 8.0]),
        Place::Phone => ([11.0, 14.0, 2.0, 16.0], [0.0, 6.0, 6.0, 4.0]),
    };
    let field_key = format!("{}-field", spec.key);
    let input_row = pad(
        row().key(format!("{}-row", spec.key)).w_full().gap(4.0).items_start(),
        row_pad,
    )
    .child(col().grow(1.0).min_w(0.0).child(field(t, &spec, hooks)))
    .on_press_tx(move |_: &mut S, tx| tx.focus(field_key.clone(), false));
    let ring = spec.usage.filter(|u| u.shown()).map(|u| super::controls::context_ring(t, &spec.key, &u, &hooks.control, layer));
    let (input_row, phone_ring) = match ring {
        Some(ring) if spec.place != Place::Phone => (input_row.child(ring.mr(-8.0).my(-3.0)), None),
        ring => (input_row, ring),
    };
    let mut start = row()
        .key(format!("{}-tools-start", spec.key))
        .gap(6.0)
        .items_center()
        .child(add_button(t, &spec, hooks, layer));
    if let Some(mode) = spec.mode.as_ref().filter(|m| !m.modes.is_empty() && spec.place != Place::Phone) {
        start = start.child(super::controls::mode_chip(t, &spec.key, mode, &hooks.control, layer));
    }
    let mut end = row()
        .key(format!("{}-tools-end", spec.key))
        .gap(4.0)
        .items_center()
        .min_w(0.0);
    if let Some(agent) = &spec.agent {
        let chip = agent_chip(t, &spec, agent, hooks, layer);
        if spec.place == Place::Phone {
            start = start.child(chip);
        } else {
            end = end.child(chip);
        }
    }
    if let Some(ring) = phone_ring {
        end = end.child(ring);
    }
    end = end.child(send_button(t, &spec, can_send, hooks));
    let toolbar = pad(
        row()
            .key(format!("{}-toolbar", spec.key))
            .w_full()
            .items_center()
            .justify_between(),
        toolbar_pad,
    )
    .child(start)
    .child(end);
    let mut card = col().key(format!("{}-card", spec.key)).w_full();
    if let Some(queue) = queue_panel(t, &spec, hooks) {
        card = card.child(queue);
    }
    if let Some(prompt) = spec.prompt.take() {
        card = card.child(prompt);
    }
    if let Some(row) = attachments(t, &spec, hooks) {
        card = card.child(row);
    }
    card = card.child(input_row).child(toolbar);
    let tray = if spec.place == Place::Phone {
        let card = shadows(card.radius(20.0).fill(c.popover), &t.shadows.shadow_control);
        match &spec.foot {
            Foot::Launcher { branch } => col()
                .key(format!("{}-tray", spec.key))
                .w_full()
                .child(card)
                .child(phone_strip(t, &spec.key, branch.as_deref())),
            Foot::Chat { .. } => card.key(format!("{}-tray", spec.key)),
        }
    } else {
        let card = shadows(card.radius(14.0).fill(c.popover), &t.shadows.shadow_control);
        let foot = row()
            .key(format!("{}-foot", spec.key))
            .w_full()
            .h(40.0)
            .px(8.0)
            .gap(16.0)
            .items_center()
            .justify_between()
            .child(worktree(t, &spec.foot))
            .child(
                row()
                    .key("connectors")
                    .h(28.0)
                    .px(6.0)
                    .gap(6.0)
                    .items_center()
                    .role("button")
                    .label("Connectors, 0 on")
                    .child(
                        text("Connectors")
                            .font(sans(12.0, 400.0, 16.0))
                            .color(c.muted_foreground),
                    )
                    .child(icon(&i::CHEVRON_DOWN, 12.0).color(c.muted_foreground)),
            );
        col()
            .key(format!("{}-tray", spec.key))
            .w_full()
            .radius(20.0)
            .border(0.5, c.border)
            .fill(c.card)
            .px(6.0)
            .pt(6.0)
            .child(card)
            .child(foot)
    };
    let tray = match &spec.matched {
        Some(id) => tray.matched(id.as_str()),
        None => tray,
    };
    let mut host = col().key(spec.key.clone()).w_full();
    if let Some(menu) = &spec.menu {
        let over = menu_surface(t, &spec.key, menu, hooks).pin(Some(8.0), None, Some(8.0), Some(4.0));
        host = host.child(row().key(format!("{}-menu-host", spec.key)).w_full().h(0.0).child(over));
    }
    host.child(tray).children(std::mem::take(layer))
}
