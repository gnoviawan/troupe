//! The MCP catalog under the servers you added (`McpCatalogBrowser.tsx`, `McpCatalogCard.tsx`
//! and the search and status hooks): a debounced search of `/mcp-catalog/search` with pages of
//! 50, the catalog's source card with its sync switch, polled while it syncs, and two cards per
//! row (one on a phone). The rules and words are `model::settings::mcp_catalog`.

use client::http::{HttpError, HttpRequest, Method};
use model::settings::mcp_catalog::{self as rules, Block, More, Page, Row, Search};
use rune::{El, TextEdit, Tx, col, row, text};
use serde_json::{Value, json};
use theme::Tokens;
use ui::settings::{StatusTone, card, filter, group_with_meta, letter_tile, notice, status_word, switch_row, toned_notice};
use ui::type_::{mono, sans};
use ui::{Button, ButtonSize, ButtonVariant};

use crate::State;
use crate::host::Fetch;

/// `MCP_CATALOG_SYNCING_POLL_MS` and `MCP_CATALOG_IDLE_POLL_MS`, in seconds.
const SYNCING_POLL: f64 = 2.0;
const IDLE_POLL: f64 = 60.0;
/// `max-h-[min(40rem,70dvh)]` at its 40rem.
const GRID_MAX_HEIGHT: f32 = 640.0;

/// What a catalog read is for.
#[derive(Clone, Debug, PartialEq)]
pub enum CatalogFetch {
    /// A search page, for the search generation that sent it.
    Search { generation: u64, query: String, append: bool },
    /// `/mcp-catalog/status`, by request number.
    Status(u64),
    /// `/mcp-catalog/sync`, by request number.
    Sync(u64),
}

/// `McpCatalogStatusState`.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum CatalogStatus {
    #[default]
    Loading,
    Unavailable,
    /// The status, and the sync choice on the wire.
    Ready { catalog: Value, requested: Option<bool> },
}

#[derive(Default)]
pub struct CatalogUi {
    pub query: TextEdit,
    /// The query as last typed, and when it settles into a search.
    typed: String,
    due: Option<f64>,
    /// The query the search ran for.
    settled: String,
    pub search: Search,
    generation: u64,
    page_in_flight: bool,
    pub status: CatalogStatus,
    status_sent: u64,
    status_applied: u64,
    toggling: bool,
    next_status: Option<f64>,
    /// The catalog version a search was rerun for (`useSearchFollowsCatalog`).
    retried_for: Option<String>,
    /// The page searched since it opened.
    pub active: bool,
}

fn glyph(name: &str) -> &'static rune::Icon {
    theme::icons::named(name).expect("bundled icon")
}

fn tone(t: model::settings::mcp::Tone) -> StatusTone {
    match t {
        model::settings::mcp::Tone::Success => StatusTone::Success,
        model::settings::mcp::Tone::Warning => StatusTone::Warning,
        model::settings::mcp::Tone::Danger => StatusTone::Danger,
        model::settings::mcp::Tone::Muted => StatusTone::Muted,
    }
}

fn code_of(e: &HttpError) -> String {
    match e {
        HttpError::Refused { code, .. } => code.clone(),
        HttpError::Network(_) => "NETWORK_ERROR".into(),
        HttpError::InProgress => "IN_PROGRESS".into(),
    }
}

impl State {
    fn catalog_send(&mut self, request: HttpRequest, tag: CatalogFetch) {
        #[cfg(test)]
        self.mcp_ui.posted.push((request.path.clone(), request.body.clone().unwrap_or(Value::Null)));
        if let Some(host) = &mut self.host {
            host.request(request, Fetch::Catalog(tag));
        }
    }

    /// The MCP page opened: search the catalog and read where it comes from.
    pub(crate) fn catalog_opened(&mut self) {
        let c = &mut self.mcp_ui.catalog;
        c.active = true;
        c.typed = c.query.text.trim().to_owned();
        c.settled.clone_from(&c.typed);
        c.due = None;
        self.catalog_search();
        self.catalog_status();
    }

    /// The search for the settled query (`useMcpCatalogSearch`'s effect).
    fn catalog_search(&mut self) {
        let c = &mut self.mcp_ui.catalog;
        c.generation += 1;
        c.page_in_flight = false;
        if rules::too_many_terms(&c.settled) {
            c.search = Search::TooManyTerms;
            return;
        }
        c.search.search();
        let (generation, query) = (c.generation, c.settled.clone());
        self.catalog_send(HttpRequest::get(&search_path(&query, None)), CatalogFetch::Search { generation, query, append: false });
    }

    /// `loadMore`.
    pub(crate) fn catalog_more(&mut self) {
        let c = &mut self.mcp_ui.catalog;
        let Some(cursor) = c.search.more_cursor().map(str::to_owned) else { return };
        if c.page_in_flight {
            return;
        }
        c.page_in_flight = true;
        c.search.set_more(More::Loading);
        let (generation, query) = (c.generation, c.settled.clone());
        self.catalog_send(HttpRequest::get(&search_path(&query, Some(&cursor))), CatalogFetch::Search { generation, query, append: true });
    }

    /// The page opened again: read the status now, as React's visible effect does.
    pub(crate) fn catalog_refresh(&mut self) {
        self.catalog_status();
    }

    fn catalog_status(&mut self) {
        let c = &mut self.mcp_ui.catalog;
        if c.toggling {
            return;
        }
        c.status_sent += 1;
        let request = c.status_sent;
        let next = self.now + self.catalog_poll();
        self.mcp_ui.catalog.next_status = Some(next);
        self.catalog_send(HttpRequest::get("/mcp-catalog/status"), CatalogFetch::Status(request));
    }

    fn catalog_poll(&self) -> f64 {
        match &self.mcp_ui.catalog.status {
            CatalogStatus::Ready { catalog, .. } => {
                let state = rules::sync_state(catalog);
                if state == "syncing" || (state == "idle" && catalog["sample"] == json!(true)) { SYNCING_POLL } else { IDLE_POLL }
            }
            _ => IDLE_POLL,
        }
    }

    /// The switch: `setSync`, with the choice shown until the host answers.
    fn catalog_set_sync(&mut self, enabled: bool) {
        log::info!("[settings.McpCatalogBrowser] sync {}", if enabled { "on" } else { "off" });
        let c = &mut self.mcp_ui.catalog;
        c.toggling = true;
        if let CatalogStatus::Ready { requested, .. } = &mut c.status {
            *requested = Some(enabled);
        }
        c.status_sent += 1;
        let request = HttpRequest { method: Method::Post, path: "/mcp-catalog/sync".into(), body: Some(json!({ "enabled": enabled })), command_id: None };
        let id = c.status_sent;
        self.catalog_send(request, CatalogFetch::Sync(id));
    }

    /// The debounce and the status poll, while the page is open; returns the next wake.
    pub(crate) fn catalog_tick(&mut self, now: f64) -> Option<f64> {
        if !self.mcp_ui.catalog.active || !self.mcp_catalog_visible() {
            return None;
        }
        let c = &mut self.mcp_ui.catalog;
        let typed = c.query.text.trim().to_owned();
        if typed != c.typed {
            c.typed = typed;
            c.due = Some(now + rules::DEBOUNCE);
        }
        if c.due.is_some_and(|due| due <= now) {
            c.due = None;
            if c.typed != c.settled {
                c.settled.clone_from(&c.typed);
                self.catalog_search();
            }
        }
        if self.mcp_ui.catalog.next_status.is_some_and(|at| at <= now) {
            self.catalog_status();
        }
        let c = &self.mcp_ui.catalog;
        let status = if c.toggling { None } else { c.next_status };
        [c.due, status].into_iter().flatten().reduce(f64::min)
    }

    /// `maxLength`: what typing or a paste adds past `MAX_QUERY_CHARS` is dropped.
    pub(crate) fn catalog_query_changed(&mut self) {
        let query = &mut self.mcp_ui.catalog.query;
        let (_, caret) = query.range();
        if let Some(over) = overflow(&query.text, caret, rules::MAX_QUERY_CHARS) {
            query.set_selection(over.start, over.end);
            query.insert("");
        }
    }

    fn mcp_catalog_visible(&self) -> bool {
        self.overlays.is_open(crate::settings_view::APP) && self.settings_page().id == crate::settings_mcp::PAGE && self.mcp_ui.detail.is_none()
    }

    /// A catalog answer.
    pub(crate) fn catalog_fetched(&mut self, tag: CatalogFetch, result: Result<Value, HttpError>) {
        match tag {
            CatalogFetch::Search { generation, query, append } => {
                let c = &mut self.mcp_ui.catalog;
                if generation != c.generation {
                    return;
                }
                if append {
                    c.page_in_flight = false;
                }
                match result.map_err(|e| code_of(&e)).and_then(|v| serde_json::from_value::<Page>(v).map_err(|e| e.to_string())) {
                    Ok(page) => c.search.page(page, &query, append),
                    Err(code) => {
                        log::warn!("[settings.McpCatalogBrowser] MCP catalog {} failed: {code}", if append { "next page" } else { "search" });
                        if append { c.search.set_more(More::Failed) } else { c.search = Search::Failed }
                    }
                }
            }
            CatalogFetch::Status(request) => {
                let c = &mut self.mcp_ui.catalog;
                if request < c.status_applied {
                    return;
                }
                c.status_applied = request;
                match result {
                    Ok(catalog) => {
                        let requested = match &c.status {
                            CatalogStatus::Ready { requested, .. } => *requested,
                            _ => None,
                        };
                        c.status = CatalogStatus::Ready { catalog, requested };
                    }
                    Err(e) => {
                        log::warn!("[settings.McpCatalogBrowser] MCP catalog status failed: {}", code_of(&e));
                        if !matches!(c.status, CatalogStatus::Ready { .. }) {
                            c.status = CatalogStatus::Unavailable;
                        }
                    }
                }
                let poll = self.catalog_poll();
                self.mcp_ui.catalog.next_status = Some(self.now + poll);
            }
            CatalogFetch::Sync(request) => {
                let c = &mut self.mcp_ui.catalog;
                c.toggling = false;
                c.status_applied = c.status_applied.max(request);
                let answer = match result {
                    Ok(v) => Some(v),
                    Err(e) => {
                        let code = code_of(&e);
                        log::warn!("[settings.McpCatalogBrowser] MCP catalog sync change failed: {code}");
                        self.toasts.push(self.now, ui::overlay::ToastKind::Error, rules::sync_error_text(&code), None);
                        None
                    }
                };
                if let CatalogStatus::Ready { catalog, requested } = &mut self.mcp_ui.catalog.status {
                    if let Some(v) = answer {
                        *catalog = v;
                    }
                    *requested = None;
                }
            }
        }
        self.catalog_follow();
    }

    /// `useSearchFollowsCatalog`: a catalog newer than the rows searches again, once per version.
    fn catalog_follow(&mut self) {
        let c = &self.mcp_ui.catalog;
        let (CatalogStatus::Ready { catalog, .. }, Search::Ready { results, .. }) = (&c.status, &c.search) else { return };
        let updated = catalog.get("updatedAt").and_then(Value::as_str);
        let sample = catalog.get("sample") == Some(&json!(true));
        let servers = catalog.get("servers").and_then(Value::as_u64).unwrap_or(0);
        let behind = updated != results.updated_at.as_deref() || sample != results.sample || (results.query.is_empty() && servers != results.total);
        if !behind {
            return;
        }
        let key = format!("{updated:?}|{sample}|{servers}");
        if c.retried_for.as_deref() == Some(key.as_str()) {
            return;
        }
        self.mcp_ui.catalog.retried_for = Some(key);
        self.catalog_search();
    }

    /// Why Install is off for every row, if it is.
    fn catalog_block(&self) -> Option<Block> {
        if self.active_project().is_none() {
            Some(Block::NoProject)
        } else if !self.chat.acp.mcp_servers_loaded {
            Some(Block::Loading)
        } else {
            None
        }
    }
}

/// The bytes to drop so `text` keeps `max` characters: the ones just before the caret, where
/// the new text went in, else the end.
fn overflow(text: &str, caret: usize, max: usize) -> Option<std::ops::Range<usize>> {
    let extra = text.chars().count().checked_sub(max).filter(|&n| n > 0)?;
    let before: Vec<usize> = text[..caret].char_indices().map(|(i, _)| i).collect();
    if before.len() >= extra {
        return Some(before[before.len() - extra]..caret);
    }
    let keep = text.char_indices().nth(max).map_or(text.len(), |(i, _)| i);
    Some(keep..text.len())
}

fn search_path(query: &str, cursor: Option<&str>) -> String {
    let mut params = Vec::new();
    if !query.is_empty() {
        params.push(format!("query={}", ui::composer::encode_query(query)));
    }
    if let Some(cursor) = cursor {
        params.push(format!("cursor={}", ui::composer::encode_query(cursor)));
    }
    params.push(format!("limit={}", rules::PAGE_SIZE));
    format!("/mcp-catalog/search?{}", params.join("&"))
}

/// `CatalogCard`.
fn catalog_card(s: &State, t: &'static Tokens, r: &Row, block: Option<Block>) -> El {
    let c = &t.colors;
    let name = r.display_name().to_owned();
    let installed = s.chat.acp.mcp_servers.iter().any(|server| server.get("catalog").and_then(|k| k.get("name")).and_then(Value::as_str) == Some(r.name.as_str()));
    let reason = (!installed).then(|| r.reason.as_ref().map(rules::reason_text)).flatten();
    let mut title = row().key("title").min_w(0.0).grow(1.0).gap(6.0).items_center().child(text(name.clone()).font(sans(14.0, 500.0, 20.0)).color(c.foreground).single_line().truncate().min_w(0.0));
    if r.status == "deprecated" {
        title = title.child(text("Deprecated").font(sans(10.0, 500.0, 20.0)).color(c.warning).px(6.0).radius(10.0).fill(c.warning.alpha(0.1)));
    }
    let action = if installed {
        rune::icon(glyph("Check"), 14.0).color(c.success).key("added").label("Added")
    } else {
        let row_for = r.name.clone();
        Button::new("Install")
            .variant(ButtonVariant::Secondary)
            .size(ButtonSize::Xs)
            .disabled(block.is_some() || r.state != "installable")
            .build(t)
            .key("install")
            .on_press(move |s: &mut State| s.toast_later(&format!("Installing {row_for} from the catalog")))
    };
    let head = row().key("head").w_full().gap(10.0).items_center().child(letter_tile(t, &name, 32.0)).child(title).child(action);
    let mut body = col().key(format!("catalog-{}", r.name)).role("group").label(name.clone()).gap(10.0).padding(14.0).radius(8.0).fill(c.card).border(1.0, c.border).items_stretch().child(head);
    if !r.description.is_empty() {
        body = body.child(text(r.description.clone()).font(sans(12.0, 400.0, 16.0)).color(c.secondary_foreground).wrap().max_lines(2).min_h(32.0));
    }
    let (word, word_tone) = if installed { ("Installed", StatusTone::Success) } else { (rules::state_label(&r.state), tone(rules::state_tone(&r.state))) };
    let mut facts = col()
        .key("facts")
        .min_w(0.0)
        .grow(1.0)
        .gap(4.0)
        .child(text(r.name.clone()).font(mono(12.0, 400.0, 16.0)).color(c.muted_foreground).single_line().truncate())
        .child(row().key("state").gap(8.0).items_center().child(status_word(t, "state-word", word_tone, word, false)).child(text(r.version.clone()).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground)));
    if let Some(reason) = reason {
        facts = facts.child(text(reason).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground).wrap());
    }
    let mut foot = row().key("foot").w_full().gap(8.0).items_end().child(facts);
    if let Some(url) = r.repository_url.clone() {
        foot = foot.child(ui::quiet_icon_button(t, glyph("ExternalLink"), format!("Open the {name} repository"), 28.0, 13.0).key("repository").on_press_tx(move |_: &mut State, tx: &mut Tx| {
            if let Err(e) = tx.open_url(&url) {
                log::warn!("[settings.McpCatalogBrowser] the repository link did not open: {e}");
            }
        }));
    }
    body.child(foot)
}

/// The source card: where the catalog comes from, its sync state, and the sync switch.
fn source_card(s: &State, t: &'static Tokens) -> El {
    let c = &t.colors;
    let line = |words: String| text(words).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground).wrap();
    match &s.mcp_ui.catalog.status {
        CatalogStatus::Loading => card(t, "catalog-source", vec![row().key("loading").px(16.0).py(12.0).child(line("Loading…".into()))]),
        CatalogStatus::Unavailable => line("Troupe couldn't check where the catalog comes from.".into()).key("catalog-source"),
        CatalogStatus::Ready { catalog, requested } => {
            let registry = rules::REGISTRY_URL;
            let link = row()
                .key("registry")
                .role("link")
                .label("official MCP Registry")
                .focusable(true)
                .gap(4.0)
                .items_center()
                .child(text("official MCP Registry").font(sans(12.0, 500.0, 16.0)).color(c.primary))
                .child(rune::icon(glyph("ExternalLink"), 12.0).color(c.primary).a11y_hidden())
                .on_press_tx(move |_: &mut State, tx: &mut Tx| {
                    if let Err(e) = tx.open_url(registry) {
                        log::warn!("[settings.McpCatalogBrowser] the registry link did not open: {e}");
                    }
                });
            let updated = catalog.get("updatedAt").and_then(Value::as_str);
            let mut words = col()
                .key("words")
                .px(16.0)
                .py(12.0)
                .gap(4.0)
                .child(row().key("from").gap(4.0).items_center().child(text("From the").font(sans(12.0, 500.0, 16.0)).color(c.foreground)).child(link))
                .child(line(rules::updated_text(updated, &crate::mcp_catalog::local_time(updated.unwrap_or_default()))));
            if let Some(sample) = rules::sample_text(catalog) {
                words = words.child(line(sample));
            }
            let retry = catalog.pointer("/sync/retryAt").and_then(Value::as_str).unwrap_or_default();
            if let Some(sync) = rules::sync_text(catalog, &local_time(retry)) {
                let state = rules::sync_state(catalog);
                let tone = match state {
                    "idle" => StatusTone::Success,
                    "failed" => StatusTone::Danger,
                    _ => StatusTone::Muted,
                };
                words = words.child(status_word(t, "sync-word", tone, &sync, state == "syncing"));
            }
            let on = requested.unwrap_or(rules::sync_state(catalog) != "disabled");
            let toggle = switch_row(t, "mcp-catalog-sync", "Keep the catalog up to date", "The host downloads the official MCP Registry and checks it for changes.", on, |e| {
                if requested.is_some() { e.opacity(0.5) } else { e.on_press(move |s: &mut State| s.catalog_set_sync(!on)) }
            });
            card(t, "catalog-source", vec![words, toggle])
        }
    }
}

/// A date as the source card shows it (`Intl` `medium` date and `short` time, `en-US`).
pub(crate) fn local_time(iso: &str) -> String {
    #[cfg(target_arch = "wasm32")]
    {
        let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_str(iso));
        if date.get_time().is_nan() {
            return iso.to_owned();
        }
        let month = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"][date.get_month() as usize % 12];
        let (hour, minute) = (date.get_hours(), date.get_minutes());
        let (h12, half) = (if hour % 12 == 0 { 12 } else { hour % 12 }, if hour < 12 { "AM" } else { "PM" });
        format!("{month} {}, {}, {h12}:{minute:02} {half}", date.get_date(), date.get_full_year())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        iso.replace('T', " ").trim_end_matches('Z').chars().take(16).collect()
    }
}

/// `McpCatalogBrowser`: the search, the cards and the source card.
pub fn browser(s: &State, t: &'static Tokens, phone: bool) -> El {
    let c = &t.colors;
    let catalog = &s.mcp_ui.catalog;
    let block = s.catalog_block();
    let shown = catalog.search.shown();
    let search = filter(
        t,
        "catalog-search",
        &catalog.query,
        "Search the MCP catalog",
        "Search the MCP catalog",
        phone,
        |e| e.on_edit(|s: &mut State| &mut s.mcp_ui.catalog.query).on_input_change(|s: &mut State, _| s.catalog_query_changed()),
        |e| e.on_press(|s: &mut State| s.mcp_ui.catalog.query = TextEdit::new("")),
    )
    .w_full();
    let mut content = vec![search];
    let note = |key: &str, words: String| row().key(key.to_owned()).w_full().px(16.0).py(12.0).gap(8.0).items_center().radius(8.0).fill(c.card).border(1.0, c.border).child(text(words).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground).wrap());
    match &catalog.search {
        Search::TooManyTerms => content.push(toned_notice(t, "too-many", ui::settings::Tone::Warning, &format!("Search with {} words or fewer.", rules::MAX_QUERY_TERMS))),
        Search::Failed => content.push(
            col().key("failed").w_full().gap(8.0).items_start().child(toned_notice(t, "failed-notice", ui::settings::Tone::Danger, "Troupe couldn't load the MCP catalog.")).child(
                Button::new("Try again").variant(ButtonVariant::Secondary).size(ButtonSize::Xs).build(t).key("retry-catalog").on_press(|s: &mut State| s.catalog_search()),
            ),
        ),
        _ => match shown {
            None => content.push(note("catalog-loading", "Loading the catalog…".into())),
            Some(results) if results.updated_at.is_none() => content.push(toned_notice(t, "no-catalog", ui::settings::Tone::Warning, "No catalog is loaded yet, so there is nothing to browse.")),
            Some(results) if results.rows.is_empty() => content.push(
                col().key("no-match").w_full().px(16.0).py(32.0).gap(12.0).items_center().radius(8.0).fill(c.card).border(1.0, c.border).child(text(format!("No servers match \"{}\".", results.query)).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground)).child(
                    Button::new("Clear search").variant(ButtonVariant::Secondary).size(ButtonSize::Xs).build(t).key("clear-search").on_press(|s: &mut State| s.mcp_ui.catalog.query = TextEdit::new("")),
                ),
            ),
            Some(results) => {
                let per_row = if phone { 1 } else { 2 };
                let mut grid = col().key("catalog-grid").w_full().gap(12.0).items_stretch();
                for (i, chunk) in results.rows.chunks(per_row).enumerate() {
                    let mut line = row().key(format!("line-{i}")).w_full().gap(12.0).items_stretch();
                    for r in chunk {
                        line = line.child(catalog_card(s, t, r, block).grow(1.0).min_w(0.0));
                    }
                    for gap in chunk.len()..per_row {
                        line = line.child(rune::el().key(format!("gap-{gap}")).grow(1.0));
                    }
                    grid = grid.child(line);
                }
                content.push(col().key("mcp-catalog-scroll").w_full().max_h(GRID_MAX_HEIGHT).child(grid).scroll(rune::ScrollSpec { stick_bottom: false }));
                if results.next_cursor.is_some() {
                    let more = match &catalog.search {
                        Search::Ready { more, .. } => *more,
                        _ => More::Idle,
                    };
                    let can_more = catalog.search.more_cursor().is_some();
                    let mut paging = row().key("paging").w_full().gap(12.0).items_center();
                    if can_more && more == More::Loading {
                        paging = paging.child(text("Loading more…").font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground));
                    } else if can_more {
                        paging = paging.child(
                            Button::new(if more == More::Failed { "Try again" } else { "Load more" })
                                .variant(ButtonVariant::Ghost)
                                .size(ButtonSize::Xs)
                                .build(t)
                                .key("load-more")
                                .on_press(|s: &mut State| s.catalog_more()),
                        );
                    }
                    let mut words = format!("Showing {} of {}.", rules::count(results.rows.len() as u64), rules::count(results.total));
                    if !can_more {
                        words.push_str(" Search to narrow the list.");
                    }
                    paging = paging.child(text(words).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground));
                    if more == More::Failed {
                        paging = paging.child(text("Couldn't load more servers.").font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground));
                    }
                    content.push(paging);
                }
            }
        },
    }
    content.push(source_card(s, t));
    if let Some(block) = block {
        content.push(notice(t, "install-block", block.text()));
    }
    let meta = shown.map(rules::summary).unwrap_or_default();
    group_with_meta(t, "mcp-catalog", "From the catalog", &meta, vec![], content)
}

#[cfg(test)]
#[path = "mcp_catalog_tests.rs"]
mod tests;
