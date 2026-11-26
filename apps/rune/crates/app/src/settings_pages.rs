//! The bodies of the App Preferences pages this client draws: Preferences, Terminal, Agents
//! (the chat window and time limits), Keyboard shortcuts and About. Every control writes
//! `settings/app` through `model::settings`, as `useUpdateAppSetting` does.

use model::settings::{Event, Timeout};
use rune::{El, col, icon, row, text};
use serde_json::{Value, json};
use theme::Tokens;
use ui::overlay::Select;
use ui::settings::{Stepper, card, group, notice, setting, stepper, switch_row};
use ui::type_::sans;
use ui::{Button, ButtonSize, ButtonVariant};

use crate::{State, settings_shortcuts};

type Pick = fn(&mut State, usize);

pub fn page(s: &State, t: &'static Tokens, id: &str, phone: bool, layer: &mut Vec<El>) -> Vec<El> {
    match id {
        "terminal-profiles" => crate::settings_profiles::profiles_page(s, t),
        "skills" => crate::settings_profiles::skills_page(s, t, phone),
        "preferences" => preferences(s, t, layer),
        "terminal" => terminal(s, t, layer),
        "ai-agents" => agents(s, t, layer),
        "shortcuts" => settings_shortcuts::shortcuts(s, t),
        "about" => settings_shortcuts::about(t),
        _ => Vec::new(),
    }
}

/// `SettingsSelect`: a menu of `options`, its trigger 240 wide.
fn select(
    s: &State,
    t: &'static Tokens,
    layer: &mut Vec<El>,
    (id, label): (&'static str, &str),
    options: &[&str],
    value: Option<usize>,
    pick: Pick,
) -> El {
    Select::new(format!("select-{id}"), options)
        .value(value)
        .label(label)
        .width(240.0)
        .open(s.settings_ui.select == Some(id))
        .on_open(move |s: &mut State| s.open_select(id))
        .on_pick(move |s: &mut State, i| {
            s.close_select();
            pick(s, i);
        })
        .on_close(|s: &mut State| s.close_select())
        .build(t, layer)
}

fn toggle(s: &State, t: &Tokens, key: &'static str, label: &str, description: &str, setting_key: &'static str) -> El {
    let on = s.settings.get(setting_key) != &Value::Bool(false);
    switch_row(t, key, label, description, on, move |e| {
        e.on_press(move |s: &mut State| s.set_setting(setting_key, json!(!on)))
    })
}

fn number(s: &State, key: &str) -> Option<f64> {
    s.settings.get(key).as_f64()
}

fn preferences(s: &State, t: &'static Tokens, layer: &mut Vec<El>) -> Vec<El> {
    let theme_name = s.tokens().name.clone();
    let change = Button::new("Change theme")
        .variant(ButtonVariant::Secondary)
        .size(ButtonSize::Xs)
        .build(t)
        .key("change-theme")
        .on_press(|s: &mut State| {
            s.close_settings();
            s.open_theme_picker();
        });
    let zoom = number(s, "uiZoomLevel").unwrap_or(1.0);
    let zoom_stepper = stepper(
        t,
        Stepper {
            key: "ui-zoom",
            label: "Interface zoom",
            value: format!("{}%", (zoom * 100.0).round()),
            decrement: "Decrease interface zoom",
            increment: "Increase interface zoom",
            at_min: zoom <= 0.5,
            at_max: zoom >= 3.0,
            reset: Some("Reset to 100%"),
        },
        |key, e| {
            let next = match key {
                "ui-zoom-dec" => zoom - 0.1,
                "ui-zoom-inc" => zoom + 0.1,
                _ => 1.0,
            };
            let next = (next.clamp(0.5, 3.0) * 10.0).round() / 10.0;
            e.on_press(move |s: &mut State| s.set_setting("uiZoomLevel", json!(next)))
        },
    );
    let zoom_keys = if cfg!(target_arch = "wasm32") {
        "Alt+=, Alt+- and Alt+0"
    } else {
        "Ctrl+=, Ctrl+- and Ctrl+0"
    };
    let appearance = group(
        t,
        "appearance",
        "Appearance",
        vec![],
        vec![card(
            t,
            "appearance-card",
            vec![
                setting(t, "color-theme", "Theme", &theme_name, Some(change)),
                setting(
                    t,
                    "ui-zoom-row",
                    "Interface zoom",
                    &format!("Make everything larger or smaller, from 50% to 300%. {zoom_keys} also work."),
                    Some(zoom_stepper),
                ),
            ],
        )],
    );
    let notifications = group(
        t,
        "notifications",
        "Notifications",
        vec![],
        vec![card(
            t,
            "notifications-card",
            vec![
                toggle(
                    s,
                    t,
                    "notify-needs-you",
                    "Notify when an agent chat needs you",
                    "An agent waits for your approval or an answer.",
                    "notifyOnAgentChatNeedsYou",
                ),
                toggle(
                    s,
                    t,
                    "notify-turn-finished",
                    "Notify when an agent chat turn finishes",
                    "A chat turn ends and nothing is queued.",
                    "notifyOnAgentChatTurnFinished",
                ),
                toggle(
                    s,
                    t,
                    "notify-terminal-idle",
                    "Notify when a terminal agent finishes",
                    "A long-running terminal tab goes quiet.",
                    "notifyOnTerminalIdle",
                ),
            ],
        )],
    );
    const DELAYS: [u64; 4] = [500, 1000, 2000, 5000];
    let delay = number(s, "editorAutoSaveDelayMs")
        .map(|d| d as u64)
        .filter(|d| DELAYS.contains(d))
        .unwrap_or(1000);
    let auto_save = if s.settings.bool("editorAutoSave") {
        DELAYS.iter().position(|d| *d == delay).map(|i| i + 1)
    } else {
        Some(0)
    };
    let editor = group(
        t,
        "editor",
        "Editor",
        vec![],
        vec![card(
            t,
            "editor-card",
            vec![setting(
                t,
                "editor-auto-save",
                "Auto save",
                "Save files after you stop typing. Files with changes save as soon as you turn it on.",
                Some(select(
                    s,
                    t,
                    layer,
                    ("editor-auto-save", "Auto save"),
                    &["Off", "After 0.5 seconds", "After 1 second", "After 2 seconds", "After 5 seconds"],
                    auto_save,
                    |s, i| {
                        if i == 0 {
                            s.set_setting("editorAutoSave", json!(false));
                            return;
                        }
                        s.set_setting("editorAutoSaveDelayMs", json!(DELAYS[i - 1]));
                        if !s.settings.bool("editorAutoSave") {
                            s.set_setting("editorAutoSave", json!(true));
                        }
                    },
                )),
            )],
        )],
    );
    let colours = swatches(t, s.settings.str("defaultProjectColor"));
    let new_projects = group(
        t,
        "new-projects",
        "New projects",
        vec![],
        vec![card(
            t,
            "new-projects-card",
            vec![setting(
                t,
                "default-project-color",
                "Default color",
                "New projects start with this color. You can change it per project.",
                Some(colours),
            )],
        )],
    );
    vec![appearance, notifications, editor, new_projects]
}

const COLOURS: [&str; 9] = ["blue", "purple", "pink", "red", "orange", "yellow", "green", "cyan", "gray"];

/// `ColorSwatchGroup`: 24 px dots, the chosen one checked.
fn swatches(t: &Tokens, chosen: &str) -> El {
    let check = theme::icons::named("Check").expect("bundled icon");
    let mut group = row()
        .key("swatches")
        .role("radiogroup")
        .label("Default project color")
        .gap(8.0)
        .items_center();
    for colour in COLOURS {
        let name = format!("{}{}", colour[..1].to_uppercase(), &colour[1..]);
        let mut dot = row()
            .key(format!("swatch-{colour}"))
            .role("radio")
            .label(name)
            .checked(colour == chosen)
            .w(24.0)
            .h(24.0)
            .radius(12.0)
            .items_center()
            .justify_center()
            .focusable(true)
            .fill(ui::shell::project_color(t, colour))
            .on_press(move |s: &mut State| s.set_setting("defaultProjectColor", json!(colour)));
        if colour == chosen {
            dot = dot.child(icon(check, 12.0).color(t.colors.background).a11y_hidden());
        }
        group = group.child(dot);
    }
    group
}

/// `SettingsDisclosure`: a card whose header row opens the rows under it.
fn disclosure(s: &State, t: &Tokens, id: &'static str, title: &str, summary: &str, rows: Vec<El>) -> El {
    let c = &t.colors;
    let open = s.settings_ui.open.contains(&id);
    let chevron = theme::icons::named(if open { "ChevronUp" } else { "ChevronDown" }).expect("bundled icon");
    let head = row()
        .key(format!("{id}-head"))
        .role("button")
        .label(title.to_owned())
        .expanded(open)
        .w_full()
        .px(16.0)
        .py(12.0)
        .gap(12.0)
        .items_center()
        .focusable(true)
        .hover(c.foreground.alpha(0.03))
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(2.0)
                .child(text(title).font(sans(12.0, 500.0, 16.0)).color(c.foreground))
                .child(
                    text(summary)
                        .font(sans(12.0, 400.0, 16.0))
                        .color(c.muted_foreground)
                        .single_line()
                        .truncate(),
                ),
        )
        .child(icon(chevron, 14.0).color(c.muted_foreground).a11y_hidden())
        .on_press(move |s: &mut State| s.toggle_disclosure(id));
    let mut all = vec![head];
    if open {
        all.extend(rows);
    }
    card(t, id, all)
}

fn terminal(s: &State, t: &'static Tokens, layer: &mut Vec<El>) -> Vec<El> {
    let c = &t.colors;
    let size = number(s, "terminalFontSize").unwrap_or(14.0);
    let font = stepper(
        t,
        Stepper {
            key: "terminal-font-size",
            label: "Font size",
            value: format!("{size} px"),
            decrement: "Decrease font size",
            increment: "Increase font size",
            at_min: size <= 10.0,
            at_max: size >= 24.0,
            reset: None,
        },
        |key, e| {
            let next = if key.ends_with("-dec") { size - 1.0 } else { size + 1.0 }.clamp(10.0, 24.0);
            e.on_press(move |s: &mut State| s.set_setting("terminalFontSize", json!(next)))
        },
    );
    let line = |i: usize, words: &str| {
        text(words.to_owned())
            .key(format!("preview{i}"))
            .font(ui::type_::mono(size as f32, 400.0, (size * 1.2) as f32))
            .color(t.terminal.foreground)
            .single_line()
    };
    let preview = col()
        .key("terminal-preview")
        .role("figure")
        .label("Preview")
        .w_full()
        .px(16.0)
        .py(12.0)
        .fill(t.terminal.background)
        .child(line(0, "$ echo \"Hello, World!\""))
        .child(line(1, "Hello, World!"))
        .child(line(2, "$ ls -la"))
        .child(line(3, "drwxr-xr-x 5 user staff 160 Jan 11 10:00 ."));
    let text_group = group(
        t,
        "text",
        "Text",
        vec![],
        vec![card(
            t,
            "text-card",
            vec![
                setting(
                    t,
                    "terminal-font-size-row",
                    "Font size",
                    "Terminal text size, from 10 to 24 px.",
                    Some(font),
                ),
                preview,
            ],
        )],
    );
    let shell = s.settings.str("defaultShell");
    let shells: Vec<&str> = if shell.is_empty() {
        vec!["System default"]
    } else {
        vec!["System default", shell]
    };
    let new_terminals = group(
        t,
        "new-terminals",
        "New terminals",
        vec![],
        vec![card(
            t,
            "new-terminals-card",
            vec![
                setting(
                    t,
                    "default-shell",
                    "Default shell",
                    "A project can choose its own shell in Project settings.",
                    Some(select(
                        s,
                        t,
                        layer,
                        ("default-shell", "Default shell"),
                        &shells,
                        Some(usize::from(!shell.is_empty())),
                        |s, i| {
                            if i == 0 {
                                s.set_setting("defaultShell", json!(""));
                            }
                        },
                    )),
                ),
                {
                    let on = s.settings.bool("terminalScreenReaderMode");
                    switch_row(
                        t,
                        "terminal-screen-reader-mode",
                        "Screen reader mode",
                        "Makes terminal output readable by screen readers. Can repeat typed characters in some setups. Changes apply to new terminals.",
                        on,
                        move |e| e.on_press(move |s: &mut State| s.set_setting("terminalScreenReaderMode", json!(!on))),
                    )
                },
            ],
        )],
    );
    const RENDERERS: [(&str, &str); 3] = [("auto", "Auto"), ("webgl", "WebGL"), ("dom", "DOM")];
    const BUFFERS: [(u64, &str); 5] = [
        (1000, "1,000 lines"),
        (5000, "5,000 lines"),
        (10000, "10,000 lines"),
        (25000, "25,000 lines"),
        (50000, "50,000 lines"),
    ];
    let renderer = s.settings.str("terminalRenderer");
    let renderer_at = RENDERERS.iter().position(|(v, _)| *v == renderer);
    let buffer = number(s, "terminalBufferSize").unwrap_or(10000.0) as u64;
    let buffer_at = BUFFERS.iter().position(|(v, _)| *v == buffer);
    let refresh = s.settings.get("automaticCompletionRefresh") != &Value::Bool(false);
    let summary = format!(
        "{} renderer · {} · completion refresh {}",
        renderer_at.map_or("Auto", |i| RENDERERS[i].1),
        buffer_at.map_or_else(|| format!("{buffer} lines"), |i| BUFFERS[i].1.to_owned()),
        if refresh { "on" } else { "off" }
    );
    let advanced = disclosure(
        s,
        t,
        "terminal-advanced",
        "Advanced",
        &summary,
        vec![
            setting(
                t,
                "terminal-renderer",
                "Renderer",
                "WebGL is fastest. Auto falls back to DOM when WebGL is not available. Applies to new terminals.",
                Some(select(
                    s,
                    t,
                    layer,
                    ("terminal-renderer", "Renderer"),
                    &RENDERERS.map(|(_, l)| l),
                    renderer_at,
                    |s, i| {
                        s.set_setting("terminalRenderer", json!(RENDERERS[i].0));
                    },
                )),
            ),
            setting(
                t,
                "terminal-scrollback",
                "Scrollback",
                "Lines kept in terminal history. More lines use more memory. Applies to new terminals.",
                Some(select(
                    s,
                    t,
                    layer,
                    ("terminal-scrollback", "Scrollback"),
                    &BUFFERS.map(|(_, l)| l),
                    buffer_at,
                    |s, i| {
                        s.set_setting("terminalBufferSize", json!(BUFFERS[i].0));
                    },
                )),
            ),
            switch_row(
                t,
                "terminal-automatic-completion-refresh",
                "Automatic completion refresh",
                "Updates command suggestions while you type. When off, press Tab to ask for suggestions. Applies immediately.",
                refresh,
                move |e| e.on_press(move |s: &mut State| s.set_setting("automaticCompletionRefresh", json!(!refresh))),
            ),
        ],
    );
    let _ = c;
    vec![text_group, new_terminals, advanced]
}

/// `formatAgentIdleWindow` and the option labels of each limit.
const IDLE: [(Option<u64>, &str); 6] = [
    (Some(900), "15 minutes"),
    (None, "30 minutes"),
    (Some(3600), "1 hour"),
    (Some(7200), "2 hours"),
    (Some(21600), "6 hours"),
    (Some(86400), "24 hours"),
];
const TURN_IDLE: [(Option<u64>, &str); 6] = [
    (None, "No limit"),
    (Some(300), "5 minutes"),
    (Some(600), "10 minutes"),
    (Some(900), "15 minutes"),
    (Some(1800), "30 minutes"),
    (Some(3600), "1 hour"),
];
const TURN: [(Option<u64>, &str); 6] = [
    (None, "No limit"),
    (Some(3600), "1 hour"),
    (Some(7200), "2 hours"),
    (Some(21600), "6 hours"),
    (Some(86400), "24 hours"),
    (Some(31_536_000), "1 year"),
];
const SESSION: [(Option<u64>, &str); 4] = [
    (None, "1 minute"),
    (Some(30), "30 seconds"),
    (Some(120), "2 minutes"),
    (Some(300), "5 minutes"),
];

fn secs(s: &State, key: &str) -> Option<u64> {
    s.settings.get(key).as_f64().map(|v| v as u64)
}

fn position(options: &[(Option<u64>, &str)], value: Option<u64>) -> Option<usize> {
    options.iter().position(|(v, _)| *v == value)
}

fn limit_label(options: &[(Option<u64>, &str)], value: Option<u64>, none: &str) -> String {
    match value {
        None => none.to_owned(),
        Some(v) => options
            .iter()
            .find(|(o, _)| *o == Some(v))
            .map_or_else(|| format!("{v} seconds"), |(_, l)| (*l).to_owned()),
    }
}

fn agents(s: &State, t: &'static Tokens, layer: &mut Vec<El>) -> Vec<El> {
    let idle = secs(s, "agentIdleWindowSecs").filter(|v| *v != 1800);
    let idle_select = select(
        s,
        t,
        layer,
        ("agent-idle-window", "Close idle chats after"),
        &IDLE.map(|(_, l)| l),
        position(&IDLE, idle),
        |s, i| {
            let secs = IDLE[i].0.map_or(Value::Null, |v| json!(v));
            log::info!("[settings] agent idle window {secs}");
            s.settings_step(Event::SetAgentIdleWindow(secs));
        },
    );
    let chats = group(
        t,
        "chats",
        "Chats",
        vec![],
        vec![
            card(
                t,
                "chats-card",
                vec![setting(
                    t,
                    "agent-idle-window",
                    "Close idle chats after",
                    "A chat with no turn running and no open view closes after this long. Reopen it from the rail.",
                    Some(idle_select),
                )],
            ),
            notice(
                t,
                "idle-window-note",
                "The server applies this window to every device that uses it.",
            ),
        ],
    );
    let limit = |key: &'static str,
                 label: &str,
                 help: &str,
                 options: &'static [(Option<u64>, &'static str)],
                 timeout: Timeout,
                 layer: &mut Vec<El>| {
        let value = secs(s, key);
        let labels: Vec<&str> = options.iter().map(|(_, l)| *l).collect();
        let pick: Pick = match timeout {
            Timeout::TurnIdleTimeout => |s, i| s.set_timeout(Timeout::TurnIdleTimeout, TURN_IDLE[i].0),
            Timeout::TurnTimeout => |s, i| s.set_timeout(Timeout::TurnTimeout, TURN[i].0),
            Timeout::SessionNewTimeout => |s, i| s.set_timeout(Timeout::SessionNewTimeout, SESSION[i].0),
            Timeout::SessionReopenTimeout => |s, i| s.set_timeout(Timeout::SessionReopenTimeout, SESSION[i].0),
        };
        setting(
            t,
            key,
            label,
            help,
            Some(select(s, t, layer, (key, label), &labels, position(options, value), pick)),
        )
    };
    let rows = vec![
        limit(
            "acpTurnIdleTimeoutSecs",
            "If an answer goes quiet",
            "Stop a turn when the agent sends nothing for this long.",
            &TURN_IDLE,
            Timeout::TurnIdleTimeout,
            layer,
        ),
        limit(
            "acpTurnTimeoutSecs",
            "Longest one answer",
            "Stop a turn that is still writing after this long.",
            &TURN,
            Timeout::TurnTimeout,
            layer,
        ),
        limit(
            "acpSessionNewTimeoutSecs",
            "Start a new chat",
            "How long a new chat may take to start.",
            &SESSION,
            Timeout::SessionNewTimeout,
            layer,
        ),
        limit(
            "acpSessionReopenTimeoutSecs",
            "Open a saved chat",
            "How long a saved chat may take to open.",
            &SESSION,
            Timeout::SessionReopenTimeout,
            layer,
        ),
    ];
    let summary = format!(
        "Goes quiet: {} · Still writing: {}",
        limit_label(&TURN_IDLE, secs(s, "acpTurnIdleTimeoutSecs"), "No limit"),
        limit_label(&TURN, secs(s, "acpTurnTimeoutSecs"), "No limit")
    );
    let limits = disclosure(s, t, "agents-limits", "Advanced: time limits", &summary, rows);
    let suggest = card(
        t,
        "suggest-card",
        vec![toggle(
            s,
            t,
            "suggest-automations",
            "Suggest automations from repeated chats",
            "Offer to schedule something you ask on several days.",
            "suggestAutomations",
        )],
    );
    let later = notice(
        t,
        "agents-later",
        "The agent list, updates and custom agents are not in the Rune client yet; open this page in the web client for them.",
    );
    vec![chats, limits, suggest, later]
}

impl State {
    fn set_timeout(&mut self, timeout: Timeout, secs: Option<u64>) {
        let secs = secs.map_or(Value::Null, |v| json!(v));
        log::info!("[settings] {} = {secs}", timeout.key());
        self.settings_step(Event::SetTimeout(timeout, secs));
    }
}
