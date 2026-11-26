//! Scenes for the base components, as `design-system/src/previews/components.tsx` and the
//! switch and emboss specimens of `tokens.tsx` draw them.

use rune::{Color, El, col, el, icon, row, text};
use theme::icons as i;
use ui::type_::{MONO_XS, TEXT_XS, mono, sans};
use ui::*;

use crate::kit::{caption, labelled, pad};
use crate::{Ctx, State};

const BRANCH: &str = "feature/very-long-branch-name-that-overflows-the-tab";
const FILES: [&str; 3] = ["src/server/index.ts", "src/server/routes.ts", "README.md"];

pub fn button_variants(c: &Ctx) -> El {
    let t = c.t;
    let mut rows: Vec<El> = ButtonVariant::ALL
        .iter()
        .map(|&v| {
            labelled(
                t,
                v.name(),
                vec![
                    Button::new("Run task").variant(v).build(t).on_press(|s: &mut State| s.presses += 1),
                    Button::new("New session").variant(v).icon(&i::PLUS).build(t),
                    Button::new("Disabled").variant(v).disabled(true).build(t),
                ],
            )
        })
        .collect();
    rows.push(caption(t, "One default button per view. composer is reserved for send, stop and launch."));
    pad(16.0, rows)
}

pub fn button_sizes(c: &Ctx) -> El {
    let t = c.t;
    let mut rows: Vec<El> = ButtonSize::TEXT
        .iter()
        .map(|&size| {
            labelled(
                t,
                size.name(),
                vec![Button::new("Open folder").size(size).variant(ButtonVariant::Outline).build(t)],
            )
        })
        .collect();
    rows.push(labelled(
        t,
        "icon sizes",
        ButtonSize::ICON
            .iter()
            .map(|&size| {
                Button::icon_only(&i::SETTINGS, format!("Settings, {}", size.name()), size)
                    .variant(ButtonVariant::Outline)
                    .build(t)
            })
            .collect(),
    ));
    pad(16.0, rows)
}

pub fn button_states(c: &Ctx) -> El {
    let t = c.t;
    let spin = || Spinner::new().size(14.0).decorative().build(t.colors.disabled_foreground, c.still);
    pad(
        16.0,
        vec![
            labelled(
                t,
                "loading",
                vec![
                    Button::new("Connecting").disabled(true).leading(spin()).build(t),
                    Button::new("Fetching")
                        .variant(ButtonVariant::Outline)
                        .disabled(true)
                        .leading(Spinner::new().size(14.0).decorative().build(t.colors.foreground, c.still))
                        .build(t),
                ],
            ),
            labelled(
                t,
                "focus",
                vec![Button::new("Forced focus ring").variant(ButtonVariant::Outline).ring(true).build(t)],
            ),
            labelled(
                t,
                "as link",
                vec![Button::new("Read the docs").variant(ButtonVariant::Link).as_link().build(t)],
            ),
        ],
    )
}

pub fn toggles(c: &Ctx) -> El {
    let t = c.t;
    let bold = c.s.toggle_bold;
    let mut rows: Vec<El> = [ToggleVariant::Default, ToggleVariant::Outline]
        .iter()
        .map(|&v| {
            let mut items: Vec<El> = ToggleSize::ALL
                .iter()
                .map(|&size| {
                    Toggle::icon(&i::TERMINAL, format!("Wrap, {}", size.name()))
                        .variant(v)
                        .size(size)
                        .build(t)
                })
                .collect();
            items.push(
                Toggle::new(if bold { "Pressed" } else { "Not pressed" })
                    .variant(v)
                    .pressed(bold)
                    .build(t)
                    .on_press(|s: &mut State| s.toggle_bold = !s.toggle_bold),
            );
            items.push(Toggle::new("Disabled").variant(v).disabled(true).build(t));
            labelled(t, if v == ToggleVariant::Default { "default" } else { "outline" }, items)
        })
        .collect();
    let on = c.s.switch_on;
    let pair = |switch: El, text: &str, disabled: bool| {
        row().items_center().gap(8.0).child(switch).child(label(t, text, disabled))
    };
    rows.push(labelled(
        t,
        "Switch",
        vec![
            pair(
                Switch::new(on).label(if on { "On" } else { "Off" }).build(t).on_press(|s: &mut State| s.switch_on = !s.switch_on),
                if on { "On" } else { "Off" },
                false,
            ),
            pair(Switch::new(false).disabled(true).label("Disabled").build(t), "Disabled", true),
            pair(Switch::new(true).disabled(true).label("Disabled, on").build(t), "Disabled, on", true),
        ],
    ));
    pad(16.0, rows)
}

fn switch_specimen(c: &Ctx, name: &str, switch: Switch, disabled: bool) -> El {
    let t = c.t;
    let caption = text(name.to_owned()).font(sans(12.0, 500.0, 16.0)).color(t.colors.muted_foreground);
    col()
        .w(96.0)
        .gap(6.0)
        .items_start()
        .child(switch.label(name).build(t))
        .child(if disabled { caption.opacity(0.7) } else { caption })
}

pub fn switch_states(c: &Ctx) -> El {
    let t = c.t;
    let live = c.s.switch_on;
    pad(
        16.0,
        vec![
            labelled(
                t,
                "rest",
                vec![
                    switch_specimen(c, "Off", Switch::new(false), false),
                    switch_specimen(c, "On", Switch::new(true), false),
                ],
            ),
            labelled(
                t,
                "forced",
                vec![
                    switch_specimen(c, "Off, hover", Switch::new(false).pin(SwitchPin::Hover), false),
                    switch_specimen(c, "On, hover", Switch::new(true).pin(SwitchPin::Hover), false),
                    switch_specimen(c, "On, pressed", Switch::new(true).pin(SwitchPin::Pressed), false),
                    switch_specimen(c, "Focus", Switch::new(true).pin(SwitchPin::Focus), false),
                ],
            ),
            labelled(
                t,
                "disabled",
                vec![
                    switch_specimen(c, "Off", Switch::new(false).disabled(true), true),
                    switch_specimen(c, "On", Switch::new(true).disabled(true), true),
                ],
            ),
            labelled(
                t,
                "live",
                vec![
                    row()
                        .items_center()
                        .gap(8.0)
                        .child(
                            Switch::new(!live)
                                .label("Hover, press and tab to me")
                                .build(t)
                                .on_press(|s: &mut State| s.switch_on = !s.switch_on),
                        )
                        .child(label(t, "Hover, press and tab to me", false)),
                ],
            ),
        ],
    )
}

/// WCAG contrast of `fg` on `bg`, as the preview's `Ratio` prints it.
fn contrast(fg: Color, bg: Color) -> String {
    let l = |c: Color| 0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b;
    let (a, b) = (l(fg), l(bg));
    let ratio = (a.max(b) + 0.05) / (a.min(b) + 0.05);
    format!("{ratio:.2}:1")
}

pub fn button_emboss(c: &Ctx) -> El {
    let t = c.t;
    let k = &t.colors;
    let specimen = |name: &str, pin: ButtonPin, fill: Color| {
        col()
            .items_start()
            .gap(6.0)
            .child(Button::new("Save").pin(pin).build(t))
            .child(text(name.to_owned()).font(TEXT_XS).color(k.foreground))
            .child(text(fill.hex()[..7].to_owned()).font(MONO_XS).color(k.muted_foreground))
            .child(text(contrast(k.primary_foreground, fill)).font(MONO_XS).color(k.muted_foreground))
    };
    pad(
        16.0,
        vec![
            labelled(
                t,
                "primary",
                vec![
                    specimen("Default", ButtonPin::None, k.primary_fill),
                    specimen("Hover", ButtonPin::Hover, k.primary_fill_hover),
                    specimen("Pressed", ButtonPin::Pressed, k.primary_fill_active),
                ],
            ),
            labelled(
                t,
                "ghost hover",
                vec![
                    Button::new("Cancel").variant(ButtonVariant::Ghost).fill(k.secondary).ink(k.ghost_hover_foreground).build(t),
                    Button::new("Details").variant(ButtonVariant::Outline).fill(k.secondary).ink(k.ghost_hover_foreground).build(t),
                    text(contrast(k.ghost_hover_foreground, k.secondary)).font(MONO_XS).color(k.muted_foreground),
                ],
            ),
            caption(
                t,
                "Hover and pressed are forced with classes so they stay visible. Emboss: --shadow-emboss and --shadow-emboss-hover.",
            ),
        ],
    )
}

pub fn icon_actions(c: &Ctx) -> El {
    let t = c.t;
    let sm = |g, l: &str| IconActionButton::new(g, l).size(IconActionSize::Sm);
    pad(
        16.0,
        vec![
            labelled(
                t,
                "group",
                vec![icon_action_group(
                    t,
                    false,
                    vec![
                        IconActionButton::new(&i::COPY, "Copy").build(t),
                        IconActionButton::new(&i::EDIT, "Edit").build(t),
                        IconActionButton::new(&i::TRASH2, "Delete").build(t),
                    ],
                )],
            ),
            labelled(
                t,
                "dense, sm",
                vec![icon_action_group(
                    t,
                    true,
                    vec![
                        sm(&i::COPY, "Copy").build(t),
                        sm(&i::REFRESH_CW, "Retry").build(t),
                        sm(&i::TRASH2, "Delete, disabled").disabled(true).build(t),
                    ],
                )],
            ),
        ],
    )
}

pub fn badges(c: &Ctx) -> El {
    let t = c.t;
    pad(
        16.0,
        vec![
            labelled(t, "Badge", BadgeVariant::ALL.iter().map(|&v| Badge::new(v.name()).variant(v).build(t)).collect()),
            labelled(
                t,
                "CountBadge",
                vec![CountBadge::new(3).build(t), CountBadge::new(42).build(t), CountBadge::new(1500).max(99).build(t)],
            ),
            labelled(
                t,
                "DiffStat",
                [diff_stat_counts(t, 128, 14, false, ui::type_::TEXT_BASE), diff_stat_counts(t, 7, 0, true, ui::type_::TEXT_BASE)]
                    .into_iter()
                    .flatten()
                    .collect(),
            ),
            labelled(t, "Kbd", ["Ctrl+K", "Esc", "\u{2318} \u{21e7} P"].map(|k| kbd(t, k)).into()),
        ],
    )
}

pub fn loading(c: &Ctx) -> El {
    let t = c.t;
    let ink = t.colors.foreground;
    pad(
        16.0,
        vec![
            labelled(
                t,
                "Spinner",
                vec![
                    Spinner::new().size(12.0).label("Loading, small").build(ink, c.still),
                    Spinner::new().size(16.0).build(ink, c.still),
                    Spinner::new().size(24.0).label("Loading, large").build(ink, c.still),
                ],
            ),
            labelled(t, "Shimmer", vec![shimmer_text(t, "Thinking about the diff\u{2026}", ui::type_::TEXT_BASE, c.still)]),
            labelled(
                t,
                "Skeleton",
                vec![
                    col()
                        .w_full()
                        .gap(8.0)
                        .child(skeleton(t).h(16.0).w_frac(0.75))
                        .child(skeleton(t).h(16.0).w_frac(0.5))
                        .child(skeleton(t).h(64.0).w_full()),
                ],
            ),
        ],
    )
}

pub fn text_motion(c: &Ctx) -> El {
    let t = c.t;
    let copied = c.s.copied;
    let font = TEXT_XS;
    pad(
        16.0,
        vec![
            labelled(
                t,
                "Marquee",
                vec![
                    row()
                        .w(192.0)
                        .pt(5.0)
                        .pb(5.0)
                        .px(9.0)
                        .radius(6.0)
                        .border(1.0, t.colors.border)
                        .child(marquee_text(t, BRANCH, font, 316.4, 174.0, c.still)),
                ],
            ),
            labelled(
                t,
                "IconSwap",
                vec![
                    Button::new(if copied { "Copied" } else { "Copy path" })
                        .variant(ButtonVariant::Outline)
                        .size(ButtonSize::Sm)
                        .leading(icon_swap(if copied { &i::CHECK } else { &i::COPY }, if copied { "check" } else { "copy" }, 16.0, t.colors.foreground))
                        .build(t)
                        .on_press(|s: &mut State| s.copied = !s.copied),
                ],
            ),
            caption(t, "Hover the marquee to scroll it. IconSwap cross-fades on each click."),
        ],
    )
}

pub fn write_gates(c: &Ctx) -> El {
    let t = c.t;
    let edit = |disabled| Button::new("Edit file").variant(ButtonVariant::Outline).icon(&i::EDIT).disabled(disabled).build(t);
    pad(
        16.0,
        vec![
            labelled(t, "gated", vec![write_gate(t, true, edit(true))]),
            labelled(t, "ungated", vec![write_gate(t, false, edit(false))]),
            labelled(
                t,
                "reason",
                vec![
                    text("Screen readers hear the reason; sighted users get it on hover.")
                        .font(TEXT_XS)
                        .color(t.colors.muted_foreground)
                        .description(READ_ONLY_SERVER_REASON),
                ],
            ),
        ],
    )
}

pub fn text_fields(c: &Ctx) -> El {
    let t = c.t;
    let s = c.s;
    let cell = |name: &str, field: El| col().grow(1.0).gap(8.0).child(label(t, name, false)).child(field);
    let grid_row = |a: El, b: El| row().w_full().gap(16.0).child(a).child(b);
    pad(
        16.0,
        vec![
            col()
                .w_full()
                .gap(16.0)
                .child(grid_row(
                    cell(
                        "Default",
                        Input::new(&s.input_default).placeholder("api-server").label("Default").build(t).on_edit(|s: &mut State| &mut s.input_default),
                    ),
                    cell("Filled", Input::new(&s.input_filled).label("Filled").build(t).on_edit(|s: &mut State| &mut s.input_filled)),
                ))
                .child(grid_row(
                    cell(
                        "Invalid",
                        Input::new(&s.input_invalid).invalid(true).label("Invalid").build(t).on_edit(|s: &mut State| &mut s.input_invalid),
                    ),
                    cell("Disabled", Input::new(&s.input_disabled).disabled(true).label("Disabled").build(t)),
                )),
            col().w_full().gap(8.0).child(label(t, "Textarea", false)).child(
                Textarea::new(&s.textarea)
                    .placeholder("Describe the change for the agent\u{2026}")
                    .rows(3)
                    .label("Textarea")
                    .build(t)
                    .on_edit(|s: &mut State| &mut s.textarea),
            ),
        ],
    )
}

pub fn panel_styles(c: &Ctx) -> El {
    let t = c.t;
    let s = c.s;
    let track = Segmented::new("Git view", &["changes", "history", "stash"], s.segment)
        .build(t, "git-view", true, |s: &mut State, n| s.segment = n);
    let pills = ["ts", "css"]
        .iter()
        .enumerate()
        .map(|(n, id)| pressed_toggle(t, format!(".{id}"), s.pills[n]).on_press(move |s: &mut State| s.pills[n] = !s.pills[n]))
        .collect();
    pad(
        16.0,
        vec![
            labelled(t, "segmented", vec![track]),
            labelled(t, "pressed", pills),
            labelled(
                t,
                "panel field",
                vec![panel_field(t, &s.panel_filter, "Filter files", false).w(224.0).h(32.0).on_edit(|s: &mut State| &mut s.panel_filter)],
            ),
            labelled(
                t,
                "panel header",
                vec![
                    panel_header(
                        t,
                        "Explorer",
                        vec![panel_icon_button(t, &i::PLUS, "New file"), panel_icon_button(t, &i::REFRESH_CW, "Refresh")],
                    )
                    .w(288.0)
                    .radius(8.0)
                    .border(1.0, t.colors.border)
                    .fill(t.colors.card),
                ],
            ),
            labelled(t, "quiet icon", vec![quiet_icon_button(t, &i::MORE_HORIZONTAL, "More", 32.0, 16.0)]),
        ],
    )
}

pub fn accordion(c: &Ctx) -> El {
    let t = c.t;
    let open = c.s.accordion;
    let items = [
        ("Shell", "Default shell, login arguments and environment."),
        ("Font", "Terminal font family, size and ligatures."),
        ("Keyboard", "Shortcut scheme and copy-on-select."),
    ];
    let acc = Accordion::new(
        items
            .iter()
            .enumerate()
            .map(|(n, (title, body))| AccordionItem::new(*title, *body).open(open == Some(n)))
            .collect(),
    )
    .build(t, "accordion", |s: &mut State, n| {
        s.accordion = if s.accordion == Some(n) { None } else { Some(n) }
    });
    pad(20.0, vec![acc])
}

pub fn collapsible(c: &Ctx) -> El {
    let t = c.t;
    let open = c.s.collapsible_open;
    let chevron = icon(&i::CHEVRON_DOWN, 16.0)
        .color(t.colors.foreground)
        .rotate(if open { 0.0 } else { -std::f32::consts::FRAC_PI_2 });
    let trigger = Button::new(format!("Collapsible \u{b7} {} files", FILES.len()))
        .variant(ButtonVariant::Ghost)
        .size(ButtonSize::Sm)
        .leading(chevron)
        .build(t)
        .self_start()
        .on_press(|s: &mut State| s.collapsible_open = !s.collapsible_open);
    let files = col()
        .gap(4.0)
        .pl(28.0)
        .children(FILES.map(|f| text(f).font(mono(12.0, 400.0, 16.0)).color(t.colors.muted_foreground)));
    let motion_open = c.s.motion_open;
    let mut motion = col().gap(8.0).child(
        Button::new(format!("CollapseExpandMotion \u{b7} {}", if motion_open { "open" } else { "closed" }))
            .variant(ButtonVariant::Ghost)
            .size(ButtonSize::Sm)
            .build(t)
            .self_start()
            .on_press(|s: &mut State| s.motion_open = !s.motion_open),
    );
    if motion_open {
        motion = motion.child(
            col().w_full().padding(12.0).radius(8.0).fill(t.colors.card).child(
                text("Height and opacity animate on the chat accordion token.")
                    .font(TEXT_XS)
                    .color(t.colors.muted_foreground),
            ),
        );
    }
    pad(16.0, vec![Collapsible::new(trigger, files, open).gap(8.0).build(), separator(t, Orientation::Horizontal), motion])
}

pub fn scroll_area(c: &Ctx) -> El {
    let t = c.t;
    let rows = col().padding(8.0).children((1..=24).map(|n| {
        row()
            .px(8.0)
            .py(6.0)
            .radius(6.0)
            .hover(t.colors.row_hover)
            .child(text(format!("session-{n:02} \u{b7} zsh")).font(MONO_XS).color(t.colors.muted_foreground))
    }));
    pad(
        12.0,
        vec![
            col()
                .w_full()
                .h(176.0)
                .radius(8.0)
                .border(1.0, t.colors.border)
                .clip_rounded(8.0)
                .child(ui::scroll_area(t, "sessions", rows)),
            caption(t, "The wheel scrolls this list first, then pans the canvas at its edge."),
        ],
    )
}

pub fn resizable(c: &Ctx) -> El {
    let t = c.t;
    let k = &t.colors;
    let cell = |s: &str, fill: Option<Color>, ink: Color, mono_text: bool| {
        let e = col().w_full().h_full().center().child(
            text(s.to_owned())
                .font(if mono_text { MONO_XS } else { TEXT_XS })
                .color(ink),
        );
        match fill {
            Some(f) => e.fill(f),
            None => e,
        }
    };
    let right = col()
        .grow(1.0)
        .h_full()
        .child(col().w_full().grow(0.6).child(cell("Editor", None, k.muted_foreground, false)))
        .child(resizable_handle(t, ResizableDirection::Vertical, false))
        .child(col().w_full().grow(0.4).child(cell("terminal", Some(k.terminal_bg), k.terminal_fg, true)));
    let panes = row()
        .w_full()
        .h(192.0)
        .radius(8.0)
        .border(1.0, k.border)
        .clip_rounded(8.0)
        .child(col().w_frac(0.35).h_full().child(cell("Explorer", Some(k.card), k.muted_foreground, false)))
        .child(resizable_handle(t, ResizableDirection::Horizontal, true))
        .child(right);
    pad(
        12.0,
        vec![
            panes,
            labelled(
                t,
                "Separator",
                vec![
                    text("Left").font(TEXT_XS).color(k.foreground),
                    el().h(16.0).child(separator(t, Orientation::Vertical)),
                    text("Right").font(TEXT_XS).color(k.foreground),
                ],
            ),
        ],
    )
}

pub fn bubbles(c: &Ctx) -> El {
    let t = c.t;
    let room = 366.0 * 0.8;
    let mut rows: Vec<El> = [
        BubbleVariant::Default,
        BubbleVariant::Secondary,
        BubbleVariant::Outline,
        BubbleVariant::Ghost,
        BubbleVariant::Destructive,
    ]
    .iter()
    .map(|&v| labelled(t, v.name(), vec![Bubble::new("Run the test suite before merging.").variant(v).max_w(room).build(t)]))
    .collect();
    rows.push(labelled(
        t,
        "reactions",
        vec![bubble_group(vec![
            Bubble::new("Ship it.")
                .variant(BubbleVariant::Secondary)
                .max_w(room)
                .reactions(bubble_reactions(t, "\u{1f44d} 2"))
                .build(t),
        ])],
    ));
    pad(16.0, rows)
}

pub fn markers(c: &Ctx) -> El {
    let t = c.t;
    pad(
        16.0,
        vec![
            Marker::new("Switched to feature/canvas").icon(&i::GIT_BRANCH).build(t),
            Marker::new("Tests passed in 12.4s").icon(&i::CHECK).variant(MarkerVariant::Border).build(t),
            Marker::new("Today").variant(MarkerVariant::Separator).build(t),
        ],
    )
}

const MESSAGES: [(bool, &str, &str); 4] = [
    (true, "09:12", "Add a health check to the API server."),
    (false, "09:12", "Added GET /healthz returning 200 with uptime. Tests pass."),
    (true, "09:14", "Also log the port on startup."),
    (false, "09:15", "Done. It logs \"listening on :4000\"."),
];

pub fn messages(c: &Ctx) -> El {
    let t = c.t;
    let k = &t.colors;
    let room = (558.0 - 40.0 - 40.0) * 0.8;
    let list = col().w_full().gap(16.0).padding(20.0).children(MESSAGES.iter().map(|&(you, time, body)| {
        let avatar = row()
            .w(32.0)
            .h(32.0)
            .center()
            .radius(16.0)
            .fill(k.muted)
            .child(text(if you { "FI" } else { "AI" }).font(TEXT_XS).color(k.foreground));
        let mut m = Message::new(if you { MessageAlign::End } else { MessageAlign::Start })
            .avatar(avatar)
            .header(format!("{} \u{b7} {time}", if you { "You" } else { "Agent" }))
            .body(
                Bubble::new(body)
                    .variant(if you { BubbleVariant::Default } else { BubbleVariant::Secondary })
                    .align(if you { BubbleAlign::End } else { BubbleAlign::Start })
                    .max_w(room)
                    .build(t),
            );
        if !you {
            m = m.footer(icon_action_group(
                t,
                true,
                vec![IconActionButton::new(&i::COPY, "Copy").size(IconActionSize::Sm).build(t)],
            ));
        }
        bubble_group(vec![m.build(t)])
    }));
    col().w_full().h_full().child(ui::scroll_area(t, "messages", list))
}
