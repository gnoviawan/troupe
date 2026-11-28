# Computer Use (Troupe Comu)

Troupe Comu gives ACP agents inside Troupe structured, per-app-consented local desktop computer use across macOS, Windows, and Linux. Agents discover it in the composer `+` → **Troupe tools** menu under **Computer Use** (`[troupe-tool:comu]`) and call `comu_*` tools through Troupe's built-in `troupe` stdio MCP server.

## Architecture

- **Clean MIT Engine (`troupe-comu-engine`)**: Linked in-process in the desktop host (`src-tauri/Cargo.toml`), pinned via [`comu/troupe-comu.pin`](../comu/troupe-comu.pin) and verified in CI by `scripts/comu/check-comu-pin.mjs`. Derived from the MIT-licensed `cua-driver` Rust crates in [trycua/cua](https://github.com/trycua/cua), stripped of telemetry, CLI installers, cloud perception, UniFFI bindings, and redundant CDP browser tools (Troupe's built-in `browser_*` tools handle web browsing).
- **MCP Bridge (`src-tauri/src/acp/host_mcp/`)**: The stdio child fetches `comu_*` schemas via `FrameKind::DescribeComu` and forwards tool calls via `FrameKind::CallComu` over the local framed TCP loopback channel (`127.0.0.1`, ephemeral token). Screenshots return inline as MCP `ContentBlock::image` (`image/png` or `image/jpeg`) alongside text metadata.
- **Cross-Surface Support**:
  - **Tauri Desktop (`src-tauri/src/lib.rs`)**: Registers `ComuPolicyService` and `DesktopComuHost`.
  - **Desktop Shared-Live Remote (`src-tauri/src/remote/host.rs`)**: Authenticated remote browser/phone clients (`FullControl`) view and edit policy via `/host/comu/*`, answer consent cards over `/ws`, and trigger `Stop` via `/host/comu/stop`; view-only shared-live sessions cannot mutate policy or stop sessions.
  - **Standalone `troupe-server` (`src-tauri/src/server_main.rs`)**: Leaves `ComuHost` unregistered (`None`), fails closed with `capability_unavailable`, and links zero GUI/X11/Wayland dynamic libraries (`scripts/check-server-no-gui-libs.sh`).

## Safety & Consent Model

1. **Hard Self-Target Guard (`src-tauri/src/comu/self_guard.rs`)**: Troupe refuses any `comu_*` call targeting Troupe's own process ID, bundle/AUMID/executable name, window IDs, or screen coordinates inside any Troupe window frame. Troupe also cannot be added to Allowed Apps.
2. **Per-App Consent (`src-tauri/src/comu/consent.rs` + `HostTickets`)**:
   - **Ask before operating an app** (default: **On**): Prompts with **Allow for this chat**, **Always allow in this project**, or **Deny** before driving any app not in the global or project Allowed Apps list.
   - **Ask before foreground or full-desktop actions** (default: **On**): Prompts before raising windows, capturing the full desktop without `pid`, or moving the real OS cursor.
   - **Ask before reading clipboard text** (default: **On**): Prompts once per `comu_clipboard_get` call; clipboard text is never logged.
   - **750 ms Anti-Clickjacking Floor**: Enforced in both `HostTickets` (`ANSWER_FLOOR`) and `ComuConsentCard` (`useConsentArmed`). Unattended runs fail fast instead of hanging.
3. **Session Stop Latch (`src-tauri/src/comu/stop.rs`)**: Clicking **Stop** on the live in-chat `ComuAgentIndicator` latches the session stopped, cancels pending Comu consent tickets, hides the virtual agent cursor, and refuses subsequent `comu_*` calls until the user's next prompt. The indicator itself shows only while a turn drives Computer Use and clears when that turn ends.
4. **Turn Routing (`src-tauri/src/acp/host_mcp/parent/routes.rs`)**: `comu_*` calls are accepted during a prompt turn and while the agent works on its own outside one (Claude Code waking up after a background command; `src-tauri/src/acp/session/unprompted.rs`). Calls outside both fail with `NO_ACTIVE_TURN`.

## Virtual Agent Cursor

When **Show virtual agent cursor** is enabled in **Settings → Agents → Computer Use** (default: **On**), Troupe renders a click-through agent cursor overlay with the active agent's name over target windows during pointer and keyboard actions, hiding it automatically on turn end, **Stop**, or session close. On macOS, the overlay attaches directly to Tauri's existing main-thread `NSApplication` runloop (`attach_to_existing_appkit()`).

## Screen-Edge Indicator

While an agent drives this computer, Troupe draws a blue glow along the edges of every monitor and a label at the top edge naming the agent ("Claude is using this computer", or "2 agents are using this computer"). Turn it off with **Show screen-edge indicator** in **Settings → Agents → Computer Use** (default: **On**). The setting applies from the agent's next action.

- **When it shows**: from an agent's first acting `comu_*` call in a turn (clicks, typing, window changes; read-only probes such as `comu_list_windows` do not count) until that turn or the agent's own background work ends, **Stop** is pressed, or the chat closes. It stays while any chat is still driving.
- **How (`src-tauri/src/comu_screen_overlay.rs`)**: one transparent, click-through, always-on-top window per monitor loads `comu-overlay.html`. The windows never take focus, stay out of the taskbar and window switcher, are excluded from screen capture (`content_protected`, macOS and Windows), and have no capability or IPC. Rust pushes state with `eval`. `DesktopComuHost` only sees the `comu::indicator::ComuScreenIndicator` port, so `troupe-server` links no window code.
- **Self-target guard**: the desktop-coordinate hit-test in `self_guard.rs` skips the indicator windows (by native window id), so an agent can still click anywhere on the desktop. Targeting them by window id or pid stays blocked.
- **Limits**: not shown on Linux Wayland sessions (no window positioning). Monitors added while the indicator is up get it on its next show. macOS needs Tauri's `macOSPrivateApi` for the transparent window.

## OS Permissions

- **macOS**: Requires **Accessibility** (window inspection & synthetic input) and **Screen Recording** (window/display screenshots). **Settings → Agents → Computer Use** displays live grant status with **Grant Accessibility**, **Grant Screen Recording**, and **Recheck** controls.
- **Windows**: Uses UI Automation, Win32 input, and DXGI/GDI capture without separate TCC prompts (subject to UIPI for elevated windows).
- **Linux**: Supports X11 (`x11rb` + XTest) and Wayland (`wlr-screencopy`, XDG Desktop Portal, virtual pointer/keyboard protocols) when a graphical display (`DISPLAY` or `WAYLAND_DISPLAY`) is present.

## Tool Surface (`troupe` MCP Server)

Call `comu_guide` first for workflow guidance (`comu_list_apps` → `comu_accessibility_tree` / `comu_find_elements` → background `pid`-targeted `comu_click` / `comu_type_text` → `comu_screenshot(pid=...)` verification).

| Category | Tools |
| --- | --- |
| Guide | `comu_guide` |
| Discovery & Windows | `comu_capabilities`, `comu_displays`, `comu_list_apps`, `comu_list_windows`, `comu_open_app`, `comu_focus_window`, `comu_manage_window` |
| Accessibility | `comu_accessibility_tree`, `comu_find_elements`, `comu_do_element_action`, `comu_menu_bar`, `comu_menu_click` |
| Screen Capture | `comu_screenshot` (inline `ContentBlock::image`) |
| Pointer & Keyboard | `comu_click`, `comu_double_click`, `comu_right_click`, `comu_middle_click`, `comu_triple_click`, `comu_move_cursor`, `comu_mouse_down`, `comu_mouse_up`, `comu_drag`, `comu_scroll`, `comu_type_text`, `comu_key_tap`, `comu_key_down`, `comu_key_up`, `comu_hotkey` |
| Clipboard | `comu_clipboard_get`, `comu_clipboard_set` |

## License & Attribution

Troupe Comu's native driver engine adapts the MIT-licensed `cua-driver` crates from [trycua/cua](https://github.com/trycua/cua) (Copyright (c) 2025 TryCua). See [`comu/licenses/MIT-cua.txt`](../comu/licenses/MIT-cua.txt) and [`comu/licenses/THIRD_PARTY_NOTICES-cua-driver.md`](../comu/licenses/THIRD_PARTY_NOTICES-cua-driver.md).
