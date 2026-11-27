//! The Report a Bug dialog (`ReportBugDialog`): it collects the host's redacted diagnostics when
//! it opens, previews the exact markdown, and files through the host's `gh`
//! (`POST /log/report-bug`). When `gh` is unavailable or fails, the report goes to the clipboard
//! and a prefilled `issues/new` page opens. Boundary logs carry kinds and counts only: never
//! the title, the body or the diagnostics. Screenshots and "Draft with AI" are not ported.

use client::http::{HttpError, HttpRequest, Method};
use model::bug_report::{self, Cause, Context, Outcome};
use rune::{El, TextEdit, Tx, col, icon, row, text, text_input};
use serde_json::{Value, json};
use theme::Tokens;
use ui::overlay::{Dialog, ToastKind};
use ui::type_::{mono, sans};
use ui::{Button, ButtonSize, ButtonVariant};

use crate::State;

pub const KEY: &str = "report-bug";

/// What a `/log/*` reply answers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// `POST /log/diagnostics`, for the dialog's open number `n`.
    Collect(u64),
    Submit,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum Collect {
    #[default]
    Collecting,
    Ready(Context),
    Unavailable,
}

#[derive(Default)]
pub struct BugReportUi {
    pub title: TextEdit,
    pub description: TextEdit,
    pub field_error: Option<&'static str>,
    pub collect: Collect,
    pub preview_open: bool,
    pub submitting: bool,
    /// Bumps with every open, so a late collect of an earlier open is dropped.
    opened: u64,
    /// A fallback waiting for the next frame's clipboard and browser: the markdown and the URL.
    fallback: Option<(String, String, Cause, Option<String>)>,
    /// Every request sent, for the tests.
    #[cfg(test)]
    pub(crate) sent: Vec<(Step, HttpRequest)>,
}

fn log_boundary(message: &str) {
    log::warn!("[ReportBugDialog] {message}");
}

impl State {
    fn bug_send(&mut self, step: Step, request: HttpRequest) {
        #[cfg(test)]
        self.bug.sent.push((step.clone(), request.clone()));
        if let Some(host) = &mut self.host {
            host.request(request, crate::host::Fetch::BugReport(step));
        }
    }

    /// Opens the dialog with an empty draft and starts the diagnostics collect.
    pub(crate) fn open_bug_report(&mut self) {
        let b = &mut self.bug;
        b.opened += 1;
        b.title = TextEdit::new("");
        b.description = TextEdit::new("");
        b.field_error = None;
        b.collect = Collect::Collecting;
        b.preview_open = false;
        b.submitting = false;
        let opened = b.opened;
        self.overlays.open(KEY);
        let request = HttpRequest { method: Method::Post, path: "/log/diagnostics".into(), body: Some(json!({})), command_id: None };
        self.bug_send(Step::Collect(opened), request);
    }

    /// The context the report is built on: the collected one, or the honest placeholder.
    fn bug_context(&self) -> Context {
        match &self.bug.collect {
            Collect::Ready(context) => context.clone(),
            _ => Context::unavailable(),
        }
    }

    /// The preview: none while the collect runs.
    pub(crate) fn bug_markdown(&self) -> Option<String> {
        (self.bug.collect != Collect::Collecting)
            .then(|| bug_report::markdown(&self.bug_context(), &self.bug.title.text, &self.bug.description.text, 0))
    }

    pub(crate) fn bug_report_fetched(&mut self, step: Step, result: Result<Value, HttpError>) {
        match step {
            Step::Collect(opened) if opened == self.bug.opened => {
                self.bug.collect = match result.ok().and_then(|data| serde_json::from_value::<Context>(data).ok()) {
                    Some(context) => Collect::Ready(context),
                    None => {
                        log::warn!("[bug-report-api.collectBugReportContext] collect_bug_report_context failed");
                        Collect::Unavailable
                    }
                };
            }
            Step::Collect(_) => {}
            Step::Submit => self.bug_submitted(result),
        }
    }

    /// Submit report (`submit`): a title is required; the host's `gh` files it.
    pub(crate) fn submit_bug_report(&mut self) {
        let title = model::js_trim(&self.bug.title.text).to_owned();
        if title.is_empty() {
            self.bug.field_error = Some("Give the report a short title.");
            return;
        }
        if self.bug.submitting || self.bug.collect == Collect::Collecting {
            return;
        }
        self.bug.submitting = true;
        self.bug.field_error = None;
        let body = bug_report::markdown(&self.bug_context(), &self.bug.title.text, &self.bug.description.text, 0);
        let request = HttpRequest {
            method: Method::Post,
            path: "/log/report-bug".into(),
            body: Some(json!({ "title": title, "body": body })),
            command_id: None,
        };
        self.bug_send(Step::Submit, request);
    }

    fn bug_submitted(&mut self, result: Result<Value, HttpError>) {
        self.bug.submitting = false;
        let title = model::js_trim(&self.bug.title.text).to_owned();
        let body = bug_report::markdown(&self.bug_context(), &self.bug.title.text, &self.bug.description.text, 0);
        let outcome = match result {
            Ok(data) => serde_json::from_value::<Outcome>(data).ok(),
            Err(e) => {
                let code = match &e {
                    HttpError::Refused { code, .. } => code.clone(),
                    _ => "NETWORK_ERROR".into(),
                };
                log::warn!("[bug-report-api.submitBugReport] submit_bug_report failed (code={code})");
                None
            }
        };
        let (cause, reason) = match outcome {
            Some(Outcome::Submitted { issue_url }) => {
                log::info!("[bug-report-api.submitBugReport] submit_bug_report filed issue {issue_url}");
                self.toasts.push(self.now, ToastKind::Success, "Bug report filed", Some("The GitHub issue was created."));
                self.close(KEY);
                return;
            }
            Some(Outcome::Unavailable { reason }) => (Cause::Unavailable, Some(reason)),
            Some(Outcome::Failed { reason }) => (Cause::Failed, Some(reason)),
            None => (Cause::Transport, None),
        };
        log_boundary(&format!("submit fell back to clipboard + prefilled issue ({cause:?})"));
        let url = bug_report::issue_url(&self.bug_context(), &title, &body, true);
        self.bug.fallback = Some((body, url, cause, reason));
    }

    /// Runs a waiting fallback: the report onto the clipboard, the prefilled issue in the
    /// browser, and the toast that says so. The dialog stays open.
    pub(crate) fn bug_report_tick(&mut self, tx: &mut Tx) {
        let Some((body, url, cause, reason)) = self.bug.fallback.take() else { return };
        tx.copy(body);
        let opened = match tx.open_url(&url) {
            Ok(()) => true,
            Err(e) => {
                log_boundary(&format!("opening the prefilled issue failed ({e})"));
                false
            }
        };
        let why = bug_report::fallback_cause(cause, reason.as_deref());
        let (title, detail) = if opened {
            ("Report copied — prefilled issue opened", format!("{why} Paste the copied report into the issue in your browser."))
        } else {
            ("Report copied to clipboard", format!("{why} The browser could not be opened — paste the report into a new GitHub issue."))
        };
        self.toasts.push(self.now, ToastKind::Info, title, Some(&detail));
    }
}

fn glyph(name: &str) -> &'static rune::Icon {
    theme::icons::named(name).unwrap_or(&theme::icons::X)
}

fn caption(t: &Tokens, words: &str) -> El {
    text(words.to_owned()).font(sans(12.0, 500.0, 16.0)).color(t.colors.foreground)
}

pub fn build(s: &State, t: &'static Tokens, phone: bool, layer: &mut Vec<El>) {
    if !s.overlays.is_open(KEY) {
        return;
    }
    let c = &t.colors;
    let b = &s.bug;
    let busy = b.submitting;
    let field = |edit: &TextEdit, name: &str, placeholder: &str| {
        text_input(edit, sans(if phone { 16.0 } else { 14.0 }, 400.0, 20.0))
            .label(name.to_owned())
            .placeholder(placeholder.to_owned(), c.muted_foreground)
            .color(c.foreground)
            .w_full()
            .px(12.0)
            .radius(6.0)
            .fill(c.background)
            .border(1.0, c.input)
            .focus_ring(1.0, 0.0, c.focus_ring)
    };
    let title = field(&b.title, "Title", "Short summary of the bug")
        .single_line()
        .key("bug-title")
        .h(36.0)
        .autofocus()
        .on_edit(|s: &mut State| {
            s.bug.field_error = None;
            &mut s.bug.title
        });
    let description = field(&b.description, "What happened?", "Steps to reproduce, what you expected, and what actually happened.")
        .key("bug-description")
        .h(96.0)
        .py(8.0)
        .on_edit(|s: &mut State| &mut s.bug.description);
    let chevron = if b.preview_open { "ChevronUp" } else { "ChevronDown" };
    let toggle = row()
        .key("bug-preview-toggle")
        .role("button")
        .label("Preview — the exact markdown sent to GitHub")
        .w_full()
        .px(10.0)
        .py(6.0)
        .radius(6.0)
        .items_center()
        .justify_between()
        .fill(c.secondary.alpha(0.3))
        .hover(c.secondary.alpha(0.6))
        .border(1.0, c.border)
        .child(text("Preview — the exact markdown sent to GitHub").font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground))
        .child(icon(glyph(chevron), 14.0).color(c.muted_foreground))
        .on_press(|s: &mut State| s.bug.preview_open = !s.bug.preview_open);
    let mut body = col()
        .key("bug-body")
        .w_full()
        .gap(12.0)
        .child(col().w_full().gap(6.0).child(caption(t, "Title")).child(title))
        .child(col().w_full().gap(6.0).child(caption(t, "What happened?")).child(description))
        .child(toggle);
    if b.preview_open {
        if b.collect == Collect::Unavailable {
            body = body.child(
                text("Host diagnostics could not be collected — environment fields show as \"unavailable\". The report still works.")
                    .font(sans(11.0, 400.0, 14.0))
                    .color(c.warning)
                    .wrap()
                    .w_full(),
            );
        }
        let preview = s.bug_markdown().unwrap_or_else(|| "Collecting diagnostics…".into());
        body = body.child(
            col()
                .key("report-markdown-preview")
                .w_full()
                .h(224.0)
                .padding(10.0)
                .radius(6.0)
                .fill(c.secondary.alpha(0.3))
                .border(1.0, c.border)
                .child(text(preview).font(mono(11.0, 400.0, 16.0)).color(c.muted_foreground).wrap().w_full())
                .scroll(rune::ScrollSpec { stick_bottom: false }),
        );
    }
    if let Some(problem) = b.field_error {
        body = body.child(text(problem).font(sans(12.0, 400.0, 16.0)).color(c.destructive));
    }
    let cancel = Button::new("Cancel")
        .variant(ButtonVariant::Outline)
        .size(ButtonSize::Sm)
        .disabled(busy)
        .build(t)
        .key("bug-cancel")
        .on_press(|s: &mut State| {
            if !s.bug.submitting {
                s.close(KEY);
            }
        });
    let submit = Button::new(if busy { "Submitting…" } else { "Submit report" })
        .size(ButtonSize::Sm)
        .disabled(busy || b.collect == Collect::Collecting)
        .build(t)
        .key("bug-submit")
        .on_press(|s: &mut State| s.submit_bug_report());
    Dialog::new(KEY, "Report a bug")
        .description("Describe what went wrong. The preview below is the exact markdown sent to GitHub — nothing is filed silently.")
        .width(560.0)
        .child(body)
        .footer(cancel)
        .footer(submit)
        .open(true)
        .on_close(|s: &mut State| {
            if !s.bug.submitting {
                s.close(KEY);
            }
        })
        .build(t, layer);
}
