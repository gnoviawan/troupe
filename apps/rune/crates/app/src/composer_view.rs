//! Where the composers mount: under a chat's transcript (`ChatInputBar`, centred in the
//! transcript's 768-point column) and under the launcher's greeting (the 680-point start
//! column).

use rune::{El, col, el};
use theme::Tokens;
use ui::composer::view::{self, Place, Spec};

use crate::State;
use crate::composer::Host;

/// The composer of `host`.
pub fn composer(s: &State, t: &'static Tokens, host: &Host, place: Place) -> El {
    let draft = s.draft(host).unwrap_or(&s.composer.empty);
    let busy = match host {
        Host::Chat(id) => s
            .chat
            .acp
            .sessions
            .get(id)
            .is_some_and(|session| session.active_turn || session.open_turn_id.is_some()),
        Host::Launcher(_) => false,
    };
    let placeholder = if place == Place::Phone {
        view::PHONE_PLACEHOLDER
    } else {
        view::PLACEHOLDER
    };
    let spec = Spec {
        key: host.key(),
        edit: &draft.edit,
        place,
        busy,
        // A chat's draft waits for its transcript: the model refuses a prompt for a session it
        // has not installed, and Send clears the draft.
        disabled: s.host.is_none()
            || match host {
                Host::Launcher(id) => s.composer.launching.is_some() || !s.launcher_ready(id),
                Host::Chat(id) => !s.chat.loaded.contains(id),
            },
        placeholder: placeholder.to_owned(),
        agent: s.agent_label(host),
        agents: s.agent_choices(host),
        agent_open: draft.agent_open,
        foot: match host {
            Host::Chat(_) => "Current checkout · main".to_owned(),
            Host::Launcher(_) => "Current checkout".to_owned(),
        },
        attachments: &draft.attachments,
        menu: s.menu_view(host),
        add_open: draft.add_open,
        tools_open: draft.tools_open,
        queue: s.queue_rows(host),
        // `defaultOpen={!isMobileShell}`.
        queue_open: (place != Place::Phone) != draft.queue_toggled,
    };
    view::composer(t, spec, &State::composer_hooks(host))
}

/// A chat tab's composer dock: 12 points above, 24 below, in the transcript's column.
pub fn chat_dock(s: &State, t: &'static Tokens, session_id: &str) -> El {
    let host = Host::Chat(session_id.to_owned());
    col()
        .key(format!("composer-dock-{session_id}"))
        .w_full()
        .items_center()
        .px(20.0)
        .pt(12.0)
        .pb(24.0)
        .child(
            col()
                .w_full()
                .max_w(ui::chat::COLUMN_MAX_W)
                .child(composer(s, t, &host, Place::Chat)),
        )
}

/// The launcher's start block (`chat-start.tsx`): the greeting 24 points over the composer,
/// centred in the free height with a 44-point basis under it, so the block sits 22 points
/// above the pane's middle (centred in the window, under the tab bar). Rune's `grow` has a
/// zero basis, so the greeting and the 44 points stand outside the two growing parts.
pub fn launcher(s: &State, t: &'static Tokens, project_id: &str, name: &str) -> El {
    let host = Host::Launcher(project_id.to_owned());
    col()
        .key(format!("launcher-{project_id}"))
        .w_full()
        .h_full()
        .grow(1.0)
        .items_center()
        .px(20.0)
        .child(el().key("launcher-top").grow(1.0).min_h(0.0))
        .child(ui::shell::launcher_heading(t, name).mb(24.0))
        .child(col().w_full().max_w(680.0).child(composer(s, t, &host, Place::Start)))
        .child(el().key("launcher-bottom").grow(1.0).min_h(0.0))
        .child(el().key("launcher-tab-bar").h(44.0))
}

/// The phone launcher (Paper "2.6g"): the greeting in the middle of the room above, 28 points
/// kept under it, and the compact composer at the bottom, 16 points in from the sides.
pub fn launcher_phone(s: &State, t: &'static Tokens, project_id: &str, name: &str) -> El {
    let host = Host::Launcher(project_id.to_owned());
    col()
        .key(format!("launcher-{project_id}"))
        .w_full()
        .h_full()
        .grow(1.0)
        .child(
            col()
                .key("launcher-hero")
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .center()
                .px(20.0)
                .pb(28.0)
                .child(ui::shell::launcher_heading(t, name)),
        )
        .child(
            col()
                .w_full()
                .px(16.0)
                .pb(24.0)
                .child(composer(s, t, &host, Place::Phone)),
        )
}
