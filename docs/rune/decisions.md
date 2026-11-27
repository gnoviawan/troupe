# Rune decisions

Answers to the open questions in plan section R0, one entry per spike. Each
entry gives the answer, its key numbers and the report it came from. Unless an
entry says otherwise, every number was measured on a loaded shared host with
no GPU: 8 cores, load average 14 to 47, Mesa lavapipe for Vulkan, debug builds
with dependencies at `opt-level=2`. Read these numbers as a floor, not as what
a user sees.

Reports live in `~/.claude/orchestrate/troupe-rune/reports/` and images in
`/root/rune-research/evidence/` on the spike host, unless the entry names
another path.

## Live driver

**Answer.** An in-app control socket. A `Compositor` gets the window's live
`Runtime` every frame, so it can read the element tree and the accessibility
tree, inject pointer and key input, and draw the frame again into an
`Offscreen` for a screenshot. This uses only Rune's public API at the pin and
needs no Rune patch. AT-SPI and `--realtime` were not tried: the socket passed
first, and neither alternative can wait on a condition and capture a frame on
all three operating systems.

**Where it lives.** `apps/rune/crates/automation`, behind its `tooling`
feature. `RUNE_DRIVE_SOCK=<path>` turns it on. `scripts/rune/drive.mjs` is the
client, and the `Rune` workflow drives a real `hello` window with it on Linux.

**Numbers (spike, 50 launches).**

| Metric | p50 | p95 | max |
|---|---|---|---|
| `find` round trip | 1.05 ms | 26.7 ms | 171 ms |
| `click` to `wait` seeing the new label | 39.7 ms | 338 ms | 1,023 ms |
| Flake | 50 of 50 passed | | |

The driver's `snap` matched an X11 grab of the same window exactly (PSNR
infinite). The R0b run of `apps/rune/crates/automation` on `hello` under Xvfb
repeated both results once: `click` 39 ms, `wait` for "Pressed 1" 37 ms,
`snap` 170 ms, PSNR infinite against `ffmpeg x11grab`.

**Limit.** Injected input enters after the OS message path. It does not test
winit's pointer and DPI translation or IME from the OS, so lanes that need
those still need a platform-level check.

**Source.** `R0-live.md`, `R0b.md`. Images: `r0-live-driver.png`,
`r0-live-insnap.png`, `r0-live-x11grab.png`, and
`~/.claude/orchestrate/troupe-rune/media/r0b/r0b-live-driver.png`.

## State port

**Answer.** Porting state logic costs about one Rust line per TypeScript line,
and it is not the program's bottleneck. The parity vector corpus is.

**Numbers.** The spike ported the pure reducer core of the `acp-store`
transcript slice: 872 of the slice's 1,937 TypeScript lines, as 883 Rust lines
(ratio 1.01). It replayed 38 of 38 recorded vectors. The time from start to
the first green replay was 39 min 30 s. Extrapolated to all 135,117 logic
lines, the port is about 100 to 250 agent-hours (a guess that assumes this
slice is typical). That excludes the vectors for 11,761 test cases, which
must be recorded and reviewed.

**What did not port cleanly.** Dynamic `ToolCall` and `ContentBlock` shapes
(kept as `serde_json::Value`), JavaScript `undefined` and UTF-16 length
semantics, module-level mutable state, and the flush clock's timers and
`requestAnimationFrame` batching, which need an injected clock.

**Source.** `R0-state.md`. Artifacts: `/root/rune-research/spikes/state/`.

## Browser

**Answer.** Pending. `R0-browser2.md` does not exist yet. The first attempt,
`R0-browser.md`, is partial and settles nothing: it built no Rune pane and
measured no tunnel, so the desktop path, the web path and the need for CEF
are all unproven.

**Numbers from the partial attempt.** Headless Chromium over
`--remote-debugging-pipe`, JPEG screencast at up to 1280x800: 6.1 frames per
second while scrolling, 16.4 while a timer moved an element. The time from a
key send to the next screencast frame was 118 ms median and 431 ms p95. That is
a proxy: it does not prove the character was drawn.

**Source.** `R0-browser.md`. Images: `r0-browser-google.png`,
`r0-browser-cloudflare.png` (not reviewed).

## Code editor

**Answer.** Both. The code editor is a custom view: a `ropey` rope behind
`virtual_rows`, with each virtual row holding 32 lines. Rune's `TextEdit`
stays for small fields such as the chat input, rename boxes and settings.

**Numbers.** CPU time from one input to the frame that commits it, headless:

| Editor, 1 MB file | Typing p95 | Enter p95 | Undo p95 | Heap |
|---|---|---|---|---|
| `TextEdit` | 30.65 ms | 31.01 ms | 27.22 ms | 73.1 MB |
| Rope, one row per line | 5.48 ms | 15.49 ms | 5.63 ms | 14.9 MB |
| Rope, 32-line rows | 13.96 ms | 5.09 ms | 6.08 ms | 13.6 MB |

`TextEdit` passes the 16 ms bar at 200 KB but fails it at 1 MB. With one row
per line, Enter re-keys every row because `virtual_rows` re-keys whenever the
row count changes; 32-line rows keep the count steady.

**Still to build in R8.** Gutter, folding, multiple carets, find and replace,
mouse hit testing, horizontal scroll, and an accessible text role for the
custom view.

**Source.** `R0-editor.md`. Image:
`/root/rune-research/spikes/editor/shot-editor-c-1mb.png`.

## FigCraft

**Answer.** Yes, without a webview, on two paths. Path A runs FigCraft's CPU
renderer on a worker thread and shows its pixels through `El::pixels`. Path B
runs FigCraft's whole egui editor through `egui-wgpu` inside a Rune
`El::effect` slot, on Rune's own device. Use path B for the R11 canvas pane,
because it is the editor users have today. Keep path A for read-only previews.
FigCraft has no GPU renderer: both paths draw the canvas on the CPU.

**Numbers.**

| Measure | Path A | Path B |
|---|---|---|
| Launch to the first frame showing the document | 1.55 to 1.87 s | 2.1 to 6.4 s |
| Compositor CPU per frame while panning, p50 | 0.09 ms | 2.5 ms |

**Work left for path B.** Remove a lifetime transmute in the egui render
call, bridge keys, text, IME, clipboard and cursor, and honour egui's repaint
requests.

**Fixture.** `apps/rune/fixtures/sample.figcraft` is a 1.4 KB document written
by `make_sample.py`: one frame, two text nodes, two rectangles, an ellipse and
a vector. The spike's path A loads it in 196 ms and draws its shapes
(`~/.claude/orchestrate/troupe-rune/media/r0b/r0b-sample-figcraft.png`). Its text
is not drawn there. Inter is not installed on the host and the spike was built
without FigCraft's font input, so the cause is probably missing fonts
(inferred, not checked).

**Source.** `R0-figcraft-video.md`. Images: `r0-figcraft-pixels-window.png`,
`r0-figcraft-egui-window.png` and the other `r0-figcraft-*.png`.

## Video on Linux

**Answer.** Yes, outside Rune's `Player`, which fails off macOS and Windows. A
decoder outside the UI thread pushes NV12 `ByteFrame`s into a `NativeSurface`.
R8 builds a Troupe-owned player that paces frames on its own clock. Audio and
A/V sync are open.

**Numbers.** An ffmpeg subprocess decoded a 1280x720 H.264 clip at 30 frames
per second or more, using 25 to 35 % of one core. The window showed 9.7 to
13.4 frames per second at load 22 to 27, and fewer at higher load. The limit
is the window's frame rate on lavapipe under contention, not the decode
(inferred).

**Source.** `R0-figcraft-video.md`. Images: `r0-video-window-nv12.png`,
`r0-video-window-pixels.png`.

## Phone browsers

**Answer.** Pending. This needs the operator and two real phones.

**Checklist for the operator.**

1. Download the `hello-web` artifact from the latest green `Rune` workflow run
   on `dev`.
2. Serve the folder over HTTPS. For example, run
   `python3 -m http.server 8190` in it and open a cloudflared quick tunnel to
   port 8190.
3. Open the URL on one real iPhone (Safari) and one real Android phone
   (Chrome). For each phone, record the model and OS version, then check:
   - The window paints: a text field and a "Press me" button.
   - Tapping the field opens the keyboard, and typed text appears.
   - IME composition works: a Japanese or Chinese keyboard, and swipe or
     predictive text. The composed text lands once, without duplicates.
   - Tapping the button changes it to "Pressed 1".
   - Scrolling and pinch do not break the layout.
   - Rotating to landscape and back relays out the window.
4. Record pass or fail per item, with a screenshot per phone, in this section.

## Code highlighting

**Answer.** rune-highlights, the lexers rune-kit's `Markdown` already uses,
with each token kind coloured from the theme's syntax map exactly as the web
client's Shiki theme colours the TextMate scopes that kind stands for
(`ui::markdown::syntax_colour`), and a small re-cut where the two engines name
the same text differently (`ui::markdown::recut`: word-shaped `Number` tokens
become language constants or plain namespaces, primitive types become
keywords). It does not reach exact colour parity. Exact parity needs Shiki's
own engine, a TextMate tokenizer running the same grammars, which no section
has built yet.

**Numbers.** `bun tools/parity/highlight/parity.ts` runs Shiki with the default `termul`
theme (`syntaxThemeFor`) and `cargo run -p ui --example highlight_dump` over
the same eight samples, and counts non-space characters drawn in the same
colour (measured 2026-11-24):

| Engine | ts | tsx | bash | rust | python | json | css | go | all (1,742 chars) |
|---|---|---|---|---|---|---|---|---|---|
| rune-highlights, kinds mapped | 74.8% | 76.7% | 60.0% | 86.3% | 70.3% | 85.2% | 76.1% | 80.9% | 76.8% |
| rune-highlights, mapped and re-cut (shipped) | 81.6% | 76.7% | 60.0% | 86.3% | 75.6% | 88.1% | 79.8% | 80.9% | 79.6% |
| A TextMate engine with Shiki's grammars | not built | | | | | | | | 100% by construction (guess) |

The differences are token boundaries the grammars draw, not palette: Shiki's
TypeScript grammar colours a function's whole parameter list in the function
colour, Shiki's shell grammar colours every argument as a string, and its JSON
grammar draws a key's quotes as punctuation.

**Alternatives weighed.**

- A TextMate engine in Rust (for example `syntect` with `fancy-regex`) running
  the VS Code grammars Shiki bundles. Rejected for now: the eight grammars in
  the table are 623 KB of grammar source (measured from `@shikijs/langs`), all
  of Shiki's are 9.9 MB, TextMate matching is backtracking regex per line
  rather than linear lexing, and `fancy-regex` does not cover every Oniguruma
  construct the grammars use (guess, not tried). It is the only route to exact
  parity; take it if the matrix shows code colour is what keeps chat surfaces
  from scoring ported.
- Shiki through JavaScript in the web build. Rejected: the desktop app has no
  JavaScript, so the two Rune clients would colour code differently.

**Source.** R6b (`reports/R6b.md`). Tools: `tools/parity/highlight/`.

## Code editor buffer

**Answer.** `troupe-editor` keeps one file in a `ropey` rope (default
features off, so only `\n` breaks lines, as in CodeMirror) with snapshot undo:
each undo step is a clone of the rope before the edit, which shares every node
the edit did not touch. Every user action is a `Command` that `Editor::apply`
runs, so tests replay recorded input. One caret, because the React editor does
not turn on `allowMultipleSelections`. Steps join as CodeMirror's `history()`
joins them: the same kind of edit, from the caret the last one left, within
500 ms. Soft wrap breaks lines by columns, because the editor face is
monospace.

**Candidates weighed (one sequential pass; workers spawn no subagents).**

- Snapshot undo (chosen). Undo cannot drift from the text, because it restores
  the text it saved. A step costs a few rope nodes, about 1 to 4 KB (inferred
  from ropey's node size). The R0 spike measured undo p95 at 6.08 ms at 1 MB.
- Inverse-change undo (CodeMirror's `ChangeSet` shape): smaller steps, but
  every edit kind needs an inverse that must be right, and a wrong one corrupts
  the file. The memory win does not matter at 10,000 steps.
- Pixel wrap through `text::measure` per line, as `TextEdit` does. Rejected:
  the caret and Up and Down would need Rune's layout to place them. Column
  wrap needs no layout. Wide characters (CJK) wrap late, a known gap.

**Numbers.** `cargo test -p troupe-editor` (measured 2026-11-25): 1,000
recorded edits of every command kind, mixed with moves and selections, undo
back to the original text and redo back to the edited text.

**Source.** R8a (`reports/R8a.md`), `R0-editor.md`.

## Code editor highlighting

**Answer.** rune-highlights' lexers, the same as chat code, with each token
named by the Lezer tag that the React editor's grammar gives the same text.
The view colours that tag from the theme's syntax map, using
`codemirror-theme.ts`'s `HighlightStyle` rules. A re-cut per language
(`troupe-editor` `recut.rs`) covers where the two engines split text
differently. Examples are a method call after `.` (property), CSS units
(keyword), Markdown marks (keyword), Rust attributes (meta), and JavaScript and
Rust definitions. The view lexes only the 32-line chunks on screen, each with
up to 64 lines above it, so a block comment opened above a chunk still colours
it. Files the React editor loads no grammar for (Go, TOML, shell, XML) draw
plain in both clients.

**Numbers.** `bun tools/parity/editor-highlight/parity.ts` runs CodeMirror's
grammars with the `termul` theme's styles and the Rune dump over nine samples
copied from this repo. It counts non-space characters drawn in the same
colour (measured 2026-11-25):

| Engine | css | html | js | json | md | py | rs | ts | yaml | all (13,747 chars) |
|---|---|---|---|---|---|---|---|---|---|---|
| rune-highlights, tags mapped | 98.1% | 88.8% | 86.8% | 100% | 77.5% | 83.4% | 77.4% | 90.0% | 98.3% | 88.8% |
| mapped and re-cut (shipped) | 99.3% | 96.3% | 99.6% | 100% | 100% | 96.0% | 91.9% | 97.6% | 100% | 98.3% |

The gaps left are grammar context, not palette. Rust enum variants, struct
fields and `use` paths are tagged by position in Lezer's tree. Python
f-string parts and TypeScript type arguments are the others.

**Alternative weighed.** Lezer itself in Rust. There is no Rust Lezer runtime,
and porting the LR tables and the nine grammars' external tokenizers is a
project of its own. Take it only if the matrix shows editor colour keeps
editor surfaces from scoring ported.

**Source.** R8a (`reports/R8a.md`). Tools: `tools/parity/editor-highlight/`.

## Explorer tree

**Answer.** The explorer keeps its own state in `model::explorer`: the listed
folders, which are open, the selection, the inline name field and the delete
confirm. `ui::explorer` draws each visible row with rune-kit's `TreeItem` and
`TreeStyle`, the same row the `Tree` widget draws, in Rune's `virtual_rows`.
It does not use the `Tree` widget itself. The phone's Files sheet reads the
same listings and runs the same operations.

**Candidates weighed (one sequential pass).**

- rune-kit `Tree` over a `TreeModel` (rejected). Its rows take only
  `on_press` and `on_double_press`, so a row cannot open a context menu, be
  dragged onto a pane, or turn into the rename field. Its model also has no
  "listed or not" state for a folder whose children are not loaded yet. Such
  a folder would draw as a leaf with no chevron.
- Hand-drawn rows (rejected). They would duplicate the indent guides,
  chevron, icon and selection that `TreeItem::build` already draws.

**Source.** R8a (`reports/R8a.md`). Lanes: `tools/parity/lanes/explorer.ts`
(desktop and `--phone`), `tools/parity/lanes/editor-desktop.ts`.

## Markdown editor

**Answer.** The Markdown editor is the code editor's buffer drawn as a
document. `model::editor::markdown` splits the text into top-level blocks
(front matter, ATX headings, fenced code, and runs of other lines). Each
block is drawn rendered by rune-kit's `Markdown`, so a `mermaid` fence draws
as a diagram. The block under the caret is drawn as its source lines in the
code editor's rows, and it takes the code editor's keys. The toolbar's
Preview and Source switch shows the same buffer in the code view. The view
mode is kept in the workspace record (`OpenFile.viewMode`).

**Why.** The buffer is the file's text, so an unedited file saves byte for
byte and an edit changes only what was typed. The buffer keeps a file's
`\r\n` line breaks and its byte order mark aside and puts them back on write.
BlockNote, the web client's editor, parses the file into blocks and writes
them out again, which rewrites lists, spacing and line breaks.

**Candidates weighed (one sequential pass).**

- A rich-text model (blocks with styled runs) serialised back to Markdown
  (rejected). It is BlockNote's design, and every Markdown construct it does
  not model is lost or rewritten on save.
- A styled source view (rejected). Headings in a larger face break the code
  view's fixed row height, which its 32-line virtual rows and Up and Down
  need, and a mermaid fence would stay source.

**Limits.** A block draws alone, so a reference-style link whose definition
is in another block shows as text. A loose list splits at its blank lines.

**Source.** R8b (`reports/R8b.md`). Tests:
`apps/rune/crates/ui/src/editor/markdown_tests.rs` (20 fixture files).

## PDF engine

**Answer.** hayro 0.9, a pure-Rust PDF rasteriser (MIT or Apache-2.0), on
both targets. `ui::viewers::PdfDoc` parses the file once, keeps each page's
size, and rasterises a page on the CPU at the scale it draws at, the first
time `virtual_rows` shows it. On the desktop the parse runs on a thread of
its own and pages rasterise on one rasteriser thread per document; wasm has
no threads, so both run inline there. A page rasterises to at most 8192 px a
side, and the rasterised pages drawn longest ago are dropped past 256 MB
(at least four stay). Rune decodes the pixels into its image atlas off the
UI thread. Fonts are hayro's embedded standard 14; the CJK CMaps are
left out to keep the wasm build small.

**Candidates weighed (architect, one sequential pass: no subagents in this
unit).**

| Candidate | Native | Web | Verdict |
|---|---|---|---|
| hayro everywhere | pure Rust, a thread-free call | the same code in wasm | chosen: one path, headless tests render real pages |
| pdfium (`pdfium-render`) | a 5 MB native library per OS to ship | needs a pdfium wasm build | rejected: three binaries to ship, no shared test path |
| Native engine plus the browser's viewer in an `<iframe>` | hayro or pdfium | DOM over the canvas | rejected: two engines, and Rune menus cannot draw over the iframe |
| Server-side page images | none | none | rejected for now: needs a `troupe-server` route, outside R8 |
| MuPDF | C library | wasm build exists | rejected: AGPL |

**Limits.** hayro does not open encrypted PDFs (the viewer says the file is
password-protected) and lacks some blend and knockout features. On wasm a
page still renders on the UI thread when it first shows; a 612x792 page at
2x took tens of milliseconds in a debug test build.

**Source.** R8b (`reports/R8b.md`). Tests:
`apps/rune/crates/ui/src/viewers/tests.rs` (a 10-page fixture, page 5
rasterised and its pixels checked).

## Video viewer

**Answer.** The desktop player follows "Video on Linux": ffprobe reads the
clip's size, rate and length, and an ffmpeg subprocess decodes NV12 frames
into a `NativeSurface`, paced on the player's own clock. It has no sound
yet. The clip is staged for ffmpeg in a private temporary file (mode 0600,
a random name per player) that goes with the player, and staging, probing
and the first frame run off the UI thread. Clips larger than 8K
(7680 x 4320 pixels, or 8192 a side) are refused. The web client hands the bytes to a hidden `<video>` element, which
plays with sound, and copies its frames into `Pixels` every 33 ms, so Rune
draws the picture and menus stay over it.

## Desktop-only renderer sites

**Question.** The React renderer refuses some operations off the Tauri
desktop with a typed code: `WEB_UNSUPPORTED` (web client), `NOT_TAURI`
(browser pane) and `UNSUPPORTED_SURFACE` (phone-width shell). The Rune
desktop app talks to `troupe-server` like the web client does, so each
refusal needs a server route for a host-local client, or an operator ruling
that the desktop app may lose it.

**Answer.** 25 sites at `46df9af5a`, found with
`grep -rn "WEB_UNSUPPORTED\|NOT_TAURI\|UNSUPPORTED_SURFACE" src/renderer`
(tests left out). Sites 1 to 16 refuse an operation. Sites 17 to 25 only
react to a refusal or describe one, so they follow the site they name.
"Host-local" means the `/ws` grant from R10b (#2177): only the desktop app
on the server's own host, holding the owner-only credential, gets it. A route
that acts with the host's credentials, or reveals what the host runs, is
host-local only. A plain project operation uses the existing write
capability and project boundary instead, so the web client gains it too.

| # | Site | Refused operation | Decision |
|---|---|---|---|
| 1 | `lib/tauri-opener-api.ts:35` | `openWithExternalApp` | No route. The desktop app opens the path itself (`troupe-desktop` `services.rs` opener, R10a). A remote client stays refused. |
| 2 | `lib/tauri-opener-api.ts:108` | `revealInFileManager` | No route, as 1. |
| 3 | `lib/worktree-api.ts:30`, `removeAllManaged` | Remove every managed worktree | Route `POST /worktree/remove-all-managed`, calling `worktree_remove_all_managed_guarded` with the guards of `/worktree/remove`. |
| 4 | `lib/worktree-api.ts:30`, `archive` | Archive a worktree | Route `POST /worktree/archive`, same guards. |
| 5 | `lib/worktree-api.ts:30`, `restore` | Restore an archived worktree | Route `POST /worktree/restore`, same guards. |
| 6 | `lib/worktree-api.ts:30`, `mergePreview` | Preview a merge into a branch | Route `POST /worktree/merge-preview`. It reads only, inside the project boundary. |
| 7 | `lib/worktree-api.ts:30`, `mergeExecute` | Merge a worktree into a branch | Route `POST /worktree/merge-execute`, same guards as 3, ledgered like `/git/commit`. |
| 8 | `lib/tauri-filesystem-api.ts:475` | `searchContentStreamStart` | `/ws` request `search_content_stream` with results as events. It needs a request and an event name in `troupe-protocol`. Until then the one-shot `POST /search/content` serves the desktop app. |
| 9 | `lib/tauri-filesystem-api.ts:501` | `searchContentStreamCancel` | Served by the existing `POST /search/cancel` once 8 exists. |
| 10 | `lib/browser-api.ts:84-147` | Browser tabs: create, navigate, resize, history, reload, stop, zoom, screenshot | Owned by R11 ("Host the browser pane"). Not an R10 route. |
| 11 | `lib/browser-api.ts:155-176`, `:199` | Annotation, outbox sync, crop save, agentation injection | Owned by R11, as 10. |
| 12 | `lib/browser-api.ts:184-189` | Local dev server discovery, local port check | Host-local `/ws` requests `discover_local_servers` and `check_local_port`. They reveal what runs on the host, so never a plain route. |
| 13 | `lib/browser-api.ts:194` | Browser devtools | Needs operator ruling. Rune has no devtools, and R10a dropped Toggle DevTools from the menu. Default: the desktop app loses it. |
| 14 | `lib/ssh-api.ts:110` | SSH sessions, SFTP, keys, port forwarding | Served to host-local clients by R10b (#2178). Port forwarding is not served yet: no client calls it. SSH events need an `ssh` watch name in `troupe-protocol` before they reach the client. A remote client stays refused. |
| 15 | `lib/canvas-api.ts:31` | Canvas open on the phone-width shell | Needs operator ruling. It is a phone surface rule, not a desktop gap, so the desktop app keeps the canvas (R11). Default: the phone stays without it. |
| 16 | `lib/web-server-api.ts:167` | Streaming search and directory watching | Streaming search is 8. Directory watching is already served by the `fs` watch, so this comment is stale. A React edit is out of scope here. |
| 17 | `components/CommandPalette.tsx:258` | Hides Open Canvas on the phone | Follows 15. |
| 18 | `layouts/workspace-layout/use-tab-openers.ts:72` | Canvas open path | Follows 15. |
| 19 | `components/file-explorer/FileExplorer.tsx:42` | `.figcraft` opens as text on the phone | Follows 15. |
| 20 | `hooks/use-mobile-web-shell.ts:124` | The phone-width predicate the canvas facade reads | Follows 15. |
| 21 | `components/browser/BrowserControls.tsx:213` | Ignores `NOT_TAURI` from agentation | Follows 11. |
| 22 | `components/browser/BrowserPanel.tsx:109` | Ignores `NOT_TAURI` from tab calls | Follows 10. |
| 23 | `components/browser/AnnotationToolbar.tsx:94` | Ignores `NOT_TAURI` from annotation | Follows 11. |
| 24 | `lib/ssh-messages.ts:175`, `components/sidebar/ssh-section.tsx:95` | "SSH runs in the desktop app" message, and the SSH panel hidden on web | Follow 14. The Rune desktop app shows the panel. |
| 25 | `components/shell/use-chat-row-menu-actions.ts:86` | Reveal falls back to copying the path | Follows 2. |

`lib/web-setup-api.ts:187` names `WEB_UNSUPPORTED` only to explain its own
`DESKTOP_UNSUPPORTED` code, which refuses setup calls inside Tauri. It is not
a web gap, so it is not counted.

**Also needed (from R10a).** The Rune desktop app is a new device to the
store, so it does not see the Tauri webview's per-device settings
(`device/<tauri device>/…`). Since R10b (#2177) a host-local client takes
over the retired `termul-data.json` import at authenticate, but not a device
partition the Tauri webview already wrote. Proposal: a host-local `/ws`
request `adopt_device_partition { fromDeviceId }` that copies the partition
once, only into an empty one, and records the adoption like
`host.desktopImported`.

**Operator rulings to record in `preferences.md`.** 13 (devtools) and
15 (canvas on the phone), each with the default above.

**Source.** R10b (`reports/R10b.md`).
