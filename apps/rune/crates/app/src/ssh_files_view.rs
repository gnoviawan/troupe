//! The host-key question (`HostKeyQuestion`), the SFTP explorer (`SSHFileExplorer` with its
//! breadcrumb, toolbar, entry rows and drop overlay), the transfer tray (`SftpTransferTray`),
//! and the explorer's dialogs: the delete and replace confirms, the name dialog and the remote
//! file editor (`SSHFileEditor`).

use model::ssh::HostKeyPrompt;
use model::ssh::messages;
use model::ssh::sftp::Entry;
use rune::{El, Tx, col, el, icon, row, text, text_input};
use theme::Tokens;
use ui::overlay::Modal;
use ui::type_::{mono, sans};
use ui::{Button, ButtonSize, ButtonVariant};

use crate::State;
use crate::ssh::{FILE_EDITOR, NAME_DIALOG, NameKind, REMOTE_CONFIRM, RemoteConfirm};
use crate::ssh_view::{Confirm, confirm, glyph};

fn fingerprint(t: &Tokens, value: &str, key: String) -> El {
    let c = &t.colors;
    row()
        .key(key)
        .w_full()
        .px(12.0)
        .py(8.0)
        .radius(8.0)
        .border(1.0, c.border)
        .fill(c.background)
        .child(text(value.to_owned()).font(mono(12.0, 400.0, 16.0)).color(c.foreground).selectable(true).grow(1.0).min_w(0.0))
}

/// The host-key question. A changed key shows the key known_hosts recorded before and the new
/// one, and asks for an explicit "Replace key and connect"; Cancel takes the focus first.
pub fn host_key_question(t: &'static Tokens, tab: &str, prompt: &HostKeyPrompt) -> El {
    let c = &t.colors;
    let changed = prompt.changed();
    let (trust, reject) = (tab.to_owned(), tab.to_owned());
    let mut body = col()
        .key("ssh-host-key")
        .w_full()
        .gap(14.0)
        .role("group")
        .label(prompt.title())
        .child(
            row()
                .gap(10.0)
                .items_start()
                .child(icon(glyph(if changed { "ShieldAlert" } else { "ShieldCheck" }), 18.0).color(if changed { c.destructive } else { c.muted_foreground }))
                .child(
                    col()
                        .min_w(0.0)
                        .grow(1.0)
                        .gap(6.0)
                        .child(text(prompt.title()).font(sans(14.0, 600.0, 20.0)).color(c.foreground).role("heading"))
                        .child(text(prompt.body()).font(sans(13.0, 400.0, 20.0)).color(c.muted_foreground))
                        .children(changed.then(|| text("Continue only if you know why the key changed.").font(sans(13.0, 400.0, 20.0)).color(c.foreground))),
                ),
        );
    if changed {
        let previous = if prompt.previous_fingerprints.is_empty() { vec!["(not readable from known_hosts)".to_owned()] } else { prompt.previous_fingerprints.clone() };
        body = body.child(text(format!("Previous {} key fingerprint", prompt.key_type)).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground));
        for (index, old) in previous.iter().enumerate() {
            body = body.child(fingerprint(t, old, format!("ssh-host-key-old-{index}")));
        }
    }
    body.child(text(prompt.fingerprint_label()).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground))
        .child(fingerprint(t, &prompt.fingerprint, "ssh-host-key-new".into()))
        .child({
            let mut cancel = Button::new("Cancel")
                .variant(if changed { ButtonVariant::Outline } else { ButtonVariant::Ghost })
                .size(ButtonSize::Sm)
                .build(t)
                .key("ssh-host-key-cancel")
                .on_press(move |s: &mut State| s.ssh_answer_host_key(&reject, false));
            let mut accept = Button::new(prompt.trust_label())
                .variant(if changed { ButtonVariant::Destructive } else { ButtonVariant::Default })
                .size(ButtonSize::Sm)
                .build(t)
                .key("ssh-host-key-trust")
                .on_press(move |s: &mut State| s.ssh_answer_host_key(&trust, true));
            if changed {
                cancel = cancel.autofocus();
            } else {
                accept = accept.autofocus();
            }
            row().w_full().gap(8.0).justify_end().child(cancel).child(accept)
        })
}

fn size_label(bytes: u64) -> String {
    match bytes {
        b if b < 1024 => format!("{b} B"),
        b if b < 1024 * 1024 => format!("{:.1} KB", b as f64 / 1024.0),
        b => format!("{:.1} MB", b as f64 / (1024.0 * 1024.0)),
    }
}

/// The explorer beside a connected session's shell.
pub fn explorer(s: &State, t: &'static Tokens, conn: &str, host: &str) -> El {
    let c = &t.colors;
    let state = s.ssh.explorers.get(conn).cloned().unwrap_or_default();
    let tool = |name: &str, label: &str, key: &str| Button::icon_only(glyph(name), label, ButtonSize::IconSm).variant(ButtonVariant::Ghost).build(t).key(key.to_owned());
    let (up, refresh, folder, file, upload) = (conn.to_owned(), conn.to_owned(), conn.to_owned(), conn.to_owned(), conn.to_owned());
    let toolbar = row()
        .key("sftp-toolbar")
        .w_full()
        .h(36.0)
        .px(8.0)
        .gap(2.0)
        .items_center()
        .border_sides([0.0, 0.0, 1.0, 0.0], c.border)
        .child(text(format!("Files on {host}")).font(sans(12.0, 500.0, 18.0)).color(c.foreground).grow(1.0).single_line().truncate().min_w(0.0))
        .child(tool("ArrowUp", "Up one folder", "sftp-up").on_press(move |s: &mut State| s.sftp_up(&up)))
        .child(tool("RefreshCw", "Refresh", "sftp-refresh").on_press(move |s: &mut State| s.sftp_refresh(&refresh)))
        .child(tool("FolderPlus", "New folder", "sftp-new-folder").on_press(move |s: &mut State| s.sftp_ask_name(NameKind::Folder, &folder, None)))
        .child(tool("FilePlus", "New file", "sftp-new-file").on_press(move |s: &mut State| s.sftp_ask_name(NameKind::File, &file, None)))
        .child(tool("Upload", "Upload files (or drop them here)", "sftp-upload").on_press_tx(move |s: &mut State, tx: &mut Tx| s.sftp_pick_upload(&upload, tx)));
    let breadcrumb = row()
        .key("sftp-breadcrumb")
        .w_full()
        .px(12.0)
        .py(6.0)
        .border_sides([0.0, 0.0, 1.0, 0.0], c.border)
        .label(format!("Folder path {}", state.cwd))
        .child(text(if state.cwd.is_empty() { "…".to_owned() } else { state.cwd.clone() }).font(mono(12.0, 400.0, 16.0)).color(c.muted_foreground).single_line().truncate());
    let mut rows = col().key("sftp-entries").w_full().role("list").label("Remote files");
    if let Some(error) = &state.error {
        rows = rows.child(text(error.clone()).font(sans(12.0, 400.0, 18.0)).color(c.destructive).padding(12.0).role("alert"));
    } else if state.loading && state.entries.is_empty() {
        rows = rows.child(text("Loading…").font(sans(12.0, 400.0, 18.0)).color(c.muted_foreground).padding(12.0));
    } else if state.entries.is_empty() {
        rows = rows.child(text("This folder is empty").font(sans(12.0, 400.0, 18.0)).color(c.muted_foreground).padding(12.0));
    }
    for entry in &state.entries {
        rows = rows.child(entry_row(t, conn, entry));
    }
    let drop_conn = conn.to_owned();
    let list = col()
        .key("sftp-drop-target")
        .grow(1.0)
        .w_full()
        .min_h(0.0)
        .child(rows)
        .scroll(rune::ScrollSpec { stick_bottom: false })
        .on_drop_files(move |s: &mut State, files: &[std::path::PathBuf], _tx: &mut Tx| {
            let locals = files.iter().map(|f| f.to_string_lossy().into_owned()).collect();
            s.sftp_upload(&drop_conn, locals);
        });
    col()
        .key("sftp-explorer")
        .w(360.0)
        .h_full()
        .border_sides([0.0, 0.0, 0.0, 1.0], c.border)
        .fill(c.card)
        .child(toolbar)
        .child(breadcrumb)
        .child(list)
        .children(tray(s, t, conn))
}

/// One remote entry (`SftpEntryRow`): open (a folder) or edit (a file) on press, with its
/// download, rename and delete actions.
fn entry_row(t: &'static Tokens, conn: &str, entry: &Entry) -> El {
    let c = &t.colors;
    let key = entry.path.clone();
    let act = |name: &str, label: &str, what: &str| {
        Button::icon_only(glyph(name), label, ButtonSize::IconXs).variant(ButtonVariant::Ghost).build(t).key(format!("sftp-{what}-{key}"))
    };
    let (open, down, rename, delete) = ((conn.to_owned(), entry.clone()), (conn.to_owned(), entry.clone()), (conn.to_owned(), entry.clone()), (conn.to_owned(), entry.clone()));
    let mut line = row()
        .key(format!("sftp-entry-{key}"))
        .role("listitem")
        .label(if entry.is_dir() { entry.path.clone() } else { format!("Open {}", entry.name) })
        .w_full()
        .h(28.0)
        .px(10.0)
        .gap(8.0)
        .items_center()
        .hover(c.row_hover)
        .child(icon(glyph(if entry.is_dir() { "Folder" } else { "FileText" }), 14.0).color(c.muted_foreground).a11y_hidden())
        .child(text(entry.name.clone()).font(sans(12.0, 400.0, 18.0)).color(c.foreground).grow(1.0).min_w(0.0).single_line().truncate())
        .on_press(move |s: &mut State| s.sftp_activate(&open.0, &open.1));
    if !entry.is_dir() {
        line = line
            .child(text(size_label(entry.size)).font(sans(11.0, 400.0, 16.0)).color(c.muted_foreground))
            .child(act("Download", "Download", "download").on_press_tx(move |s: &mut State, tx: &mut Tx| s.sftp_pick_download(&down.0, &down.1, tx)));
    }
    line.child(act("Edit", "Rename", "rename").on_press(move |s: &mut State| s.sftp_ask_name(NameKind::Rename, &rename.0, Some(rename.1.clone()))))
        .child(act("Trash2", "Delete", "delete").on_press(move |s: &mut State| s.sftp_request_delete(&delete.0, &delete.1)))
}

/// The transfer tray: this session's uploads and downloads, newest last.
fn tray(s: &State, t: &'static Tokens, conn: &str) -> Option<El> {
    let c = &t.colors;
    let transfers: Vec<_> = s.ssh.model.transfers.iter().filter(|x| x.connection_id == conn).collect();
    if transfers.is_empty() {
        return None;
    }
    let finished = transfers.iter().any(|x| !x.running());
    let clear = conn.to_owned();
    let mut tray = col()
        .key("sftp-tray")
        .w_full()
        .padding(10.0)
        .gap(6.0)
        .border_sides([1.0, 0.0, 0.0, 0.0], c.border)
        .role("region")
        .label("Transfers")
        .child(
            row()
                .w_full()
                .items_center()
                .child(text("Transfers").font(sans(12.0, 600.0, 18.0)).color(c.foreground).grow(1.0))
                .children(finished.then(|| {
                    Button::new("Clear").variant(ButtonVariant::Ghost).size(ButtonSize::Xs).build(t).key("sftp-tray-clear").on_press(move |s: &mut State| s.ssh.model.clear_completed_transfers(Some(&clear)))
                })),
        );
    for transfer in transfers {
        let name = model::ssh::sftp::local_file_name(&transfer.remote_path);
        let fraction = if transfer.total_bytes > 0 { transfer.bytes_transferred as f32 / transfer.total_bytes as f32 } else if transfer.running() { 0.0 } else { 1.0 };
        let (status, ink) = match transfer.status.as_str() {
            "completed" => ("Done".to_owned(), c.success),
            "failed" => (transfer.error.clone().unwrap_or_else(|| "Failed".into()), c.destructive),
            "cancelled" => ("Cancelled".to_owned(), c.muted_foreground),
            _ => (format!("{}%", (fraction * 100.0).round()), c.muted_foreground),
        };
        let arrow = if transfer.direction == "upload" { "Upload" } else { "Download" };
        tray = tray.child(
            col()
                .key(format!("sftp-transfer-{}-{}", transfer.direction, transfer.remote_path))
                .w_full()
                .gap(4.0)
                .child(
                    row()
                        .w_full()
                        .gap(6.0)
                        .items_center()
                        .child(icon(glyph(arrow), 12.0).color(c.muted_foreground))
                        .child(text(name).font(sans(12.0, 400.0, 18.0)).color(c.foreground).grow(1.0).min_w(0.0).single_line().truncate())
                        .child(text(status).font(sans(11.0, 400.0, 16.0)).color(ink).single_line()),
                )
                .child(row().w_full().h(3.0).radius(2.0).fill(c.secondary).child(el().h_full().w_frac(fraction.clamp(0.0, 1.0)).radius(2.0).fill(ink))),
        );
    }
    Some(tray)
}

pub fn dialogs(s: &State, t: &'static Tokens, layer: &mut Vec<El>) {
    remote_confirm(s, t, layer);
    name_dialog(s, t, layer);
    file_editor(s, t, layer);
}

/// The delete and replace confirms, naming the target captured when they opened.
fn remote_confirm(s: &State, t: &'static Tokens, layer: &mut Vec<El>) {
    let Some(pending) = s.ssh.confirm.as_ref().filter(|_| s.overlays.is_open(REMOTE_CONFIRM)) else { return };
    let (title, message, label) = match pending {
        RemoteConfirm::Delete { entry, host, .. } => {
            let (title, message) = messages::delete_remote(&entry.name, entry.is_dir(), host);
            (title.to_owned(), message, "Delete")
        }
        RemoteConfirm::Replace { dir, conflicts, host, .. } => {
            let (title, message) = messages::replace_remote(conflicts, dir, host);
            (title, message, "Replace")
        }
    };
    let spec = Confirm { key: REMOTE_CONFIRM, title: &title, message, label, on_confirm: State::sftp_confirm, on_cancel: State::sftp_cancel_confirm };
    confirm(t, spec, layer);
}

fn name_dialog(s: &State, t: &'static Tokens, layer: &mut Vec<El>) {
    let Some((kind, _, _)) = s.ssh.naming.as_ref().filter(|_| s.overlays.is_open(NAME_DIALOG)) else { return };
    let c = &t.colors;
    let (title, label) = match kind {
        NameKind::Folder => ("New folder", "Create"),
        NameKind::File => ("New file", "Create"),
        NameKind::Rename => ("Rename", "Rename"),
    };
    let input = text_input(&s.ssh.name, sans(14.0, 400.0, 20.0))
        .single_line()
        .key("sftp-name")
        .label("Name")
        .color(c.foreground)
        .w_full()
        .h(34.0)
        .px(12.0)
        .radius(4.0)
        .fill(c.secondary)
        .border(1.0, c.border)
        .focus_ring(1.0, 0.0, c.focus_ring)
        .autofocus()
        .on_edit(|s: &mut State| &mut s.ssh.name)
        .on_submit(|s: &mut State, _tx: &mut Tx| s.sftp_submit_name());
    let footer = row()
        .w_full()
        .gap(8.0)
        .justify_end()
        .child(Button::new("Cancel").variant(ButtonVariant::Ghost).size(ButtonSize::Sm).build(t).key("cancel").on_press(|s: &mut State| {
            s.ssh.naming = None;
            s.close(NAME_DIALOG);
        }))
        .child(Button::new(label).size(ButtonSize::Sm).build(t).key("sftp-name-submit").on_press(|s: &mut State| s.sftp_submit_name()));
    Modal::new(NAME_DIALOG, title).width(400.0).child(col().w_full().gap(16.0).child(input).child(footer)).open(true).on_close(|s: &mut State| {
        s.ssh.naming = None;
        s.close(NAME_DIALOG);
    })
    .build(t, layer);
}

/// `SSHFileEditor`: the remote file's text, saved back over SFTP.
fn file_editor(s: &State, t: &'static Tokens, layer: &mut Vec<El>) {
    let Some(file) = s.ssh.model.editing_file.as_ref().filter(|_| s.overlays.is_open(FILE_EDITOR)) else { return };
    let c = &t.colors;
    let dirty = s.ssh.file_text.text != file.original_content;
    let editor = text_input(&s.ssh.file_text, mono(13.0, 400.0, 18.0))
        .key("sftp-file-text")
        .label(format!("Contents of {}", file.name))
        .color(c.foreground)
        .w_full()
        .h(420.0)
        .padding(10.0)
        .radius(4.0)
        .fill(c.terminal_bg)
        .border(1.0, c.border)
        .focus_ring(1.0, 0.0, c.focus_ring)
        .autofocus()
        .on_edit(|s: &mut State| &mut s.ssh.file_text);
    let footer = row()
        .w_full()
        .gap(8.0)
        .items_center()
        .child(text(file.path.clone()).font(mono(12.0, 400.0, 16.0)).color(c.muted_foreground).grow(1.0).min_w(0.0).single_line().truncate())
        .child(Button::new("Close").variant(ButtonVariant::Ghost).size(ButtonSize::Sm).build(t).key("sftp-file-close").on_press(|s: &mut State| s.sftp_close_file()))
        .child(Button::new(if dirty { "Save" } else { "Saved" }).size(ButtonSize::Sm).icon(glyph("Save")).disabled(!dirty).build(t).key("sftp-file-save").on_press(|s: &mut State| s.sftp_save_file()));
    Modal::new(FILE_EDITOR, &file.name).width(760.0).child(col().w_full().gap(12.0).child(editor).child(footer)).open(true).on_close(|s: &mut State| s.sftp_close_file()).build(t, layer);
}
