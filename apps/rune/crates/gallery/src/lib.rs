//! Troupe's design-system gallery on Rune: one scene per entry of the `design-system` canvas
//! registry (`design-system/src/registry/index.tsx`), drawn from `ui`'s components with the same
//! mock data. `gallery --list-scenes` prints the ids; `gallery --scene button.variants --size
//! 560x456 --snap out.png` renders one entry's frame. Run with no flags it opens a window with
//! every scene in a list. The same `start` is the wasm entrypoint.

#![forbid(unsafe_code)]

mod components;
mod foundations;
mod kit;
mod overlays;
mod patterns;
mod registry;

use std::cell::RefCell;

use rune::{AppSpec, Cx, El, FontSmoothing, PointerPress, TextEdit, Tx, col, row, text};
use theme::Tokens;
use ui::Zoom;
use ui::overlay::{Overlays, SheetDrag, Toasts, history};
use ui::type_::sans;

pub use registry::{SCENES, Scene};

/// The interactive state every scene reads and its handlers write.
pub struct State {
    /// The registry id on screen.
    pub scene: &'static str,
    /// The scene list beside the frame; scenes picked by `--scene` drop it.
    pub nav: bool,
    pub theme: &'static str,
    pub zoom: Zoom,
    pub toggle_bold: bool,
    pub switch_on: bool,
    pub segment: usize,
    pub pills: [bool; 2],
    pub accordion: Option<usize>,
    pub collapsible_open: bool,
    pub motion_open: bool,
    pub copied: bool,
    pub presses: u32,
    pub input_default: TextEdit,
    pub input_filled: TextEdit,
    pub input_invalid: TextEdit,
    pub input_disabled: TextEdit,
    pub textarea: TextEdit,
    pub panel_filter: TextEdit,
    /// Open overlays, oldest first.
    pub overlays: Overlays,
    pub toasts: Toasts,
    pub sheet_drag: SheetDrag,
    pub wrap: bool,
    pub minimap: bool,
    pub shell: usize,
    pub context_at: Option<PointerPress>,
    pub branch: Option<usize>,
    pub shell_pick: Option<usize>,
    /// The two tooltips and the hover card of the floating scene.
    pub tips: [bool; 3],
    pub dialog_name: TextEdit,
    pub branch_query: TextEdit,
    pub command_inline: TextEdit,
    pub command_query: TextEdit,
}

impl Default for State {
    fn default() -> State {
        State {
            scene: SCENES[0].id,
            nav: true,
            theme: theme::DEFAULT_FAMILY,
            zoom: Zoom::DEFAULT,
            toggle_bold: true,
            switch_on: true,
            segment: 0,
            pills: [true, false],
            accordion: Some(0),
            collapsible_open: true,
            motion_open: false,
            copied: false,
            presses: 0,
            input_default: TextEdit::new(""),
            input_filled: TextEdit::new("~/code/termul"),
            input_invalid: TextEdit::new("no spaces allowed"),
            input_disabled: TextEdit::new("Read only"),
            textarea: TextEdit::new(""),
            panel_filter: TextEdit::new(""),
            overlays: Overlays::default(),
            toasts: Toasts::default(),
            sheet_drag: SheetDrag::default(),
            wrap: true,
            minimap: false,
            shell: 1,
            context_at: None,
            branch: Some(0),
            shell_pick: None,
            tips: [false; 3],
            dialog_name: TextEdit::new("api-server"),
            branch_query: TextEdit::new(""),
            command_inline: TextEdit::new(""),
            command_query: TextEdit::new(""),
        }
    }
}

impl State {
    /// Shows `id` alone, as `--scene` does, with its first overlay open.
    pub fn show(&mut self, id: &'static str) {
        self.scene = id;
        self.nav = false;
        self.overlays = Overlays::default();
        if let Some(key) = overlays::first_overlay(id) {
            self.overlays.open(key);
        }
        if id == "toast" {
            use ui::overlay::ToastKind as K;
            self.toasts.push(0.0, K::Message, "Session renamed to api-server", None);
            self.toasts.push(0.0, K::Success, "Pushed 3 commits", None);
            self.toasts.push(0.0, K::Error, "Push rejected", Some("Pull first, then retry."));
            self.toasts.push(0.0, K::Info, "Update ready", Some("Restart to apply."));
        }
    }
}

/// What a scene builder reads.
pub struct Ctx<'a> {
    pub s: &'a State,
    pub t: &'static Tokens,
    /// Reduced motion: loops hold still, as the reference shots do.
    pub still: bool,
    /// The window in points, for surfaces sized to it.
    pub window: (f32, f32),
    /// Overlay surfaces the scene opens, mounted above the view.
    pub layer: RefCell<Vec<El>>,
}

fn frame(c: &Ctx, scene: &Scene) -> El {
    col()
        .key(format!("scene/{}", scene.id))
        .label(scene.name)
        .w(scene.width)
        .h(scene.height)
        .radius(12.0)
        .fill(c.t.colors.background)
        .border(1.0, c.t.colors.border)
        .clip_rounded(12.0)
        .child(col().w_full().h_full().padding(1.0).child((scene.build)(c)))
}

fn nav(c: &Ctx) -> El {
    let t = c.t;
    let mut list = col().w(260.0).h_full().gap(1.0).padding(8.0).scroll(rune::ScrollSpec { stick_bottom: false });
    for scene in SCENES {
        let on = scene.id == c.s.scene;
        let id = scene.id;
        list = list.child(
            row()
                .key(format!("nav/{id}"))
                .role("button")
                .label(scene.name)
                .h(28.0)
                .px(8.0)
                .items_center()
                .radius(6.0)
                .fill(if on { t.colors.secondary } else { rune::Color::TRANSPARENT })
                .hover(t.colors.row_hover)
                .child(text(id).font(sans(12.0, 400.0, 16.0)).color(if on { t.colors.foreground } else { t.colors.muted_foreground }))
                .on_press(move |s: &mut State| s.scene = id),
        );
    }
    list.border_sides([0.0, 1.0, 0.0, 0.0], t.colors.border)
}

pub fn view(s: &State, cx: &mut Cx<State>) -> El {
    s.zoom.apply(cx);
    let c = Ctx {
        s,
        t: theme::tokens(s.theme, true),
        still: cx.is_reduced_motion() || !s.nav,
        window: cx.layout_size("overlay-root").map_or((1280.0, 900.0), |[w, h]| (w, h)),
        layer: RefCell::new(Vec::new()),
    };
    history::sync(s.overlays.depth());
    let scene = registry::find(s.scene).unwrap_or(&SCENES[0]);
    let body = frame(&c, scene);
    let root = row().key("gallery-root").w_full().h_full().fill(c.t.colors.background);
    let page = if s.nav {
        root.child(nav(&c)).child(col().grow(1.0).h_full().padding(24.0).scroll(rune::ScrollSpec { stick_bottom: false }).child(body))
    } else {
        root.child(body)
    };
    ui::overlay::root(page, c.layer.into_inner())
}

/// A system back press closes the top overlay; toasts leave on time.
fn poll(s: &mut State, now: f64, tx: &mut Tx) -> bool {
    let mut changed = false;
    for _ in 0..history::take_back_presses() {
        changed |= s.overlays.pop().is_some();
    }
    changed |= s.toasts.expire(now);
    if let Some(due) = s.toasts.next_expiry() {
        tx.wake_at(due);
    }
    changed
}

/// The fonts the web build fetches after its first frame, relative to the page.
pub const WEB_FONTS: [&str; 2] = ["fonts/NotoSansJP.ttf", "fonts/NotoColorEmoji.ttf"];

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(start))]
pub fn start() {
    let spec = AppSpec {
        fonts: theme::FONTS,
        families: theme::FAMILIES,
        scenes: registry::SETTERS,
        setup: Some(|_, wake| history::install(wake)),
        poll: Some(poll),
        ..AppSpec::new("Troupe gallery", (1280.0, 900.0), State::default, view)
    };
    // Chromium's text on Linux: the autohinted outline on whole-pixel pens (R0 measured it the
    // closest to the web client).
    let app = rune::App::new(spec).font_smoothing(FontSmoothing::HintedGpu);
    // The web target has no system fonts: CJK and emoji glyphs draw as boxes until these arrive.
    // They load after the first frame (`scripts/rune/fetch-web-fonts.mjs` puts them there).
    #[cfg(target_arch = "wasm32")]
    let app = WEB_FONTS.iter().fold(app, |app, url| app.font_url_later(url));
    app.run();
}
