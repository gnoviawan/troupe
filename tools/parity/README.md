# Parity matrix

The parity matrix drives the same user steps in two clients and compares one frame per step. A reference client (`--against`, default `react`) runs first, and a candidate client (`--client`) runs second. Both start as a fresh device on one seeded `troupe-server`. The matrix scores each surface and writes `report.md`.

```sh
bun run build:web
bun run parity:matrix --client react --against react     # self-check: reads zero difference
bun run parity:matrix --only rail.project-menu --client-dist /tmp/scratch/dist-web
bun run parity rail.project-menu                         # writes pair.png next to each step
bun run parity:matrix --profile phone --client react --against react   # the 390x844 touch profile
bun run parity:matrix --profile gallery --client rune-web --against react  # design-system registry against the Rune gallery
bun tools/parity/rune-check.ts                           # both Rune drivers on hello: press, read "Pressed 1"
```

A run drives one profile. `desktop` (the default) is a 1280x800 window with a mouse. `phone` is a 390x844 window with touch and `isMobile`, which puts the web client on its phone shell, and only web clients run it. A phone run writes `report-phone.md` and `results-phone.json` beside the desktop files. Run one profile at a time per `--out`. `gallery` is not a window: it compares each `design-system` registry entry with the Rune gallery scene of the same id, one frame each, and writes `report-gallery.md`, `results-gallery.json` and `gallery/<id>/pair.png` (reference, candidate, diff). It needs no server.

The server binary comes from `TROUPE_SERVER_BIN`, or from `--server-bin`, or from this checkout's release build. Each run seeds a fresh world under `--state` and writes frames, `results.json` and `report.md` under `--out`. Both default to directories under `/tmp/troupe-parity`.

## Files

- `surfaces/` lists the surfaces, one file per area. Each surface step is a list of actions and checks, written with `surfaces/dsl.ts`. A check passes on painted text: rendered text, or the placeholder of an empty field.
- `surfaces/inventory.ts` measures the UI from `src/renderer`: every overlay mount site and every screen (route, gate, pane tab, side-panel tab, settings page). Each surface names its `source`, and `__tests__/surfaces.test.ts` fails when a mount site or a screen has no surface, so a new dialog needs a surface in the same PR. A surface that a seeded world cannot open stays in the list as `blocked`, with the reason.
- `surfaces/phone.ts` holds the phone profile: the phone-only overlays and one journey per `tests/e2e/mobile-*.spec.ts`.
- `world.ts` seeds the server: four projects, a git repo with staged, unstaged and untracked changes, a chat from the scripted agent in `agent/`, one skill, and a fixed page clock.
- `drivers/` holds the clients. `react` is the web client in Playwright Chromium, and is the reference capture. `tauri` is the desktop app under Xvfb, driven with XTEST at the points the reference recorded, and read only through pixels. `rune-web` is a Rune wasm build in Playwright Chromium: it finds targets by text in Rune's accessibility mirror (`[data-rune-a11y]`) or at a recorded point, reads checks from the mirror, and records whether WebGPU, WebGL2 or the CPU canvas drew. `rune-desktop` is a native Rune window under Xvfb with lavapipe, driven through the live driver socket (`RUNE_DRIVE_SOCK`): find by key, text or name, click, type, keys, and `snap` frames. Both Rune clients open the workspace shell: `rune-web` loads the `troupe-web` build from `target/web-troupe`, served on the seeded server's origin by a small proxy (`serveRuneClient`), and `rune-desktop` starts `troupe-dev` with `TROUPE_SERVER_URL`. They run only the surfaces the shell draws (`RUNE_SHELL_SURFACES`); every other surface is pending on the unit that ports it (`runePending`), and a run of pending surfaces alone starts no server. `drivers/rune-apps.ts` lists where the builds live: `apps/rune/target/web`, `target/web-gallery`, `target/web-troupe` and `$CARGO_TARGET_DIR/release/{hello,gallery,troupe-dev}`, or `TROUPE_RUNE_<APP>_WEB` and `TROUPE_RUNE_<APP>_BIN`. `TROUPE_RUNE_WEB_GPU=1` offers the wasm build a WebGPU adapter; a headless screenshot of that canvas came out blank, so the CPU canvas is the default.
- `lanes/` holds live lanes that check more than frames. `lanes/composer-send.ts` sends the same composer draft (a skill pill in text) from the launcher and then in the new chat, in the React client and in the Rune web client against one seeded world, and fails unless the prompts the scripted agent received (`PARITY_PROMPT_LOG`) are byte for byte equal.
- `depth/` holds checks deeper than a frame. `depth/term-record.py` records PTY sessions (a colour, attribute and box-drawing screen from `term-screen.sh`, `ls --color`, the edge cases in `term-edges.sh`, `vim`, `htop`) to `apps/rune/crates/term/tests/fixtures/<name>.bin` with three checkpoint offsets each (`<name>.checkpoints`). `depth/term-cells.ts cells <dir>` feeds those bytes to `@xterm/headless` with the React terminal's options and dumps every cell, the cursor and the modes at each checkpoint to `<name>.cells`, which `cargo test -p troupe-term` (`tests/cells.rs`) compares cell for cell. `depth/term-cells.ts widths <file>` regenerates the Rune terminal's character width table from xterm.js's own.
- `diff.ts` counts the pixels that differ (exact and perceptual), in total and in three parts: text, icons and boxes. The reference DOM names the parts.
- `status.ts` decides each surface's status. A surface is `ported` only when no step differs perceptually. Any difference, however small, reads `needs-review`, and only a person's verdict in `reviews.ts` turns it into `ported`, `partial` or `broken`. A review records the largest difference it saw (`acceptsPx`) and stops applying when a later run differs by more.
- `weights.ts` holds the area weights and the two scores. An unreviewed difference (`needs-review`) scores 0, so a run alone never raises the score. `report.ts` renders the report.

## Add a client

Implement `ClientLauncher` from `drivers/driver.ts` and add the client to `CLIENTS` in `types.ts` and to the `launchers` table in `run.ts`. A client that cannot find targets itself sets `resolvesTargets: false`, and then acts at the points the other client recorded. After a click, a driver moves the pointer to `PARK_POINT`, so that a delayed hover look does not change the frame.
