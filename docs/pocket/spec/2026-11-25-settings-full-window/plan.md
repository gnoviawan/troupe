# Settings full window: implementation plan

Status: plan, not started. Date: 2026-11-25. Branch: `feat/settings-full-window` from `dev`.
Design source: Paper file "Termul Design System", page "Troupe - Preference/Settings", boards 1.1–1.11b (dark, desktop, 1440×900).
Earlier spec that this replaces: `docs/preferences-redesign-handoff.md` (PR #1380).

## 1. Decisions

| Topic | Decision |
|---|---|
| Shell | Full-window overlay over the workspace. The workspace stays mounted. "← Back to app" and Esc close it. |
| Project settings | Moves into the same full-window shell. Same rail and card recipe. No new boards. The Save bar stays. |
| Delivery | One PR. The 50-file rule in `AGENTS.md` is waived for this PR; the PR body says so. Many small commits, one per step below. |
| Narrow (< 768px) and phone | Keep the current behaviour: the rail becomes the top strip with a scrolling page list. Phone rules win (15px text, 44px targets, 16px fields). |
| Light theme | Tokens only. One check pass at the end. No light boards. |
| MCP logos | Registry icon first (`mcpCatalogApi.icon`), then a small bundled brand map, then a letter tile. |
| Links | One app-wide "Open links in" setting for terminal, chat, Markdown and file links. The "Open this link?" confirm stays for every link. |
| Update toasts | Stay. The toasts and the About version card read the same updater store. |
| Column | Every page uses one 640px column. 32px between sections, 12px between a section title and its card. |
| Saving | App settings save on change (no Save button). Project settings keeps its Save bar. |

## 2. Navigation (10 pages)

| Group | Page id | Label | Holds (old pages) | Board |
|---|---|---|---|---|
| — | `preferences` | Preferences | Interface, Notifications, Editor, the terminal "Links" group (now app-wide) | 1.1 |
| Terminal | `terminal` | Terminal | Appearance, Shell | 1.5 |
| Terminal | `terminal-profiles` | Profiles | Profiles | 1.6, 1.6b |
| Agents | `provider` | Accounts | Accounts | 1.2, 1.3 |
| Agents | `ai-agents` | Agents | Agent chats | 1.4, 1.3 |
| Agents | `skills` | Skills | Skills | 1.8, 1.3 |
| Agents | `mcp-servers` | MCP servers | MCP servers (grid + detail sub-page) | 1.9b, 1.9d, 1.9e |
| Agents | `browser` | Browser | Browser | 1.7 |
| App | `shortcuts` | Keyboard shortcuts | Keyboard shortcuts | 1.10, 1.10b |
| App | `about` | About | Updates, Server (web), Logs, Reset | 1.11, 1.11b |

- Keep the ids `provider`, `mcp-servers`, `ai-agents`, `skills`, `browser`, `shortcuts` and `terminal-profiles`. Deep links (`openApp('provider')`, `openApp('mcp-servers')`) keep working.
- Add an alias map in `stores/settings-modal-store.ts` so old ids still open the right page: `interface`, `notifications`, `editor` → `preferences`; `appearance`, `shell` → `terminal`; `updates`, `server`, `diagnostics`, `reset` → `about`.
- Rail icons (Hugeicons through `components/icons/index.tsx`): `SlidersHorizontal`, `Terminal`, a new `Bookmark` export (`Bookmark02Icon`) for Profiles, `KeyRound`, `Bot`, `Sparkles`, `Network`, `Globe`, `Keyboard`, `Info`. Back = `ArrowLeft`. Search = `Search`.

## 3. Work order (commits inside the one PR)

Each step ends with `bun run typecheck`, `bun run lint` and the tests of the touched files.

### Step 1. Page ids, aliases and search index
- `pages/app-preferences/categories.tsx`: replace `ALL_APP_PREF_CATEGORIES` with the 10 pages above. Move every search entry to its new `categoryId`. Keep every `anchorId`. Keep the Tauri/web filter: drop the web-only Server entries on desktop and the Updates entries on web, but both now live on `about`.
- Split `categories.tsx` (505 lines) into `categories.tsx` (pages) and `search-index.ts` (entries) so each file stays small.
- `stores/settings-modal-store.ts`: add `APP_PAGE_ALIASES` and resolve it in `openApp(page)` and `setAppPage`.
- Tests: `categories.test.ts` (Screen reader mode entry moves to `terminal`), `categories.server.test.ts` (Server entry moves to `about`), new alias test in `settings-modal-store.test.ts`.

### Step 2. Full-window shell
- New `components/settings/SettingsWindow.tsx` replaces `SettingsModal.tsx` for both surfaces.
  - `fixed inset-0 z-50 bg-background`, no scrim, `role="dialog" aria-modal aria-label={title}`.
  - Enter: opacity 0 → 1 and `translateY(8px)` → 0, 200ms, curve `cubic-bezier(.23,1,.32,1)`. Exit 150ms. Reduced motion: crossfade only.
  - Keep the focus-in, focus-return and Tab trap code from `SettingsModal` (move, do not rewrite). Keep the `[data-sibling-dialog]` Escape defer.
  - Fix the Escape bug: today any mounted `[data-shortcut-recorder]` blocks Escape, and the Shortcuts page is always mounted, so Esc never closes the window. Block Escape only while a recorder is recording (`[data-shortcut-recorder][data-recording="true"]`), and set that attribute in `ShortcutRecorder.tsx`.
  - Native browser webviews draw above the DOM. Register `NativeWebviewOccluder` (`hooks/use-native-webview-occlusion.ts`) while the window is open, as `ui/dialog.tsx` does.
  - Keep `useOverlayRegistration` / the back-stack id `SETTINGS_MODAL_OVERLAY_ID` so phone Back still closes it.
- `components/settings/SettingsLayout.tsx`:
  - Rail 240px on `bg-sidebar-background`: macOS title-bar inset (44px, drag region), "← Back to app" row, "Search settings…" field, groups with labels, rows 28px with 14px icons. Chosen row = 10% foreground fill; hover = 4%.
  - Content: a centred 640px column. Page header = title 20/28 + description 14/20 muted. No Close × on desktop (Back to app replaces it). Keep the narrow top strip and its Close button below `md`.
  - Keep search, jump-to-row (`openResult`), arrow-key focus and `data-settings-section`. The jump mark stays (2px bar + 4% wash for 2s).
- `pages/AppPreferences.tsx` and `pages/ProjectSettings.tsx`: wrap in `SettingsWindow`. Project settings: rail title "Project settings", subtitle = project name, Back label "Back to app". The Save bar ("Unsaved changes" · Discard · Save changes) is pinned at the bottom of the content area, full column width. The unsaved-changes confirm runs on Back, Esc and phone Back.
- `layouts/workspace-layout/WorkspaceModals.tsx`: lazy mounts stay. Rename the mounted components only if the file names change.
- Docs: `docs/design/overlays.md` shells table (SettingsWindow replaces SettingsModal).
- Tests: move `SettingsModal.test.tsx` to `SettingsWindow.test.tsx` (focus, Tab trap, Esc, Esc while recording). Update `SettingsLayout.test.tsx` (no Close on desktop, Back to app).

### Step 3. Shared pieces
- `components/ui/segmented.tsx`: wraps `SEGMENTED_TRACK_CLASS` + `segmentClass` as a radio group (Radix `react-toggle-group` is already a dependency). Heights: 28px in a row, 32px in a toolbar. Used by Skills scope and About release channel.
- `components/settings/SettingsDisclosure.tsx`: one card titled "Advanced" with a summary line, built on `ui/collapsible`. Used by Terminal and Agents.
- `components/settings/SettingsFilter.tsx`: the 32px filter well (search icon, clear ×, `aria-label`). Used by Agents, Skills, Shortcuts, MCP and MCP tools.
- `components/ui/progress-bar.tsx`: 4px track, `role="progressbar"`, determinate and indeterminate (moving sliver, static when reduced motion). Move `UsageBar` to use it.
- `components/settings/SettingsSection` title: 14/20 medium, actions at the right of the title (`SettingsGroup` already has `actions`; restyle only).
- Card recipe in `SettingsCard`: `bg-card`, 0.5px outline (`#38393A` becomes a token, `--color-raised-outline`), black 1px drop, radius 8. Rows 12/16, padding 12px 16px, 0.5px dividers.

### Step 4. Pages (one commit per page, in this order)
1. **Preferences** (`preferences-section.tsx`, new; merges `interface-section`, `notifications-section`, `editor-section`).
   - Sections: Appearance (zoom, translucent sidebar on macOS, theme row that opens the theme picker) · Notifications (3 switches) · Links (Open links in, see Step 5) · Editor (one "Auto save" select: Off, 0.5 s, 1 s, 2 s, 5 s; maps to `editorAutoSave` + `editorAutoSaveDelayMs`, no second row) · New projects (default project color).
2. **Terminal** (`terminal-section.tsx`, merges `appearance-section` + `shell-section`).
   - Text (font size stepper + live preview in the same card) · New terminals (Default shell, Screen reader mode) · Advanced disclosure (Renderer, Scrollback, Automatic completion refresh). Option labels sentence case ("Auto", "System default").
   - Keep the log source `AppPreferences.terminalScreenReaderMode` (an e2e spec asserts it).
3. **Profiles** (`terminal-profiles-section.tsx`): header "N profiles" + Add profile; rows = name + one meta line (mono command, folder, "Default shell") with a 24px fade; ghost Edit and Delete icon buttons. Dialog 512px per board 1.6b ("Save profile").
4. **Accounts** (`components/settings/provider/*`): one section per provider with the header from board 1.2; `BAR_FILL.normal` becomes `bg-primary-fill` (also changes Project settings → Accounts, which is wanted). Amber at 80%, red at 100% stay.
5. **Agents** (`ai-agents-section.tsx`, `AcpAgentsSettings.tsx`, `subagent-settings-group.tsx`): Available agents first (filter, rows with version, Latest badge, status dot; "Update to X" only), Sub-agents (switch + indented steppers), Agents they can use, Chats, Advanced disclosure (time limits + idle chats). Split `AcpAgentsSettings.tsx` (543 lines) into `AgentRow.tsx` + the list.
6. **Skills** (`skills-section.tsx`): filter + segmented All / Global / Project with counts; Project section first; Global shows 6 rows then "Show N more" under the card. The skill name is the only link (opens SKILL.md in the editor, then closes Settings; show a toast on failure). Ghost "Show in Finder" button (label per OS: Finder / File Explorer / file manager) calls `openerApi.revealInFileManager`; its tooltip shows the path. Hidden on web (`isTauriContext()`).
7. **MCP servers** (`mcp-servers-section.tsx`, `McpServersSettings.tsx`, `McpCatalogBrowser.tsx`):
   - Grid page: toolbar (segmented All / Added / Catalog, search, Add server), "Added" and "From the catalog" sections of 2-column cards (314px): logo 32 · name · switch / + / ✓ / Deprecated badge; 2-line description (custom server: command or URL in mono); bottom line = status dot + word + one optional action (Connect, Try again) or "by X" or the install reason. "Load more" under the catalog card.
   - Detail sub-page (`McpServerDetail.tsx`, new): opened by a click on an added card; state `mcpDetailId` in the settings store so Back and search work. "← MCP servers" link, header (logo 40, name 20/28, status line, switch), description + "by X · registry name", Connection (Test connection + Edit at the right of the title; Transport, URL or Command, Sign-in + Disconnect), Tools (count, filter, 5 rows, "Show all N"), Remove card (red text; delete + the existing undo toast). One notice under the Connection title for a problem (Connect, or the mono probe error + Try again).
   - Catalog detail: Install in the switch slot, About card (Version, Runs with, Source).
   - The secrets notice moves into `McpServerForm.tsx`, next to the env and header fields.
   - Keep the virtualised catalog list (`@tanstack/react-virtual`); the estimate changes to the card height.
   - Split `McpServersSettings.tsx` and `McpCatalogBrowser.tsx` into card, grid and detail files; both tests (964 and 879 lines) move with them.
8. **Browser** (`BrowserPolicySettings.tsx`): Ask first (2 switches with ON-phrased copy) + Pre-approved actions card · Logins (Keep logins between chats; "Logins and site data" with the red-text "Clear agent profile" trigger; the confirm keeps the red button). Remove the "Changes on this page save as you make them." footnote. `BrowserProjectPolicy` (Project settings) gets the same card recipe.
9. **Keyboard shortcuts** (`shortcuts-section.tsx`, `ShortcutRecorder.tsx`): filter + "Reset all"; rows 40px with the name only (the filter still matches the description); keycaps right; changed row = "Default ⌘B" + reset icon. States per board 1.10b (hover on the keys box only, focus ring, recording ring, conflict notice + "Use anyway").
10. **About** (`about-section.tsx`, new; uses `updates-section`, `diagnostics-section`, `server-section`, `reset-section` as parts):
    - Desktop: Version card (Step 7) · Updates (segmented Stable / Insider / Nightly, description follows the channel; Check for updates automatically) · Logs and bug reports (Report a bug → "Report"; Application log → Copy, Export ⌄ [Save as…, Save to Downloads], Show in Finder) · Reset card (red-text "Reset settings"; the existing confirm).
    - Web: Server section (the existing `ServerSetupWizard` card and its warning) · Server version row · Logs (Copy, Report; the "logs live on the server host" notice) · Reset. No Updates section.
    - AUR installs: no channel control (as today).

### Step 5. App-wide "Open links in"
- `types/settings.ts`: new `linkOpenMode: 'system' | 'troupe'`, default `'system'`. Option labels "Default browser" and "Troupe browser".
- `hooks/use-app-settings.ts` `normalizeStoredSettings`: copy `terminalUrlOpenMode` into `linkOpenMode` when the new key is missing (`'termul'` → `'troupe'`), then drop the old key (add it to `RETIRED_SETTING_KEYS`).
- New `lib/browser/open-link.ts` `openLink(url)`: reads the setting; `'troupe'` on desktop uses `openUrlInTroupeBrowser`; else `openerApi.openUrlWithSystemBrowser`. On web the Troupe option is hidden and the row shows the system browser only.
- Callers: `terminal-url-navigation.ts` (terminal ⌘-click), `ExternalLinkConfirm.tsx` `settle()` after the user selects Open (chat, link chips, Markdown editor, GitHub views, What's new). The confirm stays for every link.
- Row copy: "For links in chats, terminals, Markdown and files. In a terminal, ⌘‑click a link." (Ctrl on Windows and Linux.)
- Tests: migration in `use-app-settings.test.ts`, `open-link.test.ts`, the existing `ExternalLinkConfirm` tests.

### Step 6. Key display on macOS
- `stores/keyboard-shortcuts-store.ts` `formatKeyForDisplay` (display only; stored keys and matching do not change):
  - On macOS a single `ctrl` shows ⌘, because ⌘ triggers it and Ctrl alone never does. `ctrl+cmd` shows ⌃⌘.
  - Apple order ⌃ ⌥ ⇧ ⌘, then the key.
  - Named keys: ↑ ↓ ← →, Delete, Backspace, Home, End, PageUp, PageDown, Tab, Esc, Space, F1–F12.
- New helper `formatKeyParts(key)` returns the parts in display order, for keycap rows (`ShortcutRecorder`).
- This also changes the rail tooltips and the New tab menu (`useShortcutLabel`, 21 call sites). That is wanted: one rule everywhere. Check the command palette path (`getShortcutLabel` returns raw keys) and route it through the same formatter.
- Tests: `keyboard-shortcuts-store.test.ts` (mac and non-mac cases), snapshot of a rail tooltip.

### Step 7. Update flow in the version card
- New `components/settings/about/VersionCard.tsx` reads `useUpdaterState` and `useUpdaterActions`.
- One action slot (min width 144px): Check for updates · Checking… (disabled, spinner) · Try again · Download (primary) · Preparing… / Downloading… (disabled) · Install and restart (primary, through `confirmInstallAndRestart`) · Open download page (manual mode) · none (AUR).
- One detail band under the row: release notes (first 100 characters) + "Skip this version" (wires `skipVersion`, which has no UI caller today), the backup line + indeterminate bar, the progress bar, the restart note, the red error band with the mono raw error, the AUR command chip, the skipped-version note.
- `lib/confirm-install-update.ts`: OK label "Install and restart" (sentence case, same as the button).
- The toasts in `UpdateAvailableToast.tsx` stay. Their labels follow the same words.
- Tests: extend `updates-section.test.tsx` into `VersionCard.test.tsx` (every state in board 1.11b).

### Step 8. MCP logos
- `components/settings/mcp/McpLogo.tsx`: order = registry icon (`McpCatalogIcon` data URL, when `hasIcon`) → bundled brand map → letter tile.
- `lib/mcp-brand-logos.ts`: a short map keyed by registry name prefix (`io.github.github/`, `com.linear/`, `com.notion/`, `io.sentry/`, `com.stripe/`, `com.supabase/`, `com.microsoft/playwright`, `io.github.upstash/context7`). Path data copied from simple-icons (CC0) and the Playwright and Context7 repos; no new dependency (`react-icons` is only transitive today). Black brand marks use `currentColor` (foreground); coloured brands keep their colour.
- Installed servers keep `catalog.name`, so their cards use the same lookup.

### Step 9. Tests, e2e and docs
- Update stale e2e specs that still look for "Application Preferences", "Settings categories" and old page names: `mobile-text-field-zoom.spec.ts`, `mobile-overlay-back-stack.spec.ts`, `mobile-mobile-back-stack-tail.spec.ts`, `mobile-mobile-terminal-screen-reader-mode.spec.ts`, `mobile-drawer-home.spec.ts`. Also `WorkspaceLayout.mobile.test.tsx` and `MobileChatShell.settings-handoff.test.tsx` (`SETTINGS_TITLE`).
- `AppPreferences.test.tsx` (556 lines) and `ProjectSettings.test.tsx` (618 lines): new page names, Back to app, Save bar position.
- Docs: replace `docs/preferences-redesign-handoff.md` with a short pointer to this plan and a new hand-off checklist; update `docs/design/forms.md` (row anatomy, Advanced disclosure, segmented) and `overlays.md`.

## 4. Files that grow past the 800-line ceiling (split first)

`categories.tsx` (505), `AcpAgentsSettings.tsx` (543), `ProjectSettings.tsx` (559), `McpServersSettings.tsx` + its test, `McpCatalogBrowser.tsx` + its test. Splits are move-only commits before the restyle commit.

## 5. Verification

1. `bun run typecheck`, `bun run lint`, `bun run test` (all), `bun run test:e2e` for the specs in Step 9.
2. Manual check in the desktop app (macOS): every page, every state on boards 1.3, 1.9e, 1.10b, 1.11b; search jump; deep links from the composer (`mcp-servers`) and account chips (`provider`); Esc and Back to app; a native browser tab open under Settings (occlusion).
3. Browser client and phone width: top strip, Back, 44px targets, 16px fields, Server section on About, no Troupe-browser link option, no Show in Finder.
4. Light theme pass on every page.
5. Project settings: dirty form + Back to app → "Unsaved changes" confirm.

I do not run the app or a browser check without your OK.

## 6. Risks

- **Size of one PR.** About 80 files. Mitigation: one commit per step, move-only splits first, a checklist in the PR body.
- **Native webviews.** A full-window surface must hide them, or a browser tab draws over Settings.
- **Link setting rename.** An older client on the same `settings/app` key would not see `linkOpenMode`. Desktop and server ship together, so the risk is low; the migration keeps the old value.
- **Key display.** Changes glyphs in tooltips across the app. Only display changes; matching does not.
- **Escape fix.** Today Esc never closes Settings. After the fix it closes from every page, so check that Esc inside an open select, menu or confirm still closes only that layer.

## 7. Defaults I chose (tell me to change any)

- Page ids: new `preferences`, `terminal`, `about`; the others keep their ids.
- Bundled logos in a local file, not a new `react-icons` dependency.
- `Bookmark` icon for Profiles (the Terminal icon and the old Profiles icon look the same).
- Project settings rail: "Back to app" (not "Back to project").
- The Save bar in Project settings sits at the bottom of the content column, not full window width.
