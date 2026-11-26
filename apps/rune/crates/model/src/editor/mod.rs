//! The open text files of the editor (`stores/editor-store.ts`, text files only): each file's
//! buffer, the text on disk it was read at, its save and its unsaved draft.
//!
//! [`step`] applies one event and returns the [`Effect`]s the app carries out: read a file,
//! write it with the hash it was read at as the precondition, log, toast. A draft the workspace
//! record kept (`OpenFile.draftContent`) is reconciled with the file read from disk as
//! `reconcileDraft` does: the same text is nothing, a disk file not newer than the draft takes
//! the draft as an unsaved edit, and a newer disk file shows the draft conflict bar.

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};
use troupe_editor::{Editor, Rope};

use crate::Level;

pub mod markdown;
pub mod media;

/// A draft the record kept for a file.
#[derive(Clone, Debug, PartialEq)]
pub struct Draft {
    pub content: String,
    /// Milliseconds since the epoch.
    pub saved_at: f64,
}

/// What a `GET /fs/read` returned (`HashedFileContent`).
#[derive(Clone, Debug, PartialEq)]
pub struct Read {
    pub content: String,
    /// Milliseconds since the epoch.
    pub modified_at: f64,
    pub content_hash: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Load {
    Loading,
    Ready,
    /// The file opens as `UnsupportedFileView`: too large, binary, gone or unreadable.
    Unavailable(Unavailable),
}

/// Why a file has no text to edit (`PreviewUnavailable`).
#[derive(Clone, Debug, PartialEq)]
pub enum Unavailable {
    Binary,
    TooLarge { size: Option<u64>, limit: Option<u64> },
    Missing,
    Unreadable(String),
}

impl Unavailable {
    /// The host's refusal of a read: its code and message.
    #[must_use]
    pub fn from_refusal(code: &str, message: &str) -> Self {
        match code {
            "BINARY_FILE" => Self::Binary,
            "FILE_TOO_LARGE" => {
                let numbers: Vec<u64> = message
                    .split_once('(')
                    .map(|(_, rest)| rest.split(|c: char| !c.is_ascii_digit()).filter_map(|n| n.parse().ok()).collect())
                    .unwrap_or_default();
                Self::TooLarge { size: numbers.first().copied(), limit: numbers.get(1).copied() }
            }
            "NOT_FOUND" => Self::Missing,
            _ => Self::Unreadable(message.to_owned()),
        }
    }

    #[must_use]
    pub fn headline(&self) -> &'static str {
        match self {
            Self::Binary => "This file can't be previewed",
            Self::TooLarge { .. } => "This file is too large to preview",
            Self::Missing => "This file no longer exists",
            Self::Unreadable(_) => "This file couldn't be read",
        }
    }

    #[must_use]
    pub fn detail(&self) -> Option<String> {
        match self {
            Self::TooLarge { size: Some(size), limit: Some(limit) } => {
                Some(format!("{}, the preview limit is {}", format_size(*size), format_size(*limit)))
            }
            Self::TooLarge { size: Some(size), .. } => Some(format_size(*size)),
            Self::Unreadable(message) => Some(message.clone()),
            _ => None,
        }
    }
}

/// `formatSize` (`lib/sftp-format.ts`).
#[must_use]
pub fn format_size(bytes: u64) -> String {
    let b = bytes as f64;
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1_048_576 {
        format!("{:.1} KB", b / 1024.0)
    } else if bytes < 1_073_741_824 {
        format!("{:.1} MB", b / 1_048_576.0)
    } else {
        format!("{:.1} GB", b / 1_073_741_824.0)
    }
}

/// One open text file.
#[derive(Clone)]
pub struct Doc {
    pub path: String,
    pub editor: Editor,
    /// The text on disk as last read or written.
    pub saved: Rope,
    /// The sha256 of `saved`: the precondition of the next write.
    pub hash: Option<String>,
    /// Milliseconds since the epoch: the disk file's time, or the last save's.
    pub modified_at: f64,
    pub load: Load,
    /// A write is in flight with this content.
    pub saving: Option<String>,
    /// A save asked for while a write was in flight or the read was loading: it runs once
    /// that lands, against the buffer as it is then (`savesInFlight`).
    pub queued: bool,
    /// A kept draft that differs from a newer file on disk; the buffer is read-only until the
    /// user keeps one.
    pub draft_conflict: Option<Draft>,
    /// The host refused the last save because the file changed on disk.
    pub external_conflict: bool,
    /// The disk file broke every line with `\r\n`. The buffer holds `\n` (as CodeMirror
    /// reads it); a write puts the `\r\n` back, so an edit changes only the lines it touched.
    pub crlf: bool,
    /// The disk file starts with a byte order mark, which the buffer leaves out and a write
    /// puts back.
    pub bom: bool,
    /// Reads that replaced the buffer: a new buffer restarts its revision at 0, so caches key
    /// on this and the revision together ([`Doc::version`]).
    loads: u64,
    /// A draft to reconcile once the read lands.
    pending_draft: Option<Draft>,
}

impl Doc {
    fn new(path: &str, draft: Option<Draft>) -> Self {
        Self {
            path: path.to_owned(),
            editor: Editor::new(""),
            saved: Rope::new(),
            hash: None,
            modified_at: 0.0,
            load: Load::Loading,
            saving: None,
            queued: false,
            draft_conflict: None,
            external_conflict: false,
            crlf: false,
            bom: false,
            loads: 0,
            pending_draft: draft,
        }
    }

    /// The buffer differs from the disk text.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.load == Load::Ready && *self.editor.rope() != self.saved
    }

    /// Unsaved work the unsaved-changes dialog guards: dirty, or a conflicted draft.
    #[must_use]
    pub fn needs_attention(&self) -> bool {
        self.is_dirty() || self.draft_conflict.is_some()
    }

    #[must_use]
    pub fn read_only(&self) -> bool {
        self.draft_conflict.is_some() || self.load != Load::Ready
    }

    /// Changes whenever the buffer's text may have: the read count and the edit revision.
    #[must_use]
    pub fn version(&self) -> (u64, u64) {
        (self.loads, self.editor.revision())
    }

    /// The buffer as the disk holds it: with the file's own line breaks.
    #[must_use]
    pub fn disk_text(&self) -> String {
        let text = self.editor.text();
        let text = if self.crlf { text.replace('\n', "\r\n") } else { text };
        if self.bom { format!("{BOM}{text}") } else { text }
    }

    /// The draft the record keeps: the conflicted draft, else the dirty buffer.
    #[must_use]
    pub fn draft(&self) -> Option<Draft> {
        if let Some(conflict) = &self.draft_conflict {
            return Some(conflict.clone());
        }
        self.is_dirty().then(|| Draft { content: self.editor.text(), saved_at: self.modified_at })
    }
}

/// Every open text file, by path.
#[derive(Clone, Default)]
pub struct Docs {
    pub docs: BTreeMap<String, Doc>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// Open `path`, with the draft the record kept for it.
    Open { path: String, draft: Option<Draft> },
    /// The read answered: the file, or the host's code and message.
    Loaded { path: String, result: Result<Read, (String, String)> },
    /// Ctrl+S, or the unsaved-changes dialog's Save.
    Save { path: String },
    /// The write of `content` answered: Ok, or the host's code and message.
    Written { path: String, content: String, result: Result<(), (String, String)> },
    /// The draft conflict bar: keep the draft (true) or the disk version.
    ResolveDraft { path: String, keep_draft: bool },
    /// "Reload" after a refused save: read the disk version again.
    Reload { path: String },
    /// "Overwrite" after a refused save: write without a precondition.
    Overwrite { path: String },
    /// The file read back after a write whose reply was lost: its text, or `None` when the
    /// read failed too.
    ReadBack { path: String, content: String, disk: Option<String> },
    Close { path: String },
    /// The tab was renamed through the explorer.
    Rename { from: String, to: String },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// `GET /fs/read?path=`.
    Read { path: String },
    /// `POST /fs/write`; `expected` is the sha256 precondition.
    Write { path: String, content: String, expected: Option<String> },
    /// A save landed (or there was nothing to write). `clean`: the buffer still equals what
    /// is on disk, so a tab waiting to close may close.
    Saved { path: String, clean: bool },
    /// A save failed; the buffer stays dirty.
    SaveFailed { path: String, message: String },
    /// The write's reply was lost: read the file to learn whether it landed.
    ReadBack { path: String, content: String },
    /// "File changed externally" with Reload and Overwrite.
    ExternalChange { path: String },
    Log { level: Level, message: String },
}

fn log(level: Level, message: String) -> Effect {
    Effect::Log { level, message }
}

fn name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// The lower-case hex sha256 of `text`, as the host hashes a file (`contentHash`).
#[must_use]
pub fn sha256_hex(text: &str) -> String {
    Sha256::digest(text.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

/// Apply one event at `now` (milliseconds since the epoch).
pub fn step(docs: &mut Docs, event: Event, now: f64) -> Vec<Effect> {
    match event {
        Event::Open { path, draft } => {
            if docs.docs.contains_key(&path) {
                return Vec::new();
            }
            docs.docs.insert(path.clone(), Doc::new(&path, draft));
            vec![Effect::Read { path }]
        }
        Event::Loaded { path, result } => {
            let Some(doc) = docs.docs.get_mut(&path) else {
                return Vec::new();
            };
            match result {
                Ok(read) => load(doc, read),
                Err((code, message)) => {
                    doc.load = Load::Unavailable(Unavailable::from_refusal(&code, &message));
                    vec![log(Level::Info, format!("editor opened {} as unsupported ({code})", name(&path)))]
                }
            }
        }
        Event::Save { path } => docs.docs.get_mut(&path).map(|doc| save(doc, false)).unwrap_or_default(),
        Event::Overwrite { path } => docs
            .docs
            .get_mut(&path)
            .map(|doc| {
                doc.external_conflict = false;
                save(doc, true)
            })
            .unwrap_or_default(),
        Event::Written { path, content, result } => {
            let Some(doc) = docs.docs.get_mut(&path) else {
                return Vec::new();
            };
            match result {
                Ok(()) => written(doc, &content, now),
                // The host may or may not have applied it (`UNCERTAIN_WRITE_CODES`): a checked
                // write reads the file back; an unchecked one asks the user again.
                Err((code, _)) if UNCERTAIN.contains(&code.as_str()) && doc.hash.is_some() => {
                    vec![Effect::ReadBack { path, content }]
                }
                Err((code, message)) => {
                    let unchecked_unknown = UNCERTAIN.contains(&code.as_str());
                    let mut fx = failed(doc, &format!("{code}: {message}"));
                    if code == "PRECONDITION_FAILED" || unchecked_unknown {
                        doc.external_conflict = true;
                    }
                    if code == "PRECONDITION_FAILED" {
                        fx.push(Effect::ExternalChange { path });
                    }
                    fx
                }
            }
        }
        Event::ReadBack { path, content, disk } => {
            let Some(doc) = docs.docs.get_mut(&path) else {
                return Vec::new();
            };
            if disk.as_deref() == Some(content.as_str()) {
                written(doc, &content, now)
            } else {
                failed(doc, "the write's reply was lost and the file does not hold it")
            }
        }
        Event::ResolveDraft { path, keep_draft } => {
            let Some(doc) = docs.docs.get_mut(&path) else {
                return Vec::new();
            };
            let Some(draft) = doc.draft_conflict.take() else {
                return Vec::new();
            };
            if keep_draft {
                doc.editor.reset(&draft.content, now / 1000.0);
            }
            Vec::new()
        }
        Event::Reload { path } => {
            let Some(doc) = docs.docs.get_mut(&path) else {
                return Vec::new();
            };
            doc.external_conflict = false;
            doc.load = Load::Loading;
            vec![Effect::Read { path }]
        }
        Event::Close { path } => {
            docs.docs.remove(&path);
            Vec::new()
        }
        Event::Rename { from, to } => {
            if let Some(mut doc) = docs.docs.remove(&from) {
                doc.path.clone_from(&to);
                docs.docs.insert(to, doc);
            }
            Vec::new()
        }
    }
}

/// The read landed: the buffer takes the disk text, then a kept draft is reconciled with it.
fn load(doc: &mut Doc, read: Read) -> Vec<Effect> {
    let reloading = doc.load == Load::Loading && doc.editor.rope().len_chars() > 0;
    let caret = doc.editor.selection();
    doc.bom = read.content.starts_with(BOM);
    doc.editor = Editor::new(read.content.strip_prefix(BOM).unwrap_or(&read.content));
    doc.loads += 1;
    doc.crlf = only_crlf(&read.content);
    if reloading {
        doc.editor.apply(troupe_editor::Command::Select(caret), 0.0);
    }
    doc.saved = doc.editor.rope().clone();
    doc.hash = read.content_hash;
    doc.modified_at = read.modified_at;
    doc.load = Load::Ready;
    let mut fx = vec![log(Level::Info, format!("editor opened {} {} bytes", name(&doc.path), read.content.len()))];
    if let Some(draft) = doc.pending_draft.take()
        && draft.content != doc.editor.text()
    {
        if read.modified_at <= draft.saved_at {
            doc.editor = Editor::new(&draft.content);
        } else {
            doc.draft_conflict = Some(draft);
            fx.push(log(Level::Info, format!("editor draft conflict {}", name(&doc.path))));
        }
    }
    // A save asked for while the file loaded (the unsaved-changes dialog on a tab that had
    // not mounted) runs on the reconciled buffer.
    if std::mem::take(&mut doc.queued) {
        fx.extend(save(doc, false));
    }
    fx
}

const BOM: char = '\u{feff}';

/// Every line break in `text` is `\r\n`, and there is at least one.
fn only_crlf(text: &str) -> bool {
    let crlf = text.matches("\r\n").count();
    crlf > 0 && crlf == text.matches('\n').count() && crlf == text.matches('\r').count()
}

/// Write failures that say nothing about whether the host applied the write.
const UNCERTAIN: [&str; 3] = ["NETWORK_ERROR", "OUTCOME_UNKNOWN", "COMMAND_IN_PROGRESS"];

/// The host holds `content`: it is the new disk text, and a queued save runs now.
fn written(doc: &mut Doc, content: &str, now: f64) -> Vec<Effect> {
    doc.saving = None;
    doc.hash = Some(sha256_hex(content));
    // Only the byte order mark this file was read with is set aside; a U+FEFF typed at the
    // start of a file without one is text.
    let text = if doc.bom { content.strip_prefix(BOM).unwrap_or(content) } else { content };
    doc.saved = Editor::new(text).rope().clone();
    doc.modified_at = now;
    doc.external_conflict = false;
    let mut fx = vec![log(Level::Info, format!("editor save succeeded {} {} bytes", name(&doc.path), content.len()))];
    if std::mem::take(&mut doc.queued) {
        fx.extend(save(doc, false));
    } else {
        fx.push(Effect::Saved { path: doc.path.clone(), clean: !doc.is_dirty() });
    }
    fx
}

fn failed(doc: &mut Doc, message: &str) -> Vec<Effect> {
    doc.saving = None;
    doc.queued = false;
    vec![
        log(Level::Error, format!("editor save failed {}: {message}", name(&doc.path))),
        Effect::SaveFailed { path: doc.path.clone(), message: message.to_owned() },
    ]
}

fn save(doc: &mut Doc, overwrite: bool) -> Vec<Effect> {
    let path = doc.path.clone();
    if doc.draft_conflict.is_some() {
        return vec![log(Level::Info, format!("save skipped for {}: unresolved draft conflict", name(&path)))];
    }
    if doc.saving.is_some() || doc.load == Load::Loading {
        doc.queued = true;
        return Vec::new();
    }
    if doc.load != Load::Ready {
        return Vec::new();
    }
    if !doc.is_dirty() {
        return vec![Effect::Saved { path, clean: true }];
    }
    let content = doc.disk_text();
    doc.saving = Some(content.clone());
    let expected = if overwrite { None } else { doc.hash.clone() };
    vec![Effect::Write { path, content, expected }]
}

#[cfg(test)]
mod tests;
