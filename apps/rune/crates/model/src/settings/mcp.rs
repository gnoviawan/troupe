//! The MCP servers page's pure logic: the registry's read rules (`lib/acp-mcp-persistence.ts`),
//! the Add and Edit form's draft (`lib/mcp-server-draft.ts`), and how a server shows on its
//! card and detail page (`components/settings/mcp-server-view.ts`).
//!
//! A server stays a JSON object, as the acp model keeps it, so fields this client does not know
//! survive a round trip.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

pub type Object = Map<String, Value>;

/// The host store key (`PersistenceKeys.acpMcpServers`), shared by every device.
pub const KEY: &str = "shared/lww/acp/mcp-servers";
/// Registry ids a removed canvas feature wrote; a read drops them.
const LEGACY_CANVAS_PREFIX: &str = "canvas-mcp-";
/// The header the Access token choice fills (`AUTH_HEADER`).
pub const AUTH_HEADER: &str = "Authorization";

/// JavaScript's `\s`.
fn js_space(c: char) -> bool {
    c == '\u{feff}' || (c.is_whitespace() && c != '\u{85}')
}

pub(crate) fn trim(text: &str) -> &str {
    text.trim_matches(js_space)
}

fn str_of<'a>(server: &'a Object, key: &str) -> Option<&'a str> {
    server.get(key).and_then(Value::as_str)
}

/// `transportOf`: `type`, else stdio.
#[must_use]
pub fn transport_of(server: &Object) -> &str {
    match server.get("type") {
        Some(Value::String(t)) => t,
        _ => "stdio",
    }
}

#[must_use]
pub fn is_remote(server: &Object) -> bool {
    matches!(transport_of(server), "http" | "sse")
}

/// `mcpServerTarget`: the command, or the URL.
#[must_use]
pub fn target(server: &Object) -> &str {
    let key = if transport_of(server) == "stdio" { "command" } else { "url" };
    str_of(server, key).unwrap_or_default()
}

/// `validateMcpServer`'s issues, in its order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Issue {
    NameRequired,
    CommandRequired,
    UrlRequired,
    UrlInvalid,
}

#[must_use]
pub fn validate(server: &Object) -> Vec<Issue> {
    let mut issues = Vec::new();
    if str_of(server, "name").is_none_or(|n| trim(n).is_empty()) {
        issues.push(Issue::NameRequired);
    }
    if transport_of(server) == "stdio" {
        if str_of(server, "command").is_none_or(|c| trim(c).is_empty()) {
            issues.push(Issue::CommandRequired);
        }
    } else {
        match str_of(server, "url") {
            Some(url) if !trim(url).is_empty() => {
                if !url_parses(url) {
                    issues.push(Issue::UrlInvalid);
                }
            }
            _ => issues.push(Issue::UrlRequired),
        }
    }
    issues
}

const SPECIAL: [&str; 5] = ["http", "https", "ws", "wss", "ftp"];

/// What `new URL(text)` keeps of an absolute URL that these pages read: its path.
struct Parsed {
    path: String,
}

/// `new URL(text)` without a base, as far as these pages need it: a scheme is required, and a
/// special scheme needs a host free of forbidden code points and a numeric port.
fn parse_url(text: &str) -> Option<Parsed> {
    let text: String = text.trim_matches(|c: char| c <= ' ').chars().filter(|c| !matches!(c, '\t' | '\n' | '\r')).collect();
    let colon = text.find(':')?;
    let scheme = &text[..colon];
    let mut chars = scheme.chars();
    if !chars.next().is_some_and(|c| c.is_ascii_alphabetic()) || !chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')) {
        return None;
    }
    let scheme = scheme.to_ascii_lowercase();
    let rest = &text[colon + 1..];
    let cut = |s: &str| s.find(['?', '#']).unwrap_or(s.len());
    if scheme == "file" {
        let path = rest.trim_start_matches(['/', '\\']);
        let path = &path[..cut(path)];
        let path = path.find(['/', '\\']).map_or("/".to_owned(), |i| path[i..].replace('\\', "/"));
        return Some(Parsed { path });
    }
    if !SPECIAL.contains(&scheme.as_str()) {
        let path = &rest[..cut(rest)];
        let path = match path.strip_prefix("//") {
            Some(after) => after.find('/').map_or(String::new(), |i| after[i..].to_owned()),
            None => path.to_owned(),
        };
        return Some(Parsed { path });
    }
    let after = rest.trim_start_matches(['/', '\\']);
    let end = after.find(['/', '\\', '?', '#']).unwrap_or(after.len());
    let authority = &after[..end];
    let host_port = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let (host, port) = if host_port.starts_with('[') {
        let close = host_port.find(']')?;
        let tail = &host_port[close + 1..];
        (&host_port[..=close], if tail.is_empty() { "" } else { tail.strip_prefix(':')? })
    } else {
        host_port.rsplit_once(':').unwrap_or((host_port, ""))
    };
    let forbidden = |c: char| c.is_control() || " #%/:<>?@[\\]^|".contains(c);
    if host.is_empty() || (!host.starts_with('[') && host.chars().any(forbidden)) {
        return None;
    }
    if !port.chars().all(|c| c.is_ascii_digit()) || port.parse::<u64>().is_ok_and(|p| p > 65535) {
        return None;
    }
    let path = &after[end..];
    let path = &path[..cut(path)];
    let path = if path.is_empty() { "/".to_owned() } else { path.replace('\\', "/") };
    Some(Parsed { path })
}

fn url_parses(text: &str) -> bool {
    parse_url(text).is_some()
}

fn is_record(value: &Value) -> bool {
    value.is_object()
}

/// `stringPairs`: every entry a `{ name, value }` of strings, kept as it is; else None.
fn string_pairs(value: &Value) -> Option<Vec<Value>> {
    let entries = value.as_array()?;
    let ok = |e: &Value| e.get("name").is_some_and(Value::is_string) && e.get("value").is_some_and(Value::is_string);
    entries.iter().all(ok).then(|| entries.clone())
}

/// `catalogProvenance`.
fn catalog_provenance(value: Option<&Value>) -> Option<Value> {
    let v = value?.as_object()?;
    let string = |k: &str| v.get(k).filter(|x| x.is_string());
    let kind = string("kind").filter(|k| *k == "remote" || *k == "package")?;
    let repository = v.get("repositoryId").filter(|r| r.is_null() || r.is_string())?;
    if v.get("source") != Some(&json!("mcp-registry")) || !v.get("index").is_some_and(Value::is_number) {
        return None;
    }
    Some(json!({
        "source": "mcp-registry",
        "name": string("name")?,
        "version": string("version")?,
        "kind": kind,
        "index": v["index"],
        "repositoryId": repository,
        "launchDigest": string("launchDigest")?,
    }))
}

/// `normalizeStoredServer`: a valid entry with `enabled` filled in, else None.
fn normalize_server(value: &Value) -> Option<Object> {
    let v = value.as_object()?;
    let (id, name) = (v.get("id").filter(|x| x.is_string())?, v.get("name").filter(|x| x.is_string())?);
    let enabled = match v.get("enabled") {
        None => json!(true),
        Some(Value::Bool(b)) => json!(b),
        Some(_) => return None,
    };
    let kind = match v.get("type") {
        None | Some(Value::Null) => "stdio",
        Some(Value::String(t)) => t.as_str(),
        Some(_) => return None,
    };
    let mut server = Object::new();
    server.insert("id".into(), id.clone());
    server.insert("type".into(), json!(kind));
    server.insert("name".into(), name.clone());
    let pairs_key = if kind == "stdio" {
        server.insert("command".into(), v.get("command").filter(|x| x.is_string())?.clone());
        if let Some(args) = v.get("args") {
            let list = args.as_array()?;
            if !list.iter().all(Value::is_string) {
                return None;
            }
            server.insert("args".into(), args.clone());
        }
        "env"
    } else if kind == "http" || kind == "sse" {
        server.insert("url".into(), v.get("url").filter(|x| x.is_string())?.clone());
        "headers"
    } else {
        return None;
    };
    if let Some(pairs) = v.get(pairs_key) {
        server.insert(pairs_key.into(), Value::Array(string_pairs(pairs)?));
    }
    server.insert("enabled".into(), enabled);
    if let Some(catalog) = catalog_provenance(v.get("catalog")) {
        server.insert("catalog".into(), catalog);
    }
    validate(&server).is_empty().then_some(server)
}

/// `normalizeMcpRegistry`: the valid entries; anything that is not a list is empty.
#[must_use]
pub fn normalize_registry(value: &Value) -> Vec<Object> {
    value.as_array().into_iter().flatten().filter_map(normalize_server).collect()
}

/// `dropLegacyCanvasMcpEntries`: the registry without the old canvas entries, and whether any
/// were dropped (the caller then saves the list without them).
#[must_use]
pub fn read_registry(raw: &Value) -> (Vec<Object>, bool) {
    let Some(entries) = raw.as_array() else {
        return (normalize_registry(raw), false);
    };
    let legacy = |e: &Value| is_record(e) && e.get("id").and_then(Value::as_str).is_some_and(|id| id.starts_with(LEGACY_CANVAS_PREFIX));
    let kept: Vec<Value> = entries.iter().filter(|e| !legacy(e)).cloned().collect();
    let dropped = kept.len() != entries.len();
    (normalize_registry(&Value::Array(kept)), dropped)
}

/// `launchShape`: what starts the server, without secret values.
fn launch_shape(server: &Object) -> Value {
    let names = |key: &str| -> Vec<Value> {
        server.get(key).and_then(Value::as_array).into_iter().flatten().map(|p| p.get("name").cloned().unwrap_or(Value::Null)).collect()
    };
    match transport_of(server) {
        t @ ("http" | "sse") => json!([t, server.get("url"), names("headers")]),
        _ => json!(["stdio", server.get("command"), server.get("args").cloned().unwrap_or(json!([])), names("env")]),
    }
}

/// `sameLaunch`: an edit that keeps how the server starts keeps its catalog provenance.
#[must_use]
pub fn same_launch(a: &Object, b: &Object) -> bool {
    launch_shape(a) == launch_shape(b)
}

/// A server's status word (`mcpServerStatus`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Tone {
    Muted,
    Success,
    Danger,
    Warning,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Status {
    pub tone: Tone,
    pub label: &'static str,
    pub pending: bool,
}

#[must_use]
pub fn status(enabled: bool, probing: bool, probe_status: Option<&str>) -> Status {
    let (tone, label, pending) = match (enabled, probing, probe_status) {
        (false, ..) => (Tone::Muted, "Off", false),
        (_, true, _) => (Tone::Muted, "Checking…", true),
        (_, _, Some("connected")) => (Tone::Success, "Connected", false),
        (_, _, Some("disconnected")) => (Tone::Danger, "Unreachable", false),
        (_, _, Some("authRequired")) => (Tone::Warning, "Needs sign-in", false),
        _ => (Tone::Muted, "Not checked", false),
    };
    Status { tone, label, pending }
}

/// `mcpToolCount`.
#[must_use]
pub fn tool_count(count: usize) -> String {
    if count == 1 { "1 tool".into() } else { format!("{count} tools") }
}

/// One name and value line of the form (`KeyValueRow` without its React key).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Row {
    pub name: String,
    pub value: String,
}

impl Row {
    #[must_use]
    pub fn new(name: &str, value: &str) -> Self {
        Self { name: name.into(), value: value.into() }
    }

    fn blank(&self) -> bool {
        trim(&self.name).is_empty() && trim(&self.value).is_empty()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    #[default]
    Remote,
    Local,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Transport {
    #[default]
    Http,
    Sse,
}

impl Transport {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Sse => "sse",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Auth {
    #[default]
    Browser,
    Token,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Remote {
    pub url: String,
    pub transport: Transport,
    /// The user picked the transport, so a URL edit no longer guesses it.
    pub transport_pinned: bool,
    pub auth: Auth,
    pub headers: Vec<Row>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Local {
    pub command_line: String,
    pub env: Vec<Row>,
}

/// What the form edits (`McpServerDraft`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Draft {
    pub kind: Kind,
    pub name: String,
    pub remote: Remote,
    pub local: Local,
}

impl Default for Draft {
    /// `emptyDraft`.
    fn default() -> Self {
        Self {
            kind: Kind::Remote,
            name: String::new(),
            remote: Remote { url: String::new(), transport: Transport::Http, transport_pinned: false, auth: Auth::Browser, headers: vec![Row::default()] },
            local: Local { command_line: String::new(), env: vec![Row::default()] },
        }
    }
}

/// A form field an error names (`DraftField`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Field {
    Name,
    Url,
    CommandLine,
    Headers,
    Env,
}

/// `DraftErrors`, in the order they were found.
pub type Errors = Vec<(Field, String)>;

fn put(errors: &mut Errors, field: Field, message: &str) {
    if !errors.iter().any(|(f, _)| *f == field) {
        errors.push((field, message.to_owned()));
    }
}

fn issue_field(issue: Issue) -> (Field, &'static str) {
    match issue {
        Issue::NameRequired => (Field::Name, "Give the server a name."),
        Issue::CommandRequired => (Field::CommandLine, "Enter the command that starts the server."),
        Issue::UrlRequired => (Field::Url, "Enter the server URL."),
        Issue::UrlInvalid => (Field::Url, "Enter a full URL, like https://mcp.example.com/mcp."),
    }
}

/// `transportForUrl`: a path that ends in `/sse` means SSE.
#[must_use]
pub fn transport_for_url(url: &str) -> Transport {
    let path = match parse_url(trim(url)) {
        Some(parsed) => parsed.path,
        None => {
            let t = trim(url);
            t[..t.find(['?', '#']).unwrap_or(t.len())].to_owned()
        }
    };
    let lower = path.to_ascii_lowercase();
    if lower.ends_with("/sse") || lower.ends_with("/sse/") { Transport::Sse } else { Transport::Http }
}

struct Token {
    value: String,
    /// The byte offset where the token starts.
    start: usize,
}

/// `scanTokens`: shell-style words, quotes and backslash escapes; None for an open quote unless
/// `close_open_quote`.
fn scan_tokens(line: &str, close_open_quote: bool) -> Option<Vec<Token>> {
    let chars: Vec<(usize, char)> = line.char_indices().collect();
    let mut tokens = Vec::new();
    let mut value = String::new();
    let mut start: Option<usize> = None;
    let mut quote: Option<char> = None;
    let mut i = 0;
    while i < chars.len() {
        let (at, c) = chars[i];
        let next = chars.get(i + 1).map(|&(_, n)| n);
        match quote {
            Some('\'') => {
                if c == '\'' {
                    quote = None;
                } else {
                    value.push(c);
                }
            }
            Some(_) => {
                if c == '\\' && matches!(next, Some('"' | '\\')) {
                    value.push(next.unwrap_or_default());
                    i += 1;
                } else if c == '"' {
                    quote = None;
                } else {
                    value.push(c);
                }
            }
            None if c == '\\' && next.is_some_and(|n| js_space(n) || n == '\'' || n == '"') => {
                start.get_or_insert(at);
                value.push(next.unwrap_or_default());
                i += 1;
            }
            None if js_space(c) => {
                if let Some(s) = start.take() {
                    tokens.push(Token { value: std::mem::take(&mut value), start: s });
                }
                value.clear();
            }
            None => {
                start.get_or_insert(at);
                if c == '"' || c == '\'' {
                    quote = Some(c);
                } else {
                    value.push(c);
                }
            }
        }
        i += 1;
    }
    if quote.is_some() && !close_open_quote {
        return None;
    }
    if let Some(s) = start {
        tokens.push(Token { value, start: s });
    }
    Some(tokens)
}

/// `tokenizeCommandLine`.
#[must_use]
pub fn tokenize(line: &str) -> Option<Vec<String>> {
    scan_tokens(line, false).map(|t| t.into_iter().map(|t| t.value).collect())
}

fn quote_token(token: &str) -> String {
    let needs = |c: char| js_space(c) || c == '\'' || c == '"';
    if !token.is_empty() && !token.chars().any(needs) {
        token.to_owned()
    } else if !token.contains('\'') {
        format!("'{token}'")
    } else {
        format!("\"{}\"", token.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

/// `joinCommandLine`.
#[must_use]
pub fn join(tokens: &[String]) -> String {
    tokens.iter().map(|t| quote_token(t)).collect::<Vec<_>>().join(" ")
}

fn env_assignment(token: &str) -> bool {
    let Some((name, _)) = token.split_once('=') else { return false };
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// `liftEnvPrefix`: `KEY=value` words before the command move to the environment rows.
#[must_use]
pub fn lift_env_prefix(local: &Local) -> Local {
    let Some(tokens) = scan_tokens(&local.command_line, false) else { return local.clone() };
    let first_command = tokens.iter().position(|t| !env_assignment(&t.value));
    let lifted = &tokens[..first_command.unwrap_or(tokens.len())];
    if lifted.is_empty() {
        return local.clone();
    }
    let mut env: Vec<Row> = local.env.iter().filter(|r| !r.blank()).cloned().collect();
    for token in lifted {
        let (name, value) = token.value.split_once('=').unwrap_or_default();
        match env.iter_mut().find(|r| trim(&r.name) == name) {
            Some(row) => row.value = value.to_owned(),
            None => env.push(Row::new(name, value)),
        }
    }
    let rest = first_command.map_or(String::new(), |i| local.command_line[tokens[i].start..].to_owned());
    Local { command_line: rest, env }
}

/// `rowProblem`: a row with only a name or only a value.
#[must_use]
pub fn row_problem(row: &Row) -> Option<&'static str> {
    match (trim(&row.name).is_empty(), trim(&row.value).is_empty()) {
        (false, true) => Some("Needs a value."),
        (true, false) => Some("Needs a name."),
        _ => None,
    }
}

fn pairs(rows: &[Row], unchecked: bool) -> Vec<Value> {
    rows.iter()
        .filter(|r| !trim(&r.name).is_empty() && (unchecked || !trim(&r.value).is_empty()))
        .map(|r| json!({ "name": trim(&r.name), "value": r.value }))
        .collect()
}

fn first_row_problem(rows: &[Row]) -> Option<&'static str> {
    rows.iter().find_map(row_problem)
}

/// `buildConfig`.
fn build_config(draft: &Draft, unchecked: bool) -> (Object, Errors) {
    let mut config = Object::new();
    let mut errors = Errors::new();
    let name = trim(&draft.name);
    if draft.kind == Kind::Remote {
        let headers = pairs(&draft.remote.headers, unchecked);
        config.insert("type".into(), json!(draft.remote.transport.as_str()));
        config.insert("name".into(), json!(name));
        config.insert("url".into(), json!(trim(&draft.remote.url)));
        if !headers.is_empty() {
            config.insert("headers".into(), Value::Array(headers));
        }
        if let Some(problem) = first_row_problem(&draft.remote.headers) {
            put(&mut errors, Field::Headers, problem);
        }
        return (config, errors);
    }
    let local = lift_env_prefix(&draft.local);
    let tokens: Option<Vec<String>> = scan_tokens(&local.command_line, unchecked).map(|t| t.into_iter().map(|t| t.value).collect());
    let words = tokens.clone().unwrap_or_default();
    let env = pairs(&local.env, unchecked);
    config.insert("type".into(), json!("stdio"));
    config.insert("name".into(), json!(name));
    config.insert("command".into(), json!(words.first().cloned().unwrap_or_default()));
    if words.len() > 1 {
        config.insert("args".into(), json!(words[1..]));
    }
    if !env.is_empty() {
        config.insert("env".into(), Value::Array(env));
    }
    if let Some(problem) = first_row_problem(&local.env) {
        put(&mut errors, Field::Env, problem);
    }
    if tokens.is_none() {
        put(&mut errors, Field::CommandLine, "Close the quote in the command.");
    }
    (config, errors)
}

/// `draftToUncheckedConfig`: the draft as JSON for the JSON view, even when it is not valid.
#[must_use]
pub fn unchecked_config(draft: &Draft) -> Object {
    build_config(draft, true).0
}

/// `draftToConfig`: the server to save, or what each field must fix.
pub fn draft_to_config(draft: &Draft) -> Result<Object, Errors> {
    let (config, mut errors) = build_config(draft, false);
    for issue in validate(&config) {
        let (field, message) = issue_field(issue);
        put(&mut errors, field, message);
    }
    if errors.is_empty() { Ok(config) } else { Err(errors) }
}

fn rows_from(server: &Object, key: &str) -> Vec<Row> {
    let rows: Vec<Row> = server
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|p| Row::new(p["name"].as_str().unwrap_or_default(), p["value"].as_str().unwrap_or_default()))
        .collect();
    if rows.is_empty() { vec![Row::default()] } else { rows }
}

/// `draftFromServer`: the form for Edit.
#[must_use]
pub fn draft_from_server(server: &Object) -> Draft {
    let name = str_of(server, "name").unwrap_or_default().to_owned();
    let draft = Draft::default();
    if transport_of(server) == "stdio" {
        let mut words = vec![str_of(server, "command").unwrap_or_default().to_owned()];
        words.extend(server.get("args").and_then(Value::as_array).into_iter().flatten().filter_map(|a| a.as_str().map(str::to_owned)));
        return Draft { kind: Kind::Local, name, local: Local { command_line: join(&words), env: rows_from(server, "env") }, ..draft };
    }
    let url = str_of(server, "url").unwrap_or_default().to_owned();
    let transport = if transport_of(server) == "sse" { Transport::Sse } else { Transport::Http };
    let token = server
        .get("headers")
        .and_then(Value::as_array)
        .is_some_and(|h| h.iter().any(|p| p["name"].as_str().is_some_and(|n| n.to_lowercase() == "authorization")));
    Draft {
        kind: Kind::Remote,
        name,
        remote: Remote {
            transport_pinned: transport != transport_for_url(&url),
            url,
            transport,
            auth: if token { Auth::Token } else { Auth::Browser },
            headers: rows_from(server, "headers"),
        },
        ..draft
    }
}

/// `withUrl`: a new URL, and its guessed transport unless the user picked one.
#[must_use]
pub fn with_url(remote: &Remote, url: &str) -> Remote {
    let transport = if remote.transport_pinned { remote.transport } else { transport_for_url(url) };
    Remote { url: url.to_owned(), transport, ..remote.clone() }
}

/// `withAuth`: Access token adds an `Authorization` row at the top; Browser sign-in removes it.
#[must_use]
pub fn with_auth(remote: &Remote, auth: Auth) -> Remote {
    let is_auth = |r: &Row| trim(&r.name).to_lowercase() == "authorization";
    let filled: Vec<Row> = remote.headers.iter().filter(|r| !r.blank()).cloned().collect();
    let headers = match auth {
        Auth::Token if filled.iter().any(is_auth) => filled,
        Auth::Token => std::iter::once(Row::new(AUTH_HEADER, "")).chain(filled).collect(),
        Auth::Browser => {
            let rest: Vec<Row> = filled.into_iter().filter(|r| !is_auth(r)).collect();
            if rest.is_empty() { vec![Row::default()] } else { rest }
        }
    };
    Remote { auth, headers, ..remote.clone() }
}

#[cfg(test)]
#[path = "mcp_tests.rs"]
mod tests;
