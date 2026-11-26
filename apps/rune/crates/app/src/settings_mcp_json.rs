//! The MCP server dialog's JSON view (`McpServerDialog.tsx`): "Import JSON instead" on Add and
//! "Edit as JSON" on Edit, "Use the form" back, and Save from the JSON. The rules are
//! `model::settings::mcp_json`.

use model::acp::Event;
use model::settings::mcp::{self as rules, Object};
use model::settings::mcp_json as json_rules;
use rune::{El, TextEdit, col, text};
use serde_json::json;
use theme::Tokens;
use ui::overlay::Dialog;
use ui::settings::{Tone, toned_notice};
use ui::type_::{mono, sans};
use ui::{Button, ButtonSize, ButtonVariant, Textarea};

use crate::State;
use crate::settings_mcp::Action;
use crate::settings_mcp_dialog::{DIALOG_CHROME, ServerDialog};

/// The JSON view's text and the errors its last Save or "Use the form" found.
#[derive(Default)]
pub struct JsonView {
    pub text: TextEdit,
    pub errors: Vec<String>,
}

fn log_rejection(errors: &[String]) {
    log::warn!("[settings.McpServersSettings] MCP server JSON rejected: {}", errors.join(" | "));
}

impl State {
    /// `showJson`: the form as JSON; a blank Add form starts empty.
    pub(crate) fn mcp_show_json(&mut self) {
        let Some(d) = &mut self.mcp_ui.dialog else { return };
        let draft = d.draft();
        let mut config = rules::unchecked_config(&draft);
        let text = match &d.editing {
            Some(server) => {
                config.insert("id".into(), server["id"].clone());
                if let Some(enabled) = server.get("enabled") {
                    config.insert("enabled".into(), enabled.clone());
                }
                json_rules::server_to_json(&config)
            }
            None if json_rules::is_blank_draft(&draft) => String::new(),
            None => json_rules::server_to_json(&config),
        };
        d.json = Some(JsonView { text: TextEdit::new(text), errors: Vec::new() });
    }

    /// `showForm`: the JSON back into the form when it holds exactly one valid server.
    pub(crate) fn mcp_show_form(&mut self) {
        let Some(d) = &mut self.mcp_ui.dialog else { return };
        let Some(text) = d.json.as_ref().map(|v| v.text.text.clone()) else { return };
        let mut draft = d.draft();
        if !text.trim().is_empty() {
            let parsed = json_rules::parse_import(&text);
            if !parsed.errors.is_empty() || parsed.servers.len() != 1 {
                if let Some(view) = &mut d.json {
                    view.errors = vec![json_rules::FORM_NEEDS_ONE_SERVER.into()];
                }
                return;
            }
            draft = rules::draft_from_server(&parsed.servers[0]);
        }
        let mut editing = d.editing.clone();
        if let (Some(server), Some(enabled)) = (&mut editing, json_rules::json_enabled(&text)) {
            server.insert("enabled".into(), json!(enabled));
        }
        *d = ServerDialog::of(&draft, editing);
    }

    fn json_errors(&mut self, errors: Vec<String>) {
        if let Some(view) = self.mcp_ui.dialog.as_mut().and_then(|d| d.json.as_mut()) {
            view.errors = errors;
        }
    }

    /// `saveJson`: Add imports every server in the JSON; Edit saves the one server over the target.
    pub(crate) fn mcp_save_json(&mut self) {
        let Some(d) = &self.mcp_ui.dialog else { return };
        let Some(view) = &d.json else { return };
        let text = view.text.text.clone();
        if text.trim().is_empty() || d.saving {
            return;
        }
        let editing = d.editing.clone();
        self.json_errors(Vec::new());
        let explicit = editing.as_ref().and_then(|_| json_rules::json_enabled(&text));
        if editing.is_some() && json_rules::has_wrapper(&text) {
            let errors = vec![json_rules::EDIT_WRAPPER.to_owned()];
            log_rejection(&errors);
            return self.json_errors(errors);
        }
        let parsed = json_rules::parse_import(&text);
        if !parsed.errors.is_empty() {
            log_rejection(&parsed.errors);
            return self.json_errors(parsed.errors);
        }
        match editing {
            None if parsed.servers.is_empty() => {
                let errors = vec![json_rules::NO_SERVERS.to_owned()];
                log_rejection(&errors);
                self.json_errors(errors);
            }
            None => {
                let batch: Vec<Object> = parsed
                    .servers
                    .into_iter()
                    .map(|mut s| {
                        s.insert("id".into(), json!(crate::ids::uuid()));
                        s.insert("enabled".into(), json!(true));
                        s
                    })
                    .collect();
                self.mcp_dialog_saving();
                self.mcp_write(Action::Add(batch.len()), Event::ImportMcpServers(batch));
            }
            Some(_) if parsed.servers.len() != 1 => self.json_errors(vec![json_rules::EDIT_ONE.to_owned()]),
            Some(target) => {
                let config = parsed.servers.into_iter().next().unwrap_or_default();
                let enabled = explicit.unwrap_or(target.get("enabled") != Some(&json!(false)));
                self.mcp_dialog_saving();
                self.mcp_commit_edit(&target, config, enabled);
            }
        }
    }
}

/// The JSON view of the Add and Edit dialog.
pub fn json_dialog(t: &'static Tokens, d: &ServerDialog, view: &JsonView, height: f32, layer: &mut Vec<El>) {
    let c = &t.colors;
    let adding = d.editing.is_none();
    let area = Textarea::new(&view.text)
        .rows(12)
        .placeholder(json_rules::PLACEHOLDER)
        .label("MCP JSON")
        .disabled(d.saving)
        .build(t)
        .key("mcp-json")
        .on_edit(|s: &mut State| &mut s.mcp_ui.dialog.as_mut().and_then(|d| d.json.as_mut()).expect("the JSON view is open").text);
    let mut body = col()
        .key("mcp-json-view")
        .w_full()
        .gap(16.0)
        .items_stretch()
        .child(col().key("json-field").gap(4.0).items_stretch().child(text("MCP JSON").font(sans(14.0, 400.0, 20.0)).color(c.foreground)).child(area));
    if !view.errors.is_empty() {
        let mut list = col().key("json-errors").role("alert").gap(4.0).items_stretch();
        for (i, e) in view.errors.iter().enumerate() {
            list = list.child(text(e.clone()).font(mono(12.0, 400.0, 16.0)).color(c.destructive).wrap().key(format!("error-{i}")));
        }
        body = body.child(list);
    }
    body = body.child(toned_notice(t, "secrets", Tone::Warning, "Environment values and headers are saved in app data. Use $VARIABLE references instead of real secrets."));
    if adding {
        body = body.child(text("Only add servers from developers you trust. A server's tools can read and change things on your behalf, and Troupe can't check what they do.").font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground).wrap());
    }
    let form = Button::new("Use the form").variant(ButtonVariant::Link).size(ButtonSize::Sm).disabled(d.saving).build(t).key("use-form").on_press(|s: &mut State| s.mcp_show_form());
    let cancel = Button::new("Cancel").variant(ButtonVariant::Outline).size(ButtonSize::Sm).disabled(d.saving).build(t).key("cancel").on_press(|s: &mut State| s.close_mcp_dialog());
    let save = Button::new(if d.saving { "Saving…" } else { "Save" })
        .size(ButtonSize::Sm)
        .disabled(view.text.text.trim().is_empty() || d.saving)
        .build(t)
        .key("save-json")
        .on_press(|s: &mut State| s.mcp_save_json());
    Dialog::new("mcp-server-dialog", if adding { "Add MCP servers" } else { "Edit MCP server" })
        .description(if adding { "Paste a Claude Desktop \"mcpServers\" config or a single server object." } else { "Edit this server as JSON. Unknown fields are dropped on save." })
        .width(576.0)
        .child(col().key("mcp-form-scroll").w_full().max_h((height * 0.9 - DIALOG_CHROME).max(160.0)).child(body).scroll(rune::ScrollSpec { stick_bottom: false }))
        .footer(form)
        .footer(cancel)
        .footer(save)
        .open(true)
        .on_close(|s: &mut State| s.close_mcp_dialog())
        .build(t, layer);
}
