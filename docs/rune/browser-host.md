# Browser host on the server

Status: design for plan section R11, units R11a (server), R11b (Rune pane) and
R11c (canvas). The server half lands behind a flag that is off by default.

## Problem

The Rune clients cannot embed a webview. The desktop app today shows a
browser tab as a Tauri child webview (`browser_tab_manager`), and agents drive
that same webview through `browser_automation`. `troupe-server` has neither: its
browser tool answers `capability_unavailable` (`browser_automation::no_host_error`),
and browsers and phones have never had a browser pane.

R0 measured the replacement. One headless Chromium, driven over the Chrome
DevTools Protocol (CDP) on a pipe, streams a tab to a Rune pane as screencast
frames and takes pointer, wheel, key and IME input back. Clicks, keys, wheel
and IME reached the page. Through a cloudflared quick tunnel, a key showed in
the page after 172 ms median and 254 ms p95. The pane drew 4 to 14 frames per
second on a loaded host with no GPU (R0-browser2, R0-browser3, measured). The
corruption R0 first saw came from the `image` crate's zune-jpeg 0.5.15, which
reads only the first of the three scans Chromium writes. `jpeg-decoder` and
PNG frames decode correctly.

The constraints the design keeps:

- The automation core (`browser_automation::core`) drives engines only through
  `engine::BrowserEngine` and tabs only through `core::surface::TabSurface`.
  Consent, the policy store, the tab registry, Stop and reply shaping stay in
  the core and stay engine-neutral.
- `engine/cdp` already holds the engine-neutral CDP logic (isolated-world
  eval, trusted input, file inputs, screenshots) over a `CdpTransport` and a
  closed method allow-list (`CdpMethod`). WebView2 is its only transport today.
- `/ws` request types default to `Requirement::Write`
  (`web::capabilities::ws_requirement`), so a read-only or remote view-only
  connection cannot mutate anything it was not explicitly granted.
- Nothing may log page content, cookies, typed text or tokens.

## Usage (caller's view)

An agent's `browser` tool call reaches the same `browser_automation::dispatch`
as on the desktop. On a server started with `--browser-host`, a host is now
registered there, and the call opens or drives a Chromium tab.

A Rune pane (R11b) speaks four new `/ws` requests and one watch:

```text
-> { type: "browser_tab_open", payload: { projectId, url, width, height } }
<- { ok: true, payload: { tabId: "tab-<uuid>" } }

-> { type: "watch", payload: { watch: "browser_screencast", scope: "tab-<uuid>" } }
<- reset { width, height }, then one delta per frame:
   { seq, format: "jpeg", data: "<base64>", width, height, scale }

-> { type: "browser_tab_input", payload: { tabId, event: { kind: "pointer", ... } } }
-> { type: "browser_tab_navigate", payload: { tabId, url } }
-> { type: "browser_tab_resize", payload: { tabId, width, height, scale } }
-> { type: "unwatch", ... }          the screencast stops when no watcher is left
-> { type: "browser_tab_close", payload: { tabId } }
```

An agent's tab appears in the same list (`acp:browser_agent_tab`). A pane
that watches its screencast shows what the agent does, and the user can take
over with the same input requests. That is the shared tab.

## Shape

### Two modules, one Chromium per profile

```text
browser_automation/engine/chromium/   the engine adapter
  launch.rs     find a browser binary, build its argv, spawn it on a pipe
  pipe.rs       one CDP connection over fds 3 and 4: requests, replies, events
  mod.rs        ChromiumEngine: BrowserEngine + CdpTransport over tab sessions
  surface.rs    ChromiumSurface: TabSurface (create, navigate, history, close)
  events.rs     CDP page events -> TabEvents (commits, loads, dialogs, titles)
  tests.rs      click, type and navigate a local page

browser_host/                          the process and stream owner
  mod.rs        BrowserHost: flag, profiles, idle lifecycle, tab -> project
  profiles.rs   ProfileKey -> running Chromium, launched on demand
  screencast.rs per-tab frame stream with a subscriber count
  input.rs      pane input events -> CDP Input.* (pointer, wheel, keys, IME, touch)
  perimeter.rs  the internal-address policy for every request a tab makes

web/ws/browser_pane.rs                 the /ws requests and the watch
```

The engine adapter knows CDP and nothing about processes it did not launch for
itself. It asks a `BrowserSource` for the connected Chromium that serves a tab:

```rust
pub trait BrowserSource: Send + Sync {
    /// The running Chromium for `store`, launched if none runs.
    fn browser_for<'a>(&'a self, label: &'a str, store: &'a DataStore)
        -> SourceFuture<'a, Result<Arc<Browser>, String>>;
    /// A tab of that browser opened or closed (drives the idle timer).
    fn tab_opened(&self, label: &str);
    fn tab_closed(&self, label: &str);
}
```

The engine tests and the conformance runner use a plain source that launches
one Chromium per store and keeps it. `browser_host::BrowserHost` is the
server's source: it maps a tab to its profile, launches on demand, and closes
idle processes. Per `boundary-discipline`, the process policy lives in one
module and the CDP mechanics in another, and neither reaches into the other.

### Profiles

A Chromium process has one persistent user-data dir, so one persistent
profile is one process.

| Tab | Store | Chromium |
| --- | --- | --- |
| A user tab of project P | `DataStore::App` | the process for `ProfileKey::Project(P)`, dir `<state>/browser-host/profiles/<P>` |
| An agent tab, global profile | `DataStore::Global` | the process for `ProfileKey::Agent`, dir `<state>/browser-host/agent` |
| An agent tab, ephemeral | `DataStore::Ephemeral` | a `Target.createBrowserContext` context in the agent process, disposed with its session |

User tabs of one project share cookies. Projects never share them. Agent tabs
never use a project's user profile, as on the desktop
(`engine/profile/mod.rs`). Project ids are host-minted, and the dir name is
the id, never a path from a client.

### Finding Chromium

`launch::find` returns the first hit and its kind, in this order:

1. `TROUPE_CHROMIUM_PATH`, the operator's override.
2. An installed Chrome or Edge, which update themselves: the per-OS install
   paths, then `google-chrome`, `google-chrome-stable`, `microsoft-edge` and
   `msedge` on `PATH`.
3. A distribution Chromium on `PATH` (`chromium`, `chromium-browser`).
4. A Chromium Troupe downloaded into `<state>/browser-host/chromium/<build>`.
   Download and its update check are a follow-up unit: they need a pinned
   manifest with signed hashes, as `server_update` does for the server. Until
   then step 4 reports "no browser found" and the tool keeps answering
   `capability_unavailable` with that reason.

### Launch

Every launch uses `--remote-debugging-pipe`. CDP runs on fds 3 (Chromium reads)
and 4 (Chromium writes) of the child, null-delimited JSON. No port is ever
opened, so no other local process can attach. The host also scrubs inherited
`--remote-debugging-port` style switches the same way `host_perimeter` scrubs
the webview environment.

Fixed flags, chosen for a host with no GPU adapter:

```text
--headless=new --remote-debugging-pipe --user-data-dir=<profile>
--no-first-run --no-default-browser-check --disable-background-networking
--disable-component-update --disable-sync --disable-extensions
--disable-default-apps --mute-audio --password-store=basic
--disable-gpu --disable-features=Translate,MediaRouter,OptimizationHints
--deny-permission-prompts
```

`--disable-gpu` keeps Chromium from starting a GPU process that would run
SwiftShader in software on a GPU-less host, and lets the compositor rasterise
on the CPU in the renderer, which only works while the tab is visible to the
screencast. The R11 perf lane proves the choice. `browser-idle` must stay under
1 percent of one core with three tabs open and the pane hidden.

The Chromium sandbox stays on. Chromium refuses to run as root with its
sandbox, and R0 measured that this host also blocks unprivileged user
namespaces. So the host refuses to launch as root unless the operator passes
`--browser-host-no-sandbox`, which is logged as a warning on every launch. On
an ordinary user account the sandbox works and no flag is needed.

### Idle lifecycle

Two counters per tab, owned by `BrowserHost`:

- **Watchers** of its screencast. `Page.startScreencast` runs while the count
  is at least one and `Page.stopScreencast` is sent when the last watcher
  leaves: an `unwatch`, a closed socket (`ConnectionCleanup`), the pane
  hidden, the tab shown in another thread, or the window minimised. R11b
  sends `unwatch` for each of those. The stream also stops when a watcher's
  socket queue is full, and starts again on the next acknowledged frame. Each
  frame is acknowledged to Chromium (`Page.screencastFrameAck`) only after it
  was queued, so a slow client paces Chromium and a hidden tab encodes
  nothing.
- **Open tabs** per profile process, user and agent alike. When a process has
  had no open tab for 5 minutes it gets `Browser.close`, then a kill after a
  5 s grace, and its entry goes. The next tab launches it again. An agent tab
  counts as open from its create to its close, so an agent between steps
  never loses its browser.

A Chromium that exits on its own (a crash, an OOM kill) fails every pending
call with `EngineErrorKind::NoReply`, publishes `Closed` for each of its tabs,
and drops its entry, so the next tab launches a fresh one. R11b draws the
crash notice from the `Closed` event.

### Security model

Who can reach a tab:

- **Agent calls** come through `browser_automation::dispatch` and keep every
  rule the desktop has: origin consent tickets, the sensitive-action gate,
  Stop, the session's own tabs only, and the upload root.
- **Pane requests** (`browser_tab_*`) are not on the view allow-list in
  `ws_requirement`, so they need `Capabilities::write`. A read-only link, a
  shared-live viewer and a remote peer without `--allow-remote-writes` are
  refused with `forbidden` before the handler runs. The screencast watch
  also needs write: a page can show a signed-in mailbox, so a read-only link
  must not see it either. The handler checks this itself, because `watch`
  is a view-level request type.
- **A tab of another project** is refused. `browser_tab_open` takes a
  `projectId` the registry knows, and later requests name a `tabId` the host
  minted for that connection's open or for an agent session.

Where a tab can go:

- `perimeter.rs` sees every request the page makes, through `Fetch.enable`
  on each tab session at the request stage. It refuses, by default:
  loopback, private (RFC 1918, unique-local IPv6), link-local, carrier-grade
  NAT, unspecified and cloud metadata addresses, whether written as a literal
  or reached by name, and every port Troupe itself listens on
  (`host_perimeter::owned_ports`). A hostname is resolved before the request
  continues, and refused when any address it resolves to is internal.
- The refusal is `Fetch.failRequest` with `BlockedByClient`. A main-frame
  hop is also reported to the core as a denied `NavStarting`, so the agent
  reply says it was blocked.
- The operator opts in with `--browser-host-allow-internal`, which allows
  loopback and private addresses. Link-local, metadata and Troupe's own
  ports stay refused even then.
- Non-http schemes other than `about:blank` and `data:` are refused.
  `file:` URLs are refused on the server: the desktop's local-file rules
  (`core::local_files`) do not apply to a host other people reach.
- Downloads go to a per-tab folder under the host state dir, through the
  core's download gate. Nothing writes to a project folder.

What never leaves the host:

- CDP runs on a pipe, never on a port.
- Logs carry the tab id, the method name, the status code and the reason.
  They never carry a URL query, page text, a cookie, a typed value, a frame
  or a token.
- Screencast frames go only to the connections that watch that tab and hold
  write.

Residual risks: DNS rebinding between the host's check and Chromium's own
lookup (Chromium resolves again). WebSocket and WebRTC traffic is not seen by
`Fetch`. A page can still fingerprint the host's public address. The design
accepts these for a flag-gated first version and lists them below.

## Synthesis decision

The architect skill asks for runner candidates from several models. Workers
in this program may not spawn sub-agents, so the two candidates below were
written inline and compared by hand.

- **Candidate A, a separate pane stack.** `browser_host` owns its own CDP client
  and tab list. The pane talks to it, and the automation core gets a thin
  `BrowserEngine` that forwards to it. It is simple to start. But a tab
  exists twice (the registry and the host list), agent tabs would not appear
  in the pane without a sync step, and consent would see only half the tabs.
- **Candidate B, the core owns tabs, the host owns processes.** The Chromium
  adapter is a normal engine plus a normal `TabSurface`. The core's registry
  is the only tab list for agent and user tabs alike, and `browser_host` only
  supplies processes and streams.

B is the base. It has one owner per piece of state: the registry for tabs, the
host for processes and watchers. Its public surface is the existing port plus
`BrowserSource`. From A it keeps one idea: the pane's input path goes straight
to CDP `Input.*` (`browser_host/input.rs`), not through the agent verbs. User
input needs no consent ticket, and it must not pay the agent path's
actionability checks.

## Tradeoffs accepted

- We accept one Chromium process per project profile, in exchange for
  persistent per-project cookies with no shared state. Idle shutdown keeps the
  cost bounded.
- We accept JPEG frames decoded by the client in exchange for 5 to 10 times
  fewer bytes than PNG. The Rune pane must use `jpeg-decoder`, because
  zune-jpeg 0.5.15 corrupts these frames.
- We accept intercepting every request with `Fetch` in exchange for a
  perimeter that covers subresources as well as navigations. It costs a
  round trip per request on the pipe.
- We accept no Windows pipe transport in R11a. Chromium on Windows takes
  inherited handles, which needs its own launch code. The server ships on
  Linux and macOS first.

## Alternatives considered

- **CEF offscreen rendering.** It gives frames without encode and decode, but
  it is a 200 MB native dependency with its own release cadence. R0 measured
  the screencast as fast enough for forms and sign-in, so nothing here needs
  it.
- **Remote debugging over a port.** It is simpler to attach with tooling, but
  any local process could attach and read every cookie. A pipe has no such
  door.
- **Playwright or chromiumoxide as the client.** Both work, but each brings a
  large dependency and its own method surface, while `engine/cdp` already
  holds the allow-listed calls the core needs. The pipe client is about 200
  lines.

## Implementation reconciliation

- **R11a PR 2** builds `engine/chromium`, the plain `BrowserSource`, and the
  `--browser-host` flag that registers the host in `troupe-server`. The
  conformance runner gains a Chromium mode (`TERMUL_BROWSER_CONFORMANCE_ENGINE=chromium`).
- **R11a PR 3** builds `browser_host` with the per-project profiles, the idle
  lifecycle, the screencast watch and the `/ws` requests.
- Deferred, with reasons, in the R11a report: the downloaded Chromium and
  its update check, a Windows pipe transport, and new-window interception.

## Open questions and risks

- Should a phone get a phone viewport by default? R11b can send
  `browser_tab_resize` with a mobile flag, which maps to
  `Emulation.setDeviceMetricsOverride { mobile: true }`.
- Should the operator be able to allow internal addresses per project instead
  of host-wide? The browser policy store already has a per-project allow-list
  that could carry it.
- Cloudflare showed headless Chromium a Turnstile challenge (R0-browser2,
  measured). A headful Chromium under a virtual display may pass it, but
  that has not been tried.

## Next implementation step

Write `engine/chromium/pipe.rs`, the CDP connection over the child's fds 3
and 4, with request ids, a pending-reply map and one event channel.
