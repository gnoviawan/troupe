//! `Spinner` (`components/ui/spinner.tsx`): the comet ring, a full ring that fades into its tail
//! with a dot at its head, one lap every 500 ms. Reduced motion holds it still. The ring is drawn
//! from short arcs whose opacity climbs to the head, as the CSS conic gradient does.

use std::sync::OnceLock;

use rune::{El, Icon, LoopSpec, icon};

/// Arcs in the tail: enough that neighbours differ by under 2 % opacity.
const ARCS: usize = 60;

fn comet() -> &'static Icon {
    static ICON: OnceLock<Icon> = OnceLock::new();
    ICON.get_or_init(|| {
        // A 100-unit box; the stroke is 12 % of the diameter, centred on the ring.
        let stroke = 12.0_f32;
        let r = 50.0 - stroke / 2.0;
        let mut body = String::from(r#"<g fill="none" stroke="currentColor">"#);
        for i in 0..ARCS {
            let a0 = i as f32 / ARCS as f32 * std::f32::consts::TAU;
            let a1 = (i + 1) as f32 / ARCS as f32 * std::f32::consts::TAU + 0.01;
            let point = |a: f32| (50.0 + r * a.sin(), 50.0 - r * a.cos());
            let (x0, y0) = point(a0);
            let (x1, y1) = point(a1);
            let alpha = (i as f32 + 0.5) / ARCS as f32;
            body.push_str(&format!(
                r#"<path d="M{x0:.3} {y0:.3}A{r} {r} 0 0 1 {x1:.3} {y1:.3}" stroke-width="{stroke}" stroke-opacity="{alpha:.3}"/>"#
            ));
        }
        body.push_str(&format!(
            r#"</g><circle cx="50" cy="{}" r="{}" fill="currentColor"/>"#,
            stroke / 2.0,
            stroke / 2.0
        ));
        Icon {
            name: "TroupeComet",
            view: 100.0,
            body: Box::leak(body.into_boxed_str()),
        }
    })
}

pub struct Spinner {
    size: f32,
    label: Option<String>,
}

impl Spinner {
    /// A 16 px spinner announced as "Loading".
    pub fn new() -> Spinner {
        Spinner {
            size: 16.0,
            label: Some("Loading".into()),
        }
    }

    /// Diameter in points.
    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    /// The accessible name when the spinner is the only status for this wait.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Hidden from assistive technology: nearby text already names the wait.
    pub fn decorative(mut self) -> Self {
        self.label = None;
        self
    }

    /// The ring in `ink` (the web client's `currentColor`); `still` under reduced motion.
    pub fn build(self, ink: rune::Color, still: bool) -> El {
        let ring = icon(comet(), self.size).color(ink);
        let ring = if still { ring } else { ring.looping(LoopSpec::rotation(0.5)) };
        match self.label {
            Some(l) => ring.role("status").label(l),
            None => ring.a11y_hidden(),
        }
    }
}

impl Default for Spinner {
    fn default() -> Spinner {
        Spinner::new()
    }
}
