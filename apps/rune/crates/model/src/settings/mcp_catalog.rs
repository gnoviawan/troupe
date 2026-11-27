//! The MCP catalog on the MCP servers page: its rows and the words they show
//! (`lib/mcp-catalog-copy.ts`), and the search the browser runs (`hooks/use-mcp-catalog-search.ts`):
//! a debounced query, pages of 50 up to 500 rows, and a newer search that voids an older answer.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// `MCP_CATALOG_PAGE_SIZE`.
pub const PAGE_SIZE: usize = 50;
/// `MCP_CATALOG_MAX_ROWS`: more rows than this ask for a narrower search instead.
pub const MAX_ROWS: usize = 500;
/// `MCP_CATALOG_SEARCH_DEBOUNCE_MS`, in seconds.
pub const DEBOUNCE: f64 = 0.25;
/// `MCP_CATALOG_MAX_QUERY_CHARS` and `MCP_CATALOG_MAX_QUERY_TERMS`.
pub const MAX_QUERY_CHARS: usize = 200;
pub const MAX_QUERY_TERMS: usize = 8;
/// `MCP_REGISTRY_URL`.
pub const REGISTRY_URL: &str = "https://registry.modelcontextprotocol.io";

/// One catalog entry (`McpCatalogRow`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub name: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub status: String,
    pub state: String,
    #[serde(default)]
    pub reason: Option<Value>,
    #[serde(default)]
    pub default_option: Option<Value>,
    #[serde(default)]
    pub repository_url: Option<String>,
    #[serde(default)]
    pub has_icon: bool,
}

impl Row {
    /// The card's title: the registry title, else the name after its last `/`.
    #[must_use]
    pub fn display_name(&self) -> &str {
        self.title.as_deref().unwrap_or_else(|| self.name.rsplit('/').next().unwrap_or(&self.name))
    }
}

/// One page of `/mcp-catalog/search` (`McpCatalogPage`).
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub items: Vec<Row>,
    pub next_cursor: Option<String>,
    pub total: u64,
    pub updated_at: Option<String>,
    #[serde(default)]
    pub sample: bool,
}

/// What a search shows (`McpCatalogResults`).
#[derive(Clone, Debug, PartialEq)]
pub struct Results {
    pub query: String,
    pub rows: Vec<Row>,
    pub total: u64,
    pub next_cursor: Option<String>,
    pub updated_at: Option<String>,
    pub sample: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum More {
    Idle,
    Loading,
    Failed,
}

/// `McpCatalogSearchState`.
#[derive(Clone, Debug, PartialEq)]
pub enum Search {
    Loading { previous: Option<Results> },
    Failed,
    TooManyTerms,
    Ready { results: Results, more: More },
}

impl Default for Search {
    fn default() -> Self {
        Search::Loading { previous: None }
    }
}

impl Search {
    /// The rows on screen: the ready results, or the ones a running search replaces.
    #[must_use]
    pub fn shown(&self) -> Option<&Results> {
        match self {
            Search::Ready { results, .. } => Some(results),
            Search::Loading { previous } => previous.as_ref(),
            _ => None,
        }
    }

    /// A new search starts; the old rows stay on screen until it answers.
    pub fn search(&mut self) {
        let previous = match std::mem::take(self) {
            Search::Ready { results, .. } => Some(results),
            Search::Loading { previous } => previous,
            _ => None,
        };
        *self = Search::Loading { previous };
    }

    pub fn set_more(&mut self, next: More) {
        if let Search::Ready { more, .. } = self {
            *more = next;
        }
    }

    /// A page answered: it replaces the rows, or `append`s to the ready ones.
    pub fn page(&mut self, page: Page, query: &str, append: bool) {
        let mut rows = match (append, std::mem::take(self)) {
            (true, Search::Ready { results, .. }) => results.rows,
            _ => Vec::new(),
        };
        rows.extend(page.items);
        let results = Results { query: query.to_owned(), rows, total: page.total, next_cursor: page.next_cursor, updated_at: page.updated_at, sample: page.sample };
        *self = Search::Ready { results, more: More::Idle };
    }

    /// The next page's cursor while the list may still grow.
    #[must_use]
    pub fn more_cursor(&self) -> Option<&str> {
        match self {
            Search::Ready { results, .. } if results.rows.len() < MAX_ROWS => results.next_cursor.as_deref(),
            _ => None,
        }
    }
}

/// More words than the host searches with.
#[must_use]
pub fn too_many_terms(query: &str) -> bool {
    query.split_whitespace().count() > MAX_QUERY_TERMS
}

/// `en-US` digit grouping.
#[must_use]
pub fn count(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// The group's meta (`summary`): servers, or matches for a query.
#[must_use]
pub fn summary(results: &Results) -> String {
    let noun = match (results.query.is_empty(), results.total == 1) {
        (true, true) => "server",
        (true, false) => "servers",
        (false, true) => "match",
        (false, false) => "matches",
    };
    format!("{} {noun}", count(results.total))
}

/// `MCP_CATALOG_STATE_LABEL`.
#[must_use]
pub fn state_label(state: &str) -> &'static str {
    match state {
        "installable" => "Installable",
        "needs-runtime" => "Needs a runtime",
        "unsupported" => "Unsupported",
        _ => "Source only",
    }
}

/// `STATE_TONE`: success, warning or muted.
#[must_use]
pub fn state_tone(state: &str) -> super::mcp::Tone {
    match state {
        "installable" => super::mcp::Tone::Success,
        "needs-runtime" => super::mcp::Tone::Warning,
        _ => super::mcp::Tone::Muted,
    }
}

fn runtime_name(runtime: &str) -> &str {
    match runtime {
        "npx" => "Node.js (npx)",
        "uvx" => "uv (uvx)",
        "docker" => "Docker",
        "dotnet" => ".NET SDK 10 or newer",
        other => other,
    }
}

/// `mcpCatalogReasonText`.
#[must_use]
pub fn reason_text(reason: &Value) -> String {
    let field = |k: &str| reason.get(k).and_then(Value::as_str).unwrap_or_default();
    match field("code") {
        "needs-runtime" => format!("Install {} on this computer to run it.", runtime_name(field("runtime"))),
        "no-install-options" => "The registry lists only its source code, with no package or hosted URL to install.".into(),
        "unparseable" => "Troupe couldn't read this server's registry entry.".into(),
        "insecure-url" => "Its hosted URL doesn't use https.".into(),
        "local-network-url" => "Its hosted URL points to a local or private network address.".into(),
        "invalid-url" => "Its hosted URL isn't a valid web address.".into(),
        "unknown-remote-transport" => format!("Its hosted URL uses the \"{}\" transport, which Troupe doesn't support.", field("transport")),
        "secret-in-url" => "Its hosted URL must carry a secret, and Troupe never stores secrets in URLs.".into(),
        "secret-in-arguments" => "Its launch arguments must carry a secret, and Troupe never stores secrets in arguments.".into(),
        "runtime-arguments" => "It needs extra options passed to its runtime, which Troupe does not allow.".into(),
        "runtime-hint-mismatch" => format!("It asks to run with \"{}\", which doesn't match its package type.", field("runtimeHint")),
        "denied-env" => format!("It sets {}, an environment variable Troupe blocks because it can change what runs.", field("name")),
        "invalid-env-name" => format!("It uses \"{}\", which isn't a valid environment variable name.", field("name")),
        "unsupported-registry-type" => format!("Its package comes from {}, which Troupe can't install from.", field("registryType")),
        "non-stdio-transport" => format!("Its package talks over \"{}\". Troupe runs packages over stdio only.", field("transport")),
        "unpinned-version" => "Its package has no fixed version, so Troupe can't pin what it runs.".into(),
        "invalid-package" => "Its package name or version isn't valid.".into(),
        "unsupported-registry" => format!("Its package is hosted on {}, which Troupe doesn't download from.", field("registry")),
        "invalid-command" => "Its command starts with \"-\", so it would be read as an option.".into(),
        "invalid-header-name" => format!("It sends \"{}\", which isn't a valid HTTP header name.", field("name")),
        "control-character" => format!("Its {} holds a control character.", field("field")),
        "duplicate-name" => format!("It lists \"{}\" more than once.", field("name")),
        "unsupported-launch-argument" => {
            format!("It passes {} to {}, which Troupe doesn't allow because it can change what runs.", field("argument"), field("command"))
        }
        "unsupported-runner" => format!("It runs its package through {}, which Troupe can't check. Use npx, uvx, docker or dotnet dnx instead.", field("command")),
        "cmd-metacharacter" => format!("Its {} holds a character Windows reads as a command operator (& | < > ^ % ! \" ( or )).", field("field")),
        _ => String::new(),
    }
}

/// `mcpCatalogUpdatedText`, with the date already formatted the way the client shows dates.
#[must_use]
pub fn updated_text(updated_at: Option<&str>, formatted: &str) -> String {
    match updated_at {
        None => "No catalog is loaded yet.".into(),
        Some(_) => format!("Updated {formatted}."),
    }
}

/// `mcpCatalogSampleText`.
#[must_use]
pub fn sample_text(status: &Value) -> Option<String> {
    if status.get("sample") != Some(&Value::Bool(true)) {
        return None;
    }
    let next = if sync_state(status) == "disabled" { "Turn on sync to load the full catalog." } else { "Sync replaces it with the full catalog." };
    Some(format!("This is the small sample bundled with this build, not the full registry. {next}"))
}

/// The status's `sync.state`.
#[must_use]
pub fn sync_state(status: &Value) -> &str {
    status.pointer("/sync/state").and_then(Value::as_str).unwrap_or("disabled")
}

/// `mcpCatalogSyncText`; `retry_at` is the failed sync's retry time as the client shows it.
#[must_use]
pub fn sync_text(status: &Value, retry_at: &str) -> Option<String> {
    let sync = status.get("sync")?;
    match sync_state(status) {
        "disabled" => Some("Sync is off. No catalog request leaves this host.".into()),
        "idle" => (status.get("sample") != Some(&Value::Bool(true))).then(|| "Up to date.".into()),
        "syncing" => Some(if sync.get("full") == Some(&Value::Bool(true)) {
            format!("Downloading the full registry… {} servers so far.", count(sync.get("fetched").and_then(Value::as_u64).unwrap_or(0)))
        } else {
            "Checking for updates…".into()
        }),
        "failed" => {
            let error = sync.get("error").and_then(Value::as_str).unwrap_or_default();
            Some(format!("Sync failed: {}. Retrying at {retry_at}.", error.strip_suffix('.').unwrap_or(error)))
        }
        _ => None,
    }
}

/// `mcpCatalogSyncErrorText`.
#[must_use]
pub fn sync_error_text(code: &str) -> &'static str {
    if code == "FORBIDDEN" { "Only the host can change catalog sync." } else { "Couldn't change catalog sync. Try again." }
}

/// Why Install is off for every row (`McpInstallBlock`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Block {
    ViewOnly,
    NoProject,
    Loading,
}

impl Block {
    /// `MCP_INSTALL_BLOCK_TEXT`.
    #[must_use]
    pub fn text(self) -> &'static str {
        match self {
            Block::ViewOnly => "This connection can't make changes on the host, so it can't install servers.",
            Block::NoProject => "Open a project to install servers. This server saves MCP servers per project.",
            Block::Loading => "Install is available once your MCP servers have loaded.",
        }
    }
}

#[cfg(test)]
#[path = "mcp_catalog_tests.rs"]
mod tests;
