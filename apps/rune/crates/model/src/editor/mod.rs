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
    /// The read failed; the tab shows why.
    Failed(String),
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
    /// A kept draft that differs from a newer file on disk; the buffer is read-only until the
    /// user keeps one.
    pub draft_conflict: Option<Draft>,
    /// The host refused the last save because the file changed on disk.
    pub external_conflict: bool,
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
            draft_conflict: None,
            external_conflict: false,
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
    Loaded { path: String, result: Result<Read, String> },
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
    /// The file is on disk: close the tab that asked (`SaveAndClose`).
    Saved { path: String },
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
                Err(message) => {
                    doc.load = Load::Failed(message.clone());
                    vec![log(Level::Warn, format!("editor open failed {}: {message}", name(&path)))]
                }
            }
        }
        Event::Save { path } => save(docs, &path, false),
        Event::Overwrite { path } => {
            if let Some(doc) = docs.docs.get_mut(&path) {
                doc.external_conflict = false;
            }
            save(docs, &path, true)
        }
        Event::Written { path, content, result } => {
            let Some(doc) = docs.docs.get_mut(&path) else {
                return Vec::new();
            };
            doc.saving = None;
            match result {
                Ok(()) => {
                    doc.hash = Some(sha256_hex(&content));
                    doc.saved = Rope::from_str(&content);
                    doc.modified_at = now;
                    doc.external_conflict = false;
                    vec![
                        log(Level::Info, format!("editor save succeeded {} {} bytes", name(&path), content.len())),
                        Effect::Saved { path },
                    ]
                }
                Err((code, message)) => {
                    let mut fx = vec![log(Level::Error, format!("editor save failed {}: {code} {message}", name(&path)))];
                    if code == "PRECONDITION_FAILED" {
                        doc.external_conflict = true;
                        fx.push(Effect::ExternalChange { path });
                    }
                    fx
                }
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
    doc.editor = Editor::new(&read.content);
    if reloading {
        doc.editor.apply(troupe_editor::Command::Select(caret), 0.0);
    }
    doc.saved = doc.editor.rope().clone();
    doc.hash = read.content_hash;
    doc.modified_at = read.modified_at;
    doc.load = Load::Ready;
    let mut fx = vec![log(Level::Info, format!("editor opened {} {} bytes", name(&doc.path), read.content.len()))];
    if let Some(draft) = doc.pending_draft.take() {
        let disk = doc.editor.text();
        if draft.content == disk {
            return fx;
        }
        if read.modified_at <= draft.saved_at {
            doc.editor = Editor::new(&draft.content);
        } else {
            doc.draft_conflict = Some(draft);
            fx.push(log(Level::Info, format!("editor draft conflict {}", name(&doc.path))));
        }
    }
    fx
}

fn save(docs: &mut Docs, path: &str, overwrite: bool) -> Vec<Effect> {
    let Some(doc) = docs.docs.get_mut(path) else {
        return Vec::new();
    };
    if doc.draft_conflict.is_some() {
        return vec![log(Level::Info, format!("save skipped for {}: unresolved draft conflict", name(path)))];
    }
    if doc.saving.is_some() || doc.load != Load::Ready {
        return Vec::new();
    }
    if !doc.is_dirty() {
        return vec![Effect::Saved { path: path.to_owned() }];
    }
    let content = doc.editor.text();
    doc.saving = Some(content.clone());
    let expected = if overwrite { None } else { doc.hash.clone() };
    vec![Effect::Write { path: path.to_owned(), content, expected }]
}

#[cfg(test)]
mod tests;
