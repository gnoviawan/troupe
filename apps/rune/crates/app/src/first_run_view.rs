//! The full-window first-run screens, drawn in place of the shell: the setup gate's loading
//! splash, the server setup wizard (`ServerSetupWizard` in setup mode), the restart handoff
//! (`RestartingScreen`), the access token screen (`WebTokenGateScreen`) and the three onboarding
//! steps (`Onboarding`).

use model::setup::{self, AuthGate, Submit};
use rune::{El, Tx, col, icon, row, text, text_input};
use theme::Tokens;
use ui::overlay::Select;
use ui::type_::{mono, sans};
use ui::{Button, ButtonSize, ButtonVariant};

use crate::State;
use crate::first_run::Gate;

pub const HOST_SELECT: &str = "setup-host";
pub const CHANNEL_SELECT: &str = "setup-channel";

fn glyph(name: &str) -> &'static rune::Icon {
    theme::icons::named(name).unwrap_or(&theme::icons::X)
}

/// The screen first run shows instead of the shell, if any.
pub fn screen(s: &State, t: &'static Tokens, phone: bool, layer: &mut Vec<El>) -> Option<El> {
    let f = &s.first_run;
    match f.gate {
        Gate::Checking => return Some(splash(t)),
        Gate::Setup => return Some(if f.wizard.applied.is_some() { restarting(s, t) } else { wizard(s, t, layer) }),
        Gate::Off | Gate::Ok => {}
    }
    if f.auth == AuthGate::Unauthorized {
        return Some(token_gate(s, t, phone));
    }
    s.onboarding_shown().then(|| onboarding(s, t, phone))
}

fn page(t: &Tokens, key: &str) -> El {
    col().key(key.to_owned()).w_full().h_full().fill(t.colors.background).items_center().px(24.0).py(40.0)
}

fn splash(t: &Tokens) -> El {
    col()
        .key("setup-checking")
        .w_full()
        .h_full()
        .fill(t.colors.background)
        .center()
        .child(text("Loading...").font(sans(14.0, 400.0, 20.0)).color(t.colors.muted_foreground))
}

fn label(t: &Tokens, words: &str) -> El {
    text(words.to_owned()).font(sans(14.0, 500.0, 20.0)).color(t.colors.foreground)
}

fn hint(t: &Tokens, words: &str) -> El {
    text(words.to_owned()).font(sans(12.0, 400.0, 16.0)).color(t.colors.muted_foreground).wrap().w_full()
}

fn error(t: &Tokens, words: &str) -> El {
    text(words.to_owned()).font(sans(12.0, 400.0, 16.0)).color(t.colors.destructive).role("alert").wrap().w_full()
}

fn field(t: &Tokens, edit: &rune::TextEdit, name: &str, mono_font: bool) -> El {
    let c = &t.colors;
    let font = if mono_font { mono(14.0, 400.0, 20.0) } else { sans(14.0, 400.0, 20.0) };
    text_input(edit, font)
        .single_line()
        .label(name.to_owned())
        .color(c.foreground)
        .w_full()
        .h(36.0)
        .px(12.0)
        .py(8.0)
        .radius(6.0)
        .fill(c.background)
        .border(1.0, c.input)
        .focus_ring(1.0, 0.0, c.focus_ring)
}

fn wizard(s: &State, t: &'static Tokens, layer: &mut Vec<El>) -> El {
    let c = &t.colors;
    let w = &s.first_run.wizard;
    let Some(a) = w.answers.as_ref() else {
        let body = if w.load_error {
            col()
                .gap(16.0)
                .items_center()
                .child(error(t, "Could not load the server settings. Check your connection and try again."))
                .child(
                    Button::new("Try again")
                        .variant(ButtonVariant::Outline)
                        .size(ButtonSize::Sm)
                        .build(t)
                        .on_press(|s: &mut State| s.load_wizard()),
                )
        } else {
            col().py(32.0).child(text("Loading server settings").font(sans(14.0, 400.0, 20.0)).color(c.muted_foreground))
        };
        return page(t, "setup-wizard").justify_center().child(body);
    };
    let errs = &w.errors;
    let err = |key: &str| errs.get(key).map(|m| error(t, m));
    let host_index = setup::HOSTS.iter().position(|(v, _)| *v == a.host).unwrap_or(0);
    let host = Select::new(HOST_SELECT, &[setup::HOSTS[0].1, setup::HOSTS[1].1])
        .value(Some(host_index))
        .label("Where can Troupe be reached?")
        .width(448.0)
        .open(s.overlays.is_open(HOST_SELECT))
        .on_open(|s: &mut State| s.overlays.open(HOST_SELECT))
        .on_pick(|s: &mut State, i: usize| {
            s.close(HOST_SELECT);
            s.wizard_set_host(setup::HOSTS[i].0);
        })
        .on_close(|s: &mut State| s.close(HOST_SELECT))
        .build(t, layer);
    let channel = a.update_channel.as_deref().unwrap_or("none");
    let channel_index = setup::CHANNELS.iter().position(|(v, ..)| *v == channel).unwrap_or(0);
    let channels = Select::new(CHANNEL_SELECT, &[setup::CHANNELS[0].1, setup::CHANNELS[1].1, setup::CHANNELS[2].1])
        .value(Some(channel_index))
        .label("Updates")
        .width(448.0)
        .open(s.overlays.is_open(CHANNEL_SELECT))
        .on_open(|s: &mut State| s.overlays.open(CHANNEL_SELECT))
        .on_pick(|s: &mut State, i: usize| {
            s.close(CHANNEL_SELECT);
            s.wizard_set_channel(setup::CHANNELS[i].0);
        })
        .on_close(|s: &mut State| s.close(CHANNEL_SELECT))
        .build(t, layer);
    let group = |children: Vec<Option<El>>| col().w_full().min_w(0.0).gap(6.0).children(children.into_iter().flatten());
    let mut form = col()
        .key("setup-form")
        .w_full()
        .max_w(448.0)
        .gap(20.0)
        .child(
            col()
                .w_full()
                .gap(6.0)
                .items_center()
                .child(text("Set up Troupe").font(sans(18.0, 600.0, 28.0)).color(c.foreground).role("heading"))
                .child(
                    text("A few questions and you're in — the defaults work for most people.")
                        .font(sans(14.0, 400.0, 20.0))
                        .color(c.muted_foreground),
                ),
        );
    if let Some(problem) = &w.config_error {
        form = form.child(
            row()
                .w_full()
                .gap(8.0)
                .px(12.0)
                .py(8.0)
                .radius(6.0)
                .fill(c.warning.alpha(0.1))
                .border(1.0, c.warning.alpha(0.4))
                .child(icon(glyph("AlertTriangle"), 14.0).color(c.warning).mt(2.0))
                .child(
                    text(format!(
                        "A saved server configuration couldn't be used ({problem}). Answer the questions below and Troupe will write a fresh one."
                    ))
                    .font(sans(12.0, 400.0, 16.0))
                    .color(c.warning)
                    .wrap()
                    .grow(1.0)
                    .min_w(0.0),
                ),
        );
    }
    form = form
        .child(group(vec![Some(label(t, "Where can Troupe be reached?")), Some(host), Some(hint(t, setup::host_hint(&a.host))), err("host")]))
        .child(group(vec![
            Some(label(t, "Port")),
            Some(
                field(t, &w.port, "Port", false)
                    .key("setup-port")
                    .on_edit(|s: &mut State| {
                        s.wizard_edited("port");
                        &mut s.first_run.wizard.port
                    })
                    .on_submit(|s: &mut State, _tx: &mut Tx| s.submit_wizard()),
            ),
            Some(hint(t, "The part after the colon in the address — like :8080. The default works for most people.")),
            err("port"),
        ]));
    if a.host == setup::ALL {
        form = form.child(
            row()
                .w_full()
                .gap(16.0)
                .justify_between()
                .child(
                    col().grow(1.0).min_w(0.0).gap(6.0).children(
                        [
                            Some(label(t, "Let other devices make changes")),
                            Some(hint(t, "When off, other devices can watch terminals and browse files but can't type or edit.")),
                            err("allowRemoteWrites"),
                        ]
                        .into_iter()
                        .flatten(),
                    ),
                )
                .child(
                    ui::Switch::new(a.allow_remote_writes)
                        .label("Let other devices make changes")
                        .build(t)
                        .on_press(|s: &mut State| s.wizard_toggle_remote()),
                ),
        );
    }
    form = form.child(group(vec![
        Some(label(t, "Updates")),
        Some(channels),
        Some(hint(t, setup::CHANNELS[channel_index].2)),
        err("updateChannel"),
    ]));
    let chevron = if w.advanced { "ChevronUp" } else { "ChevronDown" };
    form = form.child(
        Button::new("Advanced options")
            .variant(ButtonVariant::Ghost)
            .size(ButtonSize::Sm)
            .icon(glyph(chevron))
            .build(t)
            .key("setup-advanced")
            .self_start()
            .ml(-8.0)
            .on_press(|s: &mut State| s.first_run.wizard.advanced = !s.first_run.wizard.advanced),
    );
    if w.advanced {
        let mut advanced = col()
            .w_full()
            .gap(16.0)
            .child(hint(t, "Most people never need these."))
            .child(group(vec![
                Some(label(t, "Folder for terminal data")),
                Some(
                    field(t, &w.sessions_dir, "Folder for terminal data", true)
                        .on_edit(|s: &mut State| {
                            s.wizard_edited("sessionsDir");
                            &mut s.first_run.wizard.sessions_dir
                        }),
                ),
                Some(hint(t, "Where Troupe keeps terminal sessions.")),
                err("sessionsDir"),
            ]))
            .child(group(vec![
                Some(label(t, "Projects list file")),
                Some(
                    field(t, &w.projects_file, "Projects list file", true)
                        .on_edit(|s: &mut State| {
                            s.wizard_edited("projectsFile");
                            &mut s.first_run.wizard.projects_file
                        }),
                ),
                Some(hint(t, "Where Troupe stores your project list.")),
                err("projectsFile"),
            ]));
        if a.update_channel.is_some() {
            advanced = advanced.child(group(vec![
                Some(label(t, "How often to check for updates")),
                Some(
                    field(t, &w.interval, "How often to check for updates", false)
                        .on_edit(|s: &mut State| {
                            s.wizard_edited("updateIntervalSecs");
                            &mut s.first_run.wizard.interval
                        }),
                ),
                Some(hint(t, "Seconds between checks — 21600 is every 6 hours.")),
                err("updateIntervalSecs"),
            ]));
        }
        form = form.child(advanced);
    }
    if let Some(problem) = &w.form_error {
        form = form.child(text(problem.clone()).font(sans(14.0, 400.0, 20.0)).color(c.destructive).role("alert"));
    }
    form = form.child(
        Button::new(if w.submitting { "Saving…" } else { "Start Troupe" })
            .disabled(w.submitting)
            .build(t)
            .key("setup-start")
            .w_full()
            .on_press(|s: &mut State| s.submit_wizard()),
    );
    page(t, "setup-wizard").child(col().w_full().grow(1.0).items_center().justify_center().child(form)).scroll(rune::ScrollSpec { stick_bottom: false })
}

fn restarting(s: &State, t: &Tokens) -> El {
    let c = &t.colors;
    let w = &s.first_run.wizard;
    let Some(applied) = &w.applied else { return splash(t) };
    let origin = setup::restart_origin(&applied.host, applied.port, "");
    let mut body = col()
        .w_full()
        .max_w(384.0)
        .gap(16.0)
        .items_center()
        .child(ui::Spinner::new().size(24.0).build(c.muted_foreground, false))
        .child(
            col()
                .gap(6.0)
                .items_center()
                .child(text("Restarting Troupe").font(sans(18.0, 600.0, 28.0)).color(c.foreground).role("heading"))
                .child(
                    text("The server is coming back on its new address. You'll be taken there automatically — this usually takes a few seconds.")
                        .font(sans(14.0, 400.0, 20.0))
                        .color(c.muted_foreground)
                        .wrap()
                        .w_full(),
                ),
        )
        .child(text(origin).font(mono(12.0, 400.0, 16.0)).color(c.muted_foreground));
    if w.slow {
        body = body.child(
            text("This is taking longer than usual. You can open the new address yourself — this page keeps trying too.")
                .font(sans(14.0, 400.0, 20.0))
                .color(c.muted_foreground)
                .wrap()
                .w_full(),
        );
    }
    col().key("setup-restarting").w_full().h_full().fill(c.background).px(24.0).center().child(body)
}

fn token_gate(s: &State, t: &'static Tokens, phone: bool) -> El {
    let c = &t.colors;
    let f = &s.first_run;
    let input = text_input(&f.token, sans(if phone { 16.0 } else { 14.0 }, 400.0, 20.0))
        .single_line()
        .secure()
        .key("access-token")
        .label("Access token")
        .placeholder("Paste token".to_owned(), c.muted_foreground)
        .color(c.foreground)
        .w_full()
        .h(36.0)
        .px(12.0)
        .py(8.0)
        .radius(6.0)
        .fill(c.background)
        .border(1.0, c.input)
        .focus_ring(1.0, 0.0, c.focus_ring)
        .autofocus()
        .on_edit(|s: &mut State| &mut s.first_run.token)
        .on_submit(|s: &mut State, _tx: &mut Tx| s.submit_token());
    let mut form = col()
        .key("token-form")
        .role("form")
        .label("Server access token")
        .w_full()
        .max_w(384.0)
        .gap(16.0)
        .child(
            col()
                .gap(6.0)
                .items_center()
                .child(text("Server access token required").font(sans(18.0, 600.0, 28.0)).color(c.foreground).role("heading"))
                .child(
                    text("This Troupe server requires a token. Paste the token from your bootstrap link or server console.")
                        .font(sans(14.0, 400.0, 20.0))
                        .color(c.muted_foreground)
                        .wrap()
                        .w_full(),
                ),
        )
        .child(input);
    if let Some(problem) = f.token_error {
        form = form.child(text(problem.message()).font(sans(14.0, 400.0, 20.0)).color(c.destructive).role("alert"));
    }
    let _ = Submit::Empty;
    form = form.child(
        Button::new(if f.submitting { "Verifying…" } else { "Continue" })
            .disabled(f.submitting)
            .build(t)
            .key("token-continue")
            .w_full()
            .h(if phone { 44.0 } else { 36.0 })
            .on_press(|s: &mut State| s.submit_token()),
    );
    col().key("token-gate").w_full().h_full().fill(c.background).px(24.0).center().child(form)
}

fn step_dots(t: &Tokens, index: usize) -> El {
    let c = &t.colors;
    let mut dots = row().gap(6.0).items_center().a11y_hidden();
    for i in 0..setup::STEPS.len() {
        let (w, fill) = if i == index {
            (16.0, c.foreground)
        } else if i < index {
            (6.0, c.foreground.alpha(0.5))
        } else {
            (6.0, c.foreground.alpha(0.15))
        };
        dots = dots.child(row().w(w).h(6.0).radius(3.0).fill(fill));
    }
    row()
        .gap(12.0)
        .items_center()
        .child(dots)
        .child(text(format!("Step {} of {}", index + 1, setup::STEPS.len())).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground))
}

fn title(t: &Tokens, words: &str) -> El {
    text(words.to_owned()).font(sans(24.0, 400.0, 28.0)).color(t.colors.foreground).role("heading")
}

fn touch(button: El, phone: bool) -> El {
    if phone { button.min_h(44.0) } else { button }
}

fn onboarding(s: &State, t: &'static Tokens, phone: bool) -> El {
    let body = match s.first_run.step {
        0 => welcome(s, t, phone),
        1 => agents(s, t, phone),
        _ => project(s, t, phone),
    };
    col()
        .key("onboarding")
        .w_full()
        .h_full()
        .fill(t.colors.background)
        .child(col().w_full().grow(1.0).min_h(0.0).px(24.0).py(40.0).items_center().justify_center().child(body))
        .scroll(rune::ScrollSpec { stick_bottom: false })
}

fn welcome(s: &State, t: &'static Tokens, phone: bool) -> El {
    let c = &t.colors;
    let mut outline = col().w_full().radius(8.0).clip_rounded(8.0).border(1.0, c.border).fill(c.card);
    for (i, item) in setup::OUTLINE.iter().enumerate() {
        let mut line = row()
            .w_full()
            .min_h(40.0)
            .px(12.0)
            .py(8.0)
            .gap(12.0)
            .items_center()
            .child(
                text(format!("{}", i + 1))
                    .font(sans(10.0, 400.0, 14.0))
                    .color(c.muted_foreground)
                    .size(20.0, 20.0)
                    .radius(10.0)
                    .fill(c.muted)
                    .center(),
            )
            .child(text((*item).to_owned()).font(sans(12.0, 400.0, 16.0)).color(c.foreground));
        if i > 0 {
            line = line.border_sides([1.0, 0.0, 0.0, 0.0], c.border);
        }
        outline = outline.child(line);
    }
    let dark = s.dark;
    let segment = |label: &'static str, mode: &'static str, on: bool| {
        let fill = if on { c.background } else { c.muted.alpha(0.0) };
        row()
            .key(format!("appearance-{mode}"))
            .role("radio")
            .label(label)
            .h(if phone { 44.0 } else { 28.0 })
            .px(12.0)
            .radius(6.0)
            .center()
            .fill(fill)
            .child(text(label).font(sans(12.0, 500.0, 16.0)).color(if on { c.foreground } else { c.muted_foreground }))
            .on_press(move |s: &mut State| s.set_setting("appearanceMode", serde_json::json!(mode)))
    };
    let track = row()
        .role("radiogroup")
        .label("Appearance")
        .padding(2.0)
        .radius(8.0)
        .fill(c.muted)
        .child(segment("Dark", "dark", dark))
        .child(segment("Light", "light", !dark));
    col()
        .key("onboarding-welcome")
        .w_full()
        .max_w(420.0)
        .gap(32.0)
        .items_center()
        .child(
            col()
                .gap(20.0)
                .items_center()
                .child(icon(&ui::shell::TROUPE_MARK, 56.0).color(c.secondary_foreground))
                .child(
                    col()
                        .gap(8.0)
                        .items_center()
                        .child(title(t, "Welcome to Troupe"))
                        .child(
                            text("Run Claude Code, Codex and other coding agents side by side, in your own projects. Setup takes about a minute.")
                                .font(sans(14.0, 400.0, 20.0))
                                .color(c.muted_foreground)
                                .wrap()
                                .w_full(),
                        ),
                ),
        )
        .child(outline)
        .child(
            row()
                .w_full()
                .flex_wrap()
                .gap(12.0)
                .items_center()
                .justify_between()
                .child(row().gap(12.0).items_center().child(text("Appearance").font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground)).child(track))
                .child(touch(
                    Button::new("Get started").size(ButtonSize::Sm).build(t).key("onboarding-start").on_press(|s: &mut State| s.onboarding_step(1)),
                    phone,
                )),
        )
}

fn frame(t: &Tokens, index: usize, heading: &str, body: &str, wide: bool, content: El, footer: El) -> El {
    col()
        .key(format!("onboarding-{}", setup::STEPS[index]))
        .w_full()
        .max_w(if wide { 560.0 } else { 480.0 })
        .gap(24.0)
        .child(step_dots(t, index))
        .child(
            col()
                .w_full()
                .gap(6.0)
                .child(title(t, heading))
                .child(text(body.to_owned()).font(sans(14.0, 400.0, 20.0)).color(t.colors.muted_foreground).wrap().w_full()),
        )
        .child(content)
        .child(row().w_full().flex_wrap().gap(8.0).pt(8.0).items_center().justify_between().child(footer))
}

fn back(t: &Tokens, to: usize, phone: bool) -> El {
    touch(
        Button::new("Back")
            .variant(ButtonVariant::Ghost)
            .size(ButtonSize::Sm)
            .icon(glyph("ArrowLeft"))
            .build(t)
            .key("onboarding-back")
            .on_press(move |s: &mut State| s.onboarding_step(to)),
        phone,
    )
}

fn agents(s: &State, t: &'static Tokens, phone: bool) -> El {
    let c = &t.colors;
    let ready = &s.composer.agents;
    let chosen = s.first_run.agent.as_ref().and_then(|id| ready.iter().find(|a| &a.id == id)).or(ready.first());
    let mut list = col().w_full().gap(8.0).role("listbox").label("Agents");
    for agent in ready {
        let on = chosen.is_some_and(|c| c.id == agent.id);
        let id = agent.id.clone();
        list = list.child(
            row()
                .key(format!("agent-{}", agent.id))
                .role("option")
                .label(agent.name.clone())
                .w_full()
                .min_h(56.0)
                .px(12.0)
                .gap(12.0)
                .items_center()
                .radius(8.0)
                .fill(c.card)
                .border(1.0, if on { c.foreground.alpha(0.4) } else { c.border })
                .child(text(agent.name.clone()).font(sans(12.0, 500.0, 16.0)).color(c.foreground).grow(1.0))
                .child(text("Ready").font(sans(12.0, 400.0, 16.0)).color(c.success))
                .on_press(move |s: &mut State| s.first_run.agent = Some(id.clone())),
        );
    }
    if ready.is_empty() {
        list = list.child(
            row()
                .w_full()
                .min_h(56.0)
                .px(12.0)
                .items_center()
                .radius(8.0)
                .fill(c.card)
                .border(1.0, c.border)
                .child(text("No agent is ready on this computer yet").font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground)),
        );
    }
    let content = col()
        .w_full()
        .gap(8.0)
        .child(text(format!("Found {} ready on this computer", ready.len())).font(sans(12.0, 500.0, 16.0)).color(c.muted_foreground))
        .child(list);
    let mut end = row().flex_wrap().gap(8.0).items_center().justify_end();
    if chosen.is_none() {
        end = end
            .child(text("Install or sign in to one agent to continue").font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground))
            .child(touch(
                Button::new("Set up later")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Sm)
                    .build(t)
                    .key("onboarding-later")
                    .on_press(|s: &mut State| {
                        s.first_run.agent = None;
                        log::info!("[onboarding] Agent chosen: none");
                        s.onboarding_step(2);
                    }),
                phone,
            ));
    }
    let next = chosen.map_or("Continue".to_owned(), |a| format!("Continue with {}", a.name));
    let chosen_id = chosen.map(|a| a.id.clone());
    end = end.child(touch(
        Button::new(next)
            .size(ButtonSize::Sm)
            .disabled(chosen.is_none())
            .build(t)
            .key("onboarding-continue")
            .on_press(move |s: &mut State| {
                if let Some(id) = chosen_id.clone() {
                    log::info!("[onboarding] Agent chosen: {id}");
                    s.first_run.agent = Some(id);
                    s.onboarding_step(2);
                }
            }),
        phone,
    ));
    let footer = row().w_full().justify_between().items_center().child(back(t, 0, phone)).child(end);
    frame(
        t,
        1,
        "Check your agents",
        "Troupe runs the coding agents you already use. Pick the one new chats start with. You can add more later.",
        true,
        content,
        footer,
    )
}

fn project(s: &State, t: &'static Tokens, phone: bool) -> El {
    let c = &t.colors;
    let choice = |key: &str, name: &str, title: &str, detail: &str| {
        col()
            .key(key.to_owned())
            .role("button")
            .label(title.to_owned())
            .grow(1.0)
            .min_w(0.0)
            .gap(12.0)
            .padding(12.0)
            .radius(8.0)
            .fill(c.card)
            .border(1.0, c.border)
            .hover(c.foreground.alpha(0.03))
            .child(row().size(32.0, 32.0).radius(8.0).fill(c.muted).center().child(icon(glyph(name), 16.0).color(c.foreground.alpha(0.8))))
            .child(
                col()
                    .child(text(title.to_owned()).font(sans(12.0, 500.0, 16.0)).color(c.foreground))
                    .child(text(detail.to_owned()).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground)),
            )
    };
    // A project created from this step is the pick (`PickedProject`); Continue lands in it.
    let picked = s.first_run.project.as_ref().and_then(|id| s.projects.store.projects.iter().find(|p| &p.id == id));
    let choices = match picked {
        Some(project) => row()
            .key("onboarding-picked")
            .w_full()
            .min_h(56.0)
            .px(12.0)
            .gap(12.0)
            .items_center()
            .radius(8.0)
            .fill(c.card)
            .border(1.0, c.border)
            .child(row().size(32.0, 32.0).radius(8.0).fill(c.muted).center().child(icon(glyph("Folder"), 16.0).color(c.foreground.alpha(0.8))))
            .child(
                col()
                    .grow(1.0)
                    .min_w(0.0)
                    .child(text(project.name.clone()).font(sans(12.0, 500.0, 16.0)).color(c.foreground))
                    .child(text(project.path.clone().unwrap_or_default()).font(mono(11.0, 400.0, 16.0)).color(c.muted_foreground).single_line().truncate()),
            )
            .child(
                Button::new("Change")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Xs)
                    .build(t)
                    .key("onboarding-change")
                    .on_press(|s: &mut State| s.first_run.project = None),
            ),
        None => if phone { col().w_full().gap(8.0) } else { row().w_full().gap(8.0) }
            .child(
                choice("onboarding-open", "FolderOpen", "Open a folder", "A project already on this computer")
                    .on_press(|s: &mut State| s.onboarding_new_project()),
            )
            .child(
                choice("onboarding-create", "FolderPlus", "Create a new project", "An empty folder or a template, with Git")
                    .on_press(|s: &mut State| s.onboarding_new_project()),
            ),
    };
    let landing = s.first_run.landing;
    let skip = if s.first_run.agent.is_some() { "Skip, chat without a project" } else { "Skip for now" };
    let end = row()
        .gap(8.0)
        .items_center()
        .child(touch(
            Button::new(skip)
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Sm)
                .disabled(landing)
                .build(t)
                .key("onboarding-skip")
                .on_press(|s: &mut State| s.finish_onboarding(None)),
            phone,
        ))
        .child(touch(
            Button::new("Continue")
                .size(ButtonSize::Sm)
                .disabled(picked.is_none() || landing)
                .build(t)
                .key("onboarding-finish")
                .on_press(|s: &mut State| {
                    let project = s.first_run.project.clone();
                    if project.is_some() {
                        s.finish_onboarding(project);
                    }
                }),
            phone,
        ));
    let footer = row().w_full().justify_between().items_center().child(back(t, 1, phone)).child(end);
    frame(t, 2, "Where should agents work?", "Pick a folder. Agents start in it, and every chat shows what changed.", false, choices, footer)
}
