# model

Client state for the Rune shell, with no UI and no I/O. Each store is a plain
state type and a `step(state, event) -> effects` function. The caller (the
Rune app) carries the effects out: it sends commands through the `client`
crate, writes the device record, moves the route, and writes the log.

The TypeScript stores in `src/renderer/stores/` are the reference. The
recorder in `tools/parity/vectors/` runs them on scripted events and writes
the input and the state after every step as JSON. `tests/vectors.rs` replays
every file and compares.

## What the client holds, and what the server already owns

The server sends these ready to use, through the `/ws` watches
(`src-tauri/src/web/watch/hub.rs`). The client keeps the latest frame and
derives nothing from it.

| Watch | What it carries | Client state here |
|---|---|---|
| `projects` | project list, archived flag, host default | none beyond the merge below |
| `chat_history` | persisted chat index | none |
| `agents` | live agent roster | none |
| `chat_activity` | working, needs you, unread per chat | none |
| `terminals` (per project) | the project's terminal set | input to `workspace` terminal sync |
| `fs` (per project) | file changes below the root | none |
| `store` (per key) | one shared store key | none |

The client must hold what no server watch carries, because it is a choice of
this device or this window:

- `workspace`. The pane tree, tabs, active and fullscreen pane, launcher and
  agent-browser pane, per project, plus the parked trees of visited projects.
  The server knows the terminals and chats but not where they sit on screen.
- `projects`. The device's active project, the sidebar order and groups, and
  the client-only fields of each project (shell, env vars, worktrees, git
  state, icon). A `projects` watch frame replaces the host fields and keeps
  these. Add, rename, recolour, move, archive and delete are host commands:
  `step` returns them as effects and the next watch frame shows the result.
- `prefs`. The device record (`project-prefs` store key) that keeps the
  client-only fields and the sidebar layout across restarts, and the
  signature that skips a write when nothing changed.
- `shortcuts`. The key bindings and their user overrides, with the pure key
  matching helpers.
- `overlay`. The stack of open overlays, for back and Esc. The model holds the
  ids. Each overlay's close is the owner's, so `step` returns a `close` effect
  and the owner runs it.

## What is not here

- Async flows that wait on the host (`addProject` waiting for the watch, the
  busy-project dialog, `deleteProject` unmounting canvases first). The model
  emits the command; the app layer and the `client` ledger own the wait.
- The quick-chat interplay in `selectProject`. The quick-chat store is not
  ported yet; the vectors record it closed.
- Secure storage for secret env vars. The vectors record the web path, where
  env vars are not persisted. The desktop path needs a keychain effect.
- The overlay back-button history reconciler (`installOverlayBackHandler`).
  It drives `window.history`; the Rune web client will need its own.
- macOS key handling. The recorder runs with `isMac` false.

## Ids

The TypeScript stores mint ids with `crypto.randomUUID()`. The model never
generates randomness: `step` takes an [`Ids`](src/lib.rs) source. The app
passes a random one; the vector replay passes the ids the TypeScript run drew,
in order, and checks that the model drew the same number.

## The ACP chat model

`src/acp/` ports the ACP chat store (`src/renderer/stores/acp-store/`):
sessions, transcripts, tool calls, plans, commands and usage, one module per
slice. `docs/rune/acp-model.md` at the repository root maps each TypeScript
module to its Rust one, says what is state, effect or React glue, and lists
what is not ported yet. Its vectors are recorded apart, by
`tools/parity/vectors/record-acp.test.ts` into `out/acp/`, and replayed by
`tests/acp_vectors.rs`. A step reads the clock and draws ids only through
`acp::Env`.
