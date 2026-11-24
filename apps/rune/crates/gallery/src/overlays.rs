//! Scenes for the overlay layer, as `design-system/src/previews/patterns.tsx` and the floating
//! and select specimens of `components.tsx` draw them. Each trigger opens its surface into the
//! view's overlay layer; picking a scene with `--scene` opens its first surface, so a snapshot
//! shows it open. The sheet scene adds a Phone trigger for the bottom sheet, which the
//! registry has no entry for.

use rune::{El, col, icon, row, text};
use theme::icons as i;
use ui::overlay::{
    Command, CommandItem, ConfirmDialog, ConfirmVariant, Dialog, DialogKind, HoverCard, Menu, MenuItem, MobileSheet, Placement,
    Popover, Select, Sheet, SheetSide, ToastKind, Tooltip,
};
use ui::type_::sans;
use ui::*;

use crate::kit::{caption, labelled, pad};
use crate::{Ctx, State};

const BRANCHES: [&str; 4] = ["main", "dev", "feature/canvas", "fix/terminal-resize"];
const SHELLS: [&str; 3] = ["bash", "zsh", "fish"];
const COMMANDS: [(&str, &str, &str); 5] = [
    ("Terminal", "New terminal", "Ctrl+T"),
    ("Terminal", "Split right", "Ctrl+\\"),
    ("Project", "Open folder", "Ctrl+O"),
    ("Project", "Switch project", "Ctrl+P"),
    ("View", "Toggle sidebar", "Ctrl+B"),
];
/// The phone sheet's sub-views, root first.
const PHONE_VIEWS: [&str; 3] = ["Model", "Effort", "Thinking budget"];

/// The surface `--scene` opens for each overlay scene.
pub fn first_overlay(scene: &str) -> Option<&'static str> {
    Some(match scene {
        "dialog" => "rename",
        "sheet" => "sheet-right",
        "confirm-dialog" => "confirm-danger",
        "menus" => "view-menu",
        "command" => "command",
        "select" => "branch",
        "floating" => "find-branch",
        _ => return None,
    })
}

fn opener(key: &'static str, label: &str, t: &ui::Tokens) -> El {
    Button::new(label).variant(ButtonVariant::Outline).build(t).on_press(move |s: &mut State| s.overlays.open(key))
}

fn closer(key: &'static str) -> impl Fn(&mut State) + Clone + 'static {
    move |s: &mut State| {
        s.overlays.close(key);
    }
}

pub fn dialogs(c: &Ctx) -> El {
    let t = c.t;
    let s = c.s;
    let mut layer = c.layer.borrow_mut();
    let field = col()
        .w_full()
        .gap(8.0)
        .child(label(t, "Name", false))
        .child(Input::new(&s.dialog_name).label("Name").build(t).on_edit(|s: &mut State| &mut s.dialog_name));
    Dialog::new("rename", "Rename session")
        .description("The name shows in the tab bar and the sidebar.")
        .child(field)
        .footer(Button::new("Cancel").variant(ButtonVariant::Ghost).build(t).on_press(closer("rename")))
        .footer(Button::new("Save").build(t).on_press(closer("rename")))
        .open(s.overlays.is_open("rename"))
        .on_close(closer("rename"))
        .build(t, &mut layer);
    Dialog::new("delete", "Delete worktree feature/canvas?")
        .kind(DialogKind::Alert)
        .description("Uncommitted changes in this worktree are lost. The branch stays.")
        .footer(Button::new("Cancel").variant(ButtonVariant::Outline).build(t).on_press(closer("delete")))
        .footer(Button::new("Delete").build(t).on_press(closer("delete")))
        .open(s.overlays.is_open("delete"))
        .on_close(closer("delete"))
        .build(t, &mut layer);
    let trash = theme::icons::named("Trash2").expect("bundled icon");
    pad(
        16.0,
        vec![
            labelled(t, "Dialog", vec![opener("rename", "Rename session", t).key("rename-trigger")]),
            labelled(
                t,
                "AlertDialog",
                vec![
                    Button::new("Delete worktree")
                        .variant(ButtonVariant::Outline)
                        .icon(trash)
                        .build(t)
                        .key("delete-trigger")
                        .on_press(|s: &mut State| s.overlays.open("delete")),
                ],
            ),
        ],
    )
}

pub fn sheets(c: &Ctx) -> El {
    let t = c.t;
    let s = c.s;
    let mut layer = c.layer.borrow_mut();
    let mut triggers = Vec::new();
    for side in SheetSide::ALL {
        let key: &'static str = match side {
            SheetSide::Top => "sheet-top",
            SheetSide::Right => "sheet-right",
            SheetSide::Bottom => "sheet-bottom",
            SheetSide::Left => "sheet-left",
        };
        triggers.push(
            Button::new(capitalized(side.name()))
                .variant(ButtonVariant::Outline)
                .size(ButtonSize::Sm)
                .build(t)
                .key(format!("{key}-trigger"))
                .on_press(move |s: &mut State| s.overlays.open(key)),
        );
        let details = |k: &str, v: &str| {
            row()
                .w_full()
                .gap(8.0)
                .child(text(k.to_owned()).font(sans(12.0, 400.0, 16.0)).color(t.colors.muted_foreground).w(96.0))
                .child(text(v.to_owned()).font(sans(12.0, 400.0, 16.0)).color(t.colors.foreground))
        };
        Sheet::new(key, "Session details")
            .side(side)
            .description(format!("Opened from the {} edge.", side.name()))
            .child(col().w_full().px(16.0).gap(8.0).child(details("Shell", "zsh")).child(details("Started", "09:12")))
            .footer(Button::new("Close").variant(ButtonVariant::Outline).build(t).on_press(closer(key)))
            .open(s.overlays.is_open(key))
            .on_close(closer(key))
            .build(t, &mut layer);
    }
    let depth = s.overlays.above("phone").len();
    let mut phone = MobileSheet::new("phone", PHONE_VIEWS[depth.min(PHONE_VIEWS.len() - 1)])
        .depth(depth)
        .open(s.overlays.is_open("phone"))
        .drag(s.sheet_drag)
        .on_drag(|s: &mut State| &mut s.sheet_drag, |s: &mut State| {
            s.overlays.close("phone");
        })
        .on_back(|s: &mut State| {
            s.overlays.pop();
        })
        .on_close(|s: &mut State| {
            s.overlays.pop();
        });
    match PHONE_VIEWS.get(depth + 1) {
        Some(next) => {
            let next = *next;
            phone = phone.child(phone_row(t, next, "Opens a sub-sheet").on_press(move |s: &mut State| s.overlays.open(format!("phone/{next}"))));
        }
        None => phone = phone.child(phone_row(t, "Medium", "The deepest view")),
    }
    phone.build(t, c.window, &mut layer);
    triggers.push(
        Button::new("Phone")
            .variant(ButtonVariant::Outline)
            .size(ButtonSize::Sm)
            .build(t)
            .key("phone-trigger")
            .on_press(|s: &mut State| s.overlays.open("phone")),
    );
    pad(16.0, vec![labelled(t, "side", triggers), caption(t, "Sheets slide on --ease-drawer. Escape or the overlay closes them.")])
}

fn capitalized(s: &str) -> String {
    let mut chars = s.chars();
    chars.next().map(|f| f.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

/// A phone sheet row: 60 px with a subtitle, grouped on `bg-muted`.
fn phone_row(t: &ui::Tokens, title: &str, subtitle: &str) -> El {
    let chevron = theme::icons::named("ChevronRight").expect("bundled icon");
    row()
        .key(format!("row-{title}"))
        .role("button")
        .label(title.to_owned())
        .w_full()
        .h(60.0)
        .px(14.0)
        .items_center()
        .radius(12.0)
        .fill(t.colors.muted)
        .child(
            col()
                .grow(1.0)
                .child(text(title.to_owned()).font(sans(15.0, 400.0, 20.0)).color(t.colors.foreground))
                .child(text(subtitle.to_owned()).font(sans(14.0, 400.0, 18.0)).color(t.colors.muted_foreground)),
        )
        .child(icon(chevron, 16.0).color(t.colors.muted_foreground))
}

pub fn confirms(c: &Ctx) -> El {
    let t = c.t;
    let s = c.s;
    let mut layer = c.layer.borrow_mut();
    let cases: [(&'static str, &str, &str, &str, ConfirmVariant, bool); 3] = [
        ("confirm-default", "Restart the terminal?", "The shell restarts in the same folder. Scrollback is kept.", "Restart", ConfirmVariant::Default, false),
        ("confirm-danger", "Close 3 terminals?", "Running processes in these terminals stop.", "Close terminals", ConfirmVariant::Danger, false),
        ("confirm-loading", "Pull latest changes?", "Fetching from origin.", "Pull", ConfirmVariant::Default, true),
    ];
    for (key, title, message, action, variant, loading) in cases {
        ConfirmDialog::new(key, title, message)
            .variant(variant)
            .confirm_label(action)
            .loading(loading)
            .open(s.overlays.is_open(key))
            .on_cancel(closer(key))
            .on_confirm(closer(key))
            .build(t, &mut layer);
    }
    pad(
        16.0,
        vec![
            labelled(
                t,
                "variant",
                vec![
                    opener("confirm-default", "Default", t).key("confirm-default-trigger"),
                    opener("confirm-danger", "Danger", t).key("confirm-danger-trigger"),
                    opener("confirm-loading", "Loading", t).key("confirm-loading-trigger"),
                ],
            ),
            caption(t, "ConfirmDialog from components/ConfirmDialog. Danger focuses Cancel first."),
        ],
    )
}

pub fn menus(c: &Ctx) -> El {
    let t = c.t;
    let s = c.s;
    let mut layer = c.layer.borrow_mut();
    let mut view = Menu::new("view-menu")
        .name("View options")
        .width(224.0)
        .label("Editor")
        .item(MenuItem::new("Word wrap").checked(s.wrap).on_select(|s: &mut State| s.wrap = !s.wrap))
        .item(MenuItem::new("Minimap").checked(s.minimap).on_select(|s: &mut State| s.minimap = !s.minimap))
        .separator()
        .label("Default shell");
    for (n, shell) in SHELLS.iter().enumerate() {
        view = view.item(MenuItem::new(*shell).checked(s.shell == n).on_select(move |s: &mut State| s.shell = n));
    }
    let open_in = Menu::new("open-in")
        .name("Open in")
        .item(MenuItem::new("New tab").on_select(closer("view-menu")))
        .item(MenuItem::new("Split right").on_select(closer("view-menu")))
        .open(s.overlays.is_open("open-in"))
        .on_close(closer("open-in"));
    let view = view
        .separator()
        .item(MenuItem::new("Open in").submenu(open_in).on_select(|s: &mut State| s.overlays.open("open-in")))
        .item(MenuItem::new("New terminal").shortcut("Ctrl+T").on_select(closer("view-menu")))
        .item(MenuItem::new("Disabled item").disabled(true))
        .open(s.overlays.is_open("view-menu"))
        .on_close(closer("view-menu"))
        .build(t, &mut layer, Button::new("View options").variant(ButtonVariant::Outline).build(t).on_press(|s: &mut State| s.overlays.open("view-menu")));

    let area = row()
        .key("context-area")
        .role("button")
        .label("Right-click here")
        .w(256.0)
        .h(80.0)
        .center()
        .radius(8.0)
        .border(1.0, t.colors.border)
        .border_dash(4.0, 4.0)
        .child(text("Right-click here").font(sans(12.0, 400.0, 16.0)).color(t.colors.muted_foreground))
        .on_secondary_press(|s: &mut State, p, _| {
            s.context_at = Some(p);
            s.overlays.open("file-menu");
        });
    let context_at = if s.overlays.is_open("file-menu") { s.context_at } else { None };
    let context = Menu::new("file-menu")
        .name("src/server/index.ts")
        .width(208.0)
        .label("src/server/index.ts")
        .item(MenuItem::new("Reveal in explorer").icon(&i::FOLDER_OPEN).on_select(closer("file-menu")))
        .item(MenuItem::new("Copy path").icon(&i::COPY).shortcut("Ctrl+Shift+C").on_select(closer("file-menu")))
        .item(MenuItem::new("Rename").icon(&i::EDIT).on_select(closer("file-menu")))
        .separator()
        .item(MenuItem::new("Delete").icon(&i::TRASH2).destructive().on_select(closer("file-menu")))
        .at(context_at)
        .on_close(closer("file-menu"))
        .build(t, &mut layer, area);
    pad(16.0, vec![labelled(t, "Dropdown", vec![view]), labelled(t, "Context", vec![context])])
}

fn command_groups(query: &str) -> Vec<(&'static str, Vec<CommandItem>)> {
    let q = query.to_lowercase();
    let mut groups: Vec<(&'static str, Vec<CommandItem>)> = Vec::new();
    for (group, label, shortcut) in COMMANDS {
        if !label.to_lowercase().contains(&q) {
            continue;
        }
        if groups.last().is_none_or(|(g, _)| *g != group) {
            groups.push((group, Vec::new()));
        }
        let item = CommandItem::new(label).icon(&i::TERMINAL).shortcut(shortcut).on_select(closer("command"));
        groups.last_mut().expect("pushed above").1.push(item);
    }
    groups
}

pub fn commands(c: &Ctx) -> El {
    let t = c.t;
    let s = c.s;
    let mut layer = c.layer.borrow_mut();
    let mut inline = Command::new("inline", &s.command_inline)
        .placeholder("Type a command")
        .empty("No commands found.")
        .max_height(Some(224.0))
        .on_edit(|e| e.on_edit(|s: &mut State| &mut s.command_inline));
    for (g, items) in command_groups(&s.command_inline.text) {
        inline = inline.group(g, items);
    }
    let mut palette = Command::new("command", &s.command_query)
        .placeholder("Type a command")
        .empty("No commands found.")
        .on_edit(|e| e.on_edit(|s: &mut State| &mut s.command_query))
        .open(s.overlays.is_open("command"))
        .on_close(closer("command"));
    for (g, items) in command_groups(&s.command_query.text) {
        palette = palette.group(g, items);
    }
    palette.dialog(t, &mut layer);
    pad(16.0, vec![inline.build(t), labelled(t, "dialog", vec![opener("command", "Open CommandDialog", t).key("command-trigger")])])
}

pub fn toasts(c: &Ctx) -> El {
    let t = c.t;
    let raise = |label: &str, kind: ToastKind, title: &'static str, description: Option<&'static str>| {
        Button::new(label).variant(ButtonVariant::Outline).size(ButtonSize::Sm).build(t).on_press_tx(move |s: &mut State, tx: &mut rune::Tx| {
            s.toasts.push(tx.clock(), kind, title, description);
            tx.wake_after(std::time::Duration::from_secs_f64(ui::overlay::toast::DURATION));
        })
    };
    c.layer.borrow_mut().push(c.s.toasts.build(t, |id| Box::new(move |s: &mut State| s.toasts.dismiss(id))));
    pad(
        16.0,
        vec![
            labelled(
                t,
                "type",
                vec![
                    raise("Message", ToastKind::Message, "Session renamed to api-server", None),
                    raise("Success", ToastKind::Success, "Pushed 3 commits", None),
                    raise("Error", ToastKind::Error, "Push rejected", Some("Pull first, then retry.")),
                    raise("Warning", ToastKind::Warning, "Disk almost full", Some("2 GB left")),
                    raise("Info", ToastKind::Info, "Update ready", Some("Restart to apply.")),
                ],
            ),
            caption(t, "Toasts stack at the bottom right, four at most, and leave after four seconds."),
        ],
    )
}

pub fn select(c: &Ctx) -> El {
    let t = c.t;
    let s = c.s;
    let mut layer = c.layer.borrow_mut();
    let branch = Select::new("branch", &BRANCHES)
        .value(s.branch)
        .label("Branch")
        .group("Local branches")
        .open(s.overlays.is_open("branch"))
        .on_open(|s: &mut State| s.overlays.open("branch"))
        .on_pick(|s: &mut State, i| {
            s.branch = Some(i);
            s.overlays.close("branch");
        })
        .on_close(closer("branch"))
        .build(t, &mut layer);
    let shell = Select::new("shell", &SHELLS)
        .value(s.shell_pick)
        .placeholder("Choose a shell")
        .label("Shell")
        .open(s.overlays.is_open("shell"))
        .on_open(|s: &mut State| s.overlays.open("shell"))
        .on_pick(|s: &mut State, i| {
            s.shell_pick = Some(i);
            s.overlays.close("shell");
        })
        .on_close(closer("shell"))
        .build(t, &mut layer);
    let disabled = Select::new("shell-disabled", &["bash"]).value(Some(0)).label("Shell, disabled").disabled(true).build(t, &mut layer);
    pad(16.0, vec![labelled(t, "default", vec![branch]), labelled(t, "placeholder", vec![shell]), labelled(t, "disabled", vec![disabled])])
}

pub fn floating(c: &Ctx) -> El {
    let t = c.t;
    let s = c.s;
    let mut layer = c.layer.borrow_mut();
    let terminal = Button::icon_only(&i::TERMINAL, "Split terminal", ButtonSize::IconSm).variant(ButtonVariant::Outline).build(t);
    let split = Tooltip::new("split-tip", terminal, "Split terminal")
        .open(s.tips[0])
        .on_toggle(|s: &mut State, on| s.tips[0] = on)
        .build(t, &mut layer);
    let hover = Button::new("Hover or focus").variant(ButtonVariant::Outline).size(ButtonSize::Sm).build(t);
    let delayed = Tooltip::new("delay-tip", hover, "Opens after --tt-delay")
        .placement(Placement::Right)
        .open(s.tips[1])
        .on_toggle(|s: &mut State, on| s.tips[1] = on)
        .build(t, &mut layer);
    let branch = Button::new("feature/canvas").variant(ButtonVariant::Link).icon(&i::GIT_BRANCH).build(t);
    let card = HoverCard::new("branch-card", branch)
        .child(text("feature/canvas").font(sans(12.0, 500.0, 16.0)).color(t.colors.foreground))
        .child(text("3 commits ahead of dev · updated 2h ago").font(sans(12.0, 400.0, 16.0)).color(t.colors.muted_foreground))
        .open(s.tips[2])
        .on_toggle(|s: &mut State, on| s.tips[2] = on)
        .build(t, &mut layer);
    let query = s.branch_query.text.clone();
    let search = theme::icons::named("Search").expect("bundled icon");
    let band = row()
        .key("band")
        .w_full()
        .h(41.0)
        .px(12.0)
        .pb(1.0)
        .gap(8.0)
        .items_center()
        .border_sides([0.0, 0.0, 1.0, 0.0], t.colors.border)
        .child(icon(search, 13.0).color(t.colors.muted_foreground))
        .child(
            rune::text_input(&s.branch_query, sans(12.0, 400.0, 16.0))
                .label("Search branches")
                .placeholder("Search branches", t.colors.muted_foreground)
                .color(t.colors.foreground)
                .single_line()
                .grow(1.0)
                .no_focus_ring()
                .autofocus()
                .on_edit(|s: &mut State| &mut s.branch_query),
        );
    let mut list = col().key("branches").w_full().padding(4.0);
    for b in BRANCHES.iter().filter(|b| b.contains(&query)) {
        list = list.child(
            row()
                .px(8.0)
                .py(6.0)
                .radius(6.0)
                .hover(t.colors.row_hover)
                .child(text((*b).to_owned()).font(sans(12.0, 400.0, 16.0)).color(t.colors.foreground)),
        );
    }
    let find = Button::new("Find branch").variant(ButtonVariant::Outline).size(ButtonSize::Sm).icon(search).build(t).on_press(|s: &mut State| s.overlays.open("find-branch"));
    let popover = Popover::new("find-branch", find)
        .label("Find branch")
        .width(256.0)
        .padding(0.0)
        .placement(Placement::BottomStart)
        .child(band)
        .child(list)
        .open(s.overlays.is_open("find-branch"))
        .on_close(closer("find-branch"))
        .build(t, &mut layer);
    pad(
        16.0,
        vec![
            labelled(t, "Tooltip", vec![split, delayed]),
            labelled(t, "HoverCard", vec![card]),
            labelled(t, "Popover", vec![popover]),
            labelled(t, "Lightbox", vec![caption(t, "The image lightbox is a chat surface (R6).")]),
        ],
    )
}
