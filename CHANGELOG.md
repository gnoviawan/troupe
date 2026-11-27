# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

## [0.6.0] - 2026-11-26

### Highlights

- **Live HTML preview.** Open an HTML file in the editor and switch between Preview, Split and Source. (#2134)
- **Settings in a full window.** Settings now open as a full window with 11 pages. (#2106, thanks @julianromli)
- **Sub-agent profiles.** Give each sub-agent its own model, mode, thinking level and approvals, and manage them on their own page. (#2084, #2087, #2100)
- **Browser redesign.** The built-in browser has a new toolbar, zoom, responsive viewports and annotations. You can also open chat links in it from the context menu. (#2092, #2145, thanks @julianromli; #1980, thanks @dickyudhandika)
- **More agents and accounts.** Adds Hermes Agent. Agent logos and extra agents now come from the Troupe registry. Antigravity supports multiple accounts and shows your plan tier. Automatic account switching is now opt-in and shows a risk warning. (#2017, #2032, thanks @julianromli; #2058, #2083, #2079, #2049)
- **Quick chat.** A new quick chat opens the agent launcher and starts the agent only when you press Send. (#2052)
- **Canvas mentions.** Mention canvas layers from the composer's @ menu. (#1979, thanks @dickyudhandika)
- **troupe-server QR codes.** The CLI and the Web UI show a QR code so you can connect from your phone. (#2137, thanks @julianromli)

### Release channels

Troupe now has two channels. Stable releases are `X.Y.0`. Nightly builds are `X.Y.N`, where N counts the commits since the last stable release. The Insider channel is gone, and installs that used it move to Stable. (#2054, #2056)

### Fixes

- MCP OAuth sign-in now works on desktop, on the web and from chat installs. (#2121, thanks @julianromli)
- Antigravity installs on Linux, lists models for every project and no longer reads quota through Google's Cloud Code API. (#2055, #2146, #2078)
- `$VAR` references in project environment variables now expand. (#2010, thanks @DarulQutni-Q)
- Markdown images load from local paths and inside table cells. (#1978)
- Relative paths stay relative. (#2133, thanks @DarulQutni-Q)
- The web file stream no longer crashes on a late poll. (#2131, thanks @DarulQutni-Q)

### Contributors

Thanks to @julianromli (#2017, #2032, #2092, #2106, #2117, #2121, #2137, #2145), @dickyudhandika (#1979, #1980), @DarulQutni-Q (#2010, #2131, #2133) and @gnoviawan for their work in this release.

**Full changelog:** https://github.com/gnoviawan/troupe/compare/v0.5.0...v0.6.0

## [0.5.0] - 2026-11-23

Termul Manager is now Troupe. This release renames the app and adds per-chat accounts, sub-agents across agents, a new sidebar and composer, quick chats, and a command editor in the terminal.

### Renamed to Troupe

- **Same app, new name.** The app is now called Troupe. The site is https://usetroupe.dev, with the docs at https://usetroupe.dev/docs, and old termul.dev links redirect there. The GitHub repo is now `gnoviawan/troupe`, and old `gnoviawan/termul` links redirect. Your settings, projects, chats and logins carry over, because the app keeps its data folder (`com.termul-manager.app`). After you update from Termul, a notice tells you once that the app is now Troupe. (#1764, #1770, #1771, #1772, #1773, #1774, #1775, #1776, #1777, #1784, #1804, #1852, #1863, #1927)
- **Windows.** The Troupe installer removes the old Termul Manager install first and keeps your app data. If you had Start menu or desktop shortcuts to Termul Manager, it creates them again for Troupe. A taskbar pin to Termul Manager stops working, so pin Troupe again. (#1764)
- **macOS.** The disk image, the install script and Homebrew install **Troupe.app**. The install script removes **Termul Manager.app**. If you install from the disk image and **Termul Manager.app** is still in Applications next to **Troupe.app**, move **Termul Manager.app** to the Trash. An update from inside the app keeps the old path, `/Applications/Termul Manager.app`. In that case that app is your updated Troupe, so keep it. (#1764)
- **Linux.** The `.deb` and `.rpm` packages are named `troupe` and replace `termul-manager`. `/usr/bin/termul-manager` stays as a link that starts Troupe. The install script adds the `troupe` command and keeps `termul-manager` and `termul-manager.AppImage` as links to Troupe. An AppImage that updates itself keeps its old file name, so run the install script again to get the `troupe` command. (#1764, #1780)
- **Install script.** The docs now pin the install script to v0.5.0. The old command pinned to v0.4.60 still works. It installs the latest release under the old names (**Termul Manager.app** on macOS, `termul-manager` on Linux). (#1787)
- **Homebrew.** The cask is now `troupe`, and `brew update` moves the `termul` cask to `troupe`. The tap is still `gnoviawan/termul`. If Homebrew 7 refuses to load the cask from an untrusted tap, run `brew trust gnoviawan/termul`. (#1780)
- **AUR.** There is no `troupe` package in the AUR for this release yet. Keep using `termul-manager`, which keeps getting new versions during 0.5.x. The update notice in the app and the install docs already say `yay -S troupe`. Until that package exists, update with `yay -S termul-manager`. (#1780, #1789)
- **troupe-server.** The standalone server is now `troupe-server`: the binary, the release assets and the `ghcr.io/gnoviawan/troupe-server` image. Existing `termul-server` installs keep working. Releases still publish the `termul-server` assets and the `ghcr.io/gnoviawan/termul-server` image. A `termul-server` with self-update on updates itself to 0.5.0 and keeps its file name. `troupe-server onboard` rewrites an existing `termul-server.service` instead of adding a second unit. (#1778, #1779)
- **Environment variables.** Every `TERMUL_*` setting now also reads as `TROUPE_*`. The `TERMUL_*` names still work but are deprecated, and the app logs one line for each old name it reads. If you set both, the `TROUPE_*` name wins unless it is blank. (#1766)
- **Agent tools.** Agents see Troupe's own tools under the MCP server name `troupe`, for example `mcp__troupe__plan`. If you told Claude Code not to ask again for a Termul tool, it asks once more for each tool in each project. Chats from 0.4.x keep their tool pills. (#1765)
- **Bug reports from 0.4.60.** Since the repo rename, the in-app bug report in v0.4.60 can say "gh created the issue but did not print its URL" even though it filed the issue. Check the repo's issues before you file again. Troupe 0.5.0 accepts issue links under both repo names. (#1792)

### Highlights

- **Each chat can run on its own Claude or Codex account.** The account chip in a chat now sets the account for that chat only, and **Follow project** puts it back on the project's account. When a Claude chat reaches its 5-hour or weekly limit, the host moves only that chat to the signed-in account with the most room, even when no app or browser is open. The chat gets a short notice to continue, so the agent does not answer your last prompt twice. If no account has room, the host continues the chat when the limit resets, also after a restart. Codex now has accounts too. Add more than one ChatGPT login in Settings > Provider, pick one per project or per chat, and a chat at its limit moves to the Codex account with the most room. An agent can also start worker chats with the `start_chat` tool. Each worker starts on the Claude account with the most room and shows in the chat list right away, with the chat that started it. (#1842, #1886, #1893, #1894, #1898, #1902, #1919, thanks @julianromli)
- **Agents can hand work to your other agents.** A chat's agent can start your other installed agents, such as Codex or Gemini CLI, as sub-agents. By default each one works in its own git worktree, on a copy of the chat's current state, so its edits don't touch your files. Its progress shows in the sub-agent panel, its approval requests show in the parent chat, and you can stop it. When it finishes, the panel shows its branch and a diff stat. Settings > Agent chats has a **Sub-agents** group: an on/off switch, how many run at once per chat, how deep they can nest, which agents are allowed, and a default model and a **When to use** note for each agent. Agents use this for long or parallel work without being told, and **Orchestrate** in the composer's + menu asks for it directly. The sub-agent panel now reads like the main chat, with text and folded tool calls in order. (#1911, #1924, #1928, #1929, #1930, #1939, #1940, #1942, #1945)

  ![A chat with two sub-agents and the sub-agent panel open](https://raw.githubusercontent.com/gnoviawan/troupe/release-assets/v0.5.0/subagents.png)

- **A new sidebar and composer.** The sidebar has New chat, Search, Canvas and Automations at the top, and each project lists its chats under its own row. A chat row shows one status (needs you, unread, crashed, or a spinner), a short time, and a stack of agent icons when the chat used more than one agent. A tool strip on the right opens Explorer, Changes and GitHub in a right panel, and opens Terminal and Browser tabs. The status bar and the Side panel tabs are gone. Chat search is in the command palette, the worktree switch is in the project row menu, and **Import chats** is in the Projects + menu. On desktop, the composer has a foot row that shows the worktree and the MCP servers that are on. Errors, usage limits and the prompt queue show at the composer, and subagents, background commands and changed files sit in a small status bar at the top right of the chat, each with its own panel. On macOS you can turn on a translucent sidebar, and the window buttons hide when the sidebar is collapsed. (#1840, #1856, #1859, #1864, #1868, #1885, #1888, #1908, #1933, thanks @julianromli)

  ![The new sidebar with projects and their chats, the composer and the tool strip](https://raw.githubusercontent.com/gnoviawan/troupe/release-assets/v0.5.0/new-look.png)

- **Quick chats without a project.** Start a chat that isn't about any project with the + next to **Quick chats** in the sidebar, **New quick chat** in the command palette or the phone menu, or **New chat** when no project is open. Troupe makes an empty folder for each quick chat in `~/.troupe/quick-chats` on the computer that runs Troupe, and the agent works there. A quick chat opens on its own, in place of your project's tabs. Deleting it also deletes its folder. (#1857, #1875, thanks @julianromli)

  ![A quick chat open from the Quick chats section of the sidebar](https://raw.githubusercontent.com/gnoviawan/troupe/release-assets/v0.5.0/quick-chats.png)

- **Install an MCP server from chat.** Give the agent a docs link, a repo link, a registry name or an `mcpServers` JSON snippet and ask it to install the server. Troupe shows a card with the exact command or URL, environment variables and headers. Nothing is saved or run until you click **Install**. Secret values you type on the card are never passed to the agent. `troupe-server` now keeps one MCP server list for all projects, the same as the desktop app. (#1794, #1862, thanks @julianromli)

  ![The MCP server install card in a chat, with Decline and Install buttons](https://raw.githubusercontent.com/gnoviawan/troupe/release-assets/v0.5.0/mcp-install-card.png)

- **First-run setup, and agent setup in the model picker.** A new install with no projects opens a short setup: pick Dark or Light, check your agents, and choose a folder for agents to work in. It ends in a new chat with your agent picked. If you already have projects, you don't see it. The model picker's tabs now show only the agents you set up. The others are under **···** in **Needs setup** and **Install**, and picking one sets it up in the picker, on desktop as on the phone. New chat shows the same tabs as an open chat. (#1946, #1948, #1950, #1971)

  ![The first-run Check your agents step](https://raw.githubusercontent.com/gnoviawan/troupe/release-assets/v0.5.0/onboarding.png)

  ![The model picker with set-up agents as tabs and the Needs setup list open](https://raw.githubusercontent.com/gnoviawan/troupe/release-assets/v0.5.0/model-picker.png)

- **A command editor in the terminal.** In local Bash and Zsh terminals on macOS and Linux, you type commands in an editor at the bottom of the terminal, in the desktop app and in the browser. `Tab` completes command names and paths with spaces. The editor steps aside for full-screen programs such as vim and less, and comes back when they exit. Each finished command gets a green or red dot. Point at a dot to see the exit code. Windows, fish and PowerShell keep the normal terminal. (#1758, #1891, #1892, #1961, #1969, thanks @rfxlamia and @julianromli)

  ![A Bash terminal with green and red dots beside finished commands and the command editor at the bottom](https://raw.githubusercontent.com/gnoviawan/troupe/release-assets/v0.5.0/terminal-editor.png)

- **Edit automations on a canvas.** An automation now opens on a canvas on desktop, browser and phone. You can move, add and connect steps, and edit a step or the trigger in an inspector, with undo and redo. Your edits stay in a draft until you click **Save**, and a live automation keeps running its approved version while you edit. Opening a run shows it on the canvas with each step's status, updated live. A new approval step pauses a run until someone approves or rejects it in the Automations tab, with a timeout from 1 minute to 7 days. Agent steps can use the project's MCP servers, limited per step. (#1691, #1819, #1829, #1834, #1835, #1836, #1838, #1874, #1897)

  ![An automation open on the canvas editor with the step inspector](https://raw.githubusercontent.com/gnoviawan/troupe/release-assets/v0.5.0/automation-canvas.png)

- **Phone, browser and desktop sharing.** Desktop sharing has an **Allow full control** switch, off by default. Turn it on before you start sharing, and the other device can prompt agents, run commands and edit files. On a phone, every tap target is at least 44px, and sheets and headers follow touch patterns. After a dropped connection, the browser version clears "Reconnecting…" once the chats on screen are back and reconnects the rest in the background. (#1901, #1914, #1951, thanks @julianromli)
- **Smaller touches.** Links in sent messages and agent replies show as the site's icon and the page title (#1973, thanks @julianromli). A chat shows commands its agent left running in the background, the idle reaper and agent or account switches no longer stop them, and a reload warns first (#1753). Right-click a chat and choose **Copy chat reference**, and another chat's agent can read that chat with the `read_chat` tool (#1796). A chat that switched agents is one row in your history (#1797). You can import chats that you started with an agent outside Troupe (#1716, thanks @julianromli). Agent browser tabs show a pixel overlay with a clear circle around your pointer. Agents open new sites without asking unless you turn on **Ask before opening new sites**, and they can open HTML files in the project (#1793). The model picker lists one OpenCode agent, and `opencode-v2` installs keep working (#1918, thanks @julianromli).

### Fixes

**Agent chats**
- **New chat** opens the composer at once and starts the agent when you send, so you can pick a model without waiting and a draft is no longer lost. (#1943, #1944)
- A chat keeps its reading position and doesn't show blank areas after you minimize and restore the window. (#1947)
- A plan shows once, in the chat header, and a new plan replaces the old one. (#1858, thanks @julianromli)
- In a chat with no messages, picking another agent's model moves the chat to that agent without a handoff. (#1884, thanks @julianromli)
- Retrying a failed Fix CI or Resolve comments launch sends the hand-off instructions again. (#1746)
- A chat started on another device shows its real title instead of "Untitled Chat". (#1805, #1809)
- A chat opened from the agent's own session list gets its MCP servers and Troupe's tools. (#1818)
- Error replies no longer carry raw provider data, such as request headers. (#1827)
- A browser or phone no longer gets an unsafe link from an agent's request, such as a `javascript:` or plain http link. (#1826)
- A damaged npx cache entry gets the steps to repair the cache instead of "Install Node.js". (#1814)

**MCP**
- A dropped connection or two chats refreshing at once no longer signs you out of an MCP server. Only a refused refresh does. (#1821)
- MCP sign-in works from the browser version. On another device, you paste the address the sign-in ends on into a dialog. (#1811)
- A server with an `Authorization` header no longer also gets the stored OAuth token in checks and automation calls. (#1738)
- The MCP catalog's search box no longer jumps down while the source card loads. (#1807, #1853)

**Automations**
- MCP tool steps send typed values, such as `true` and `3`, in the type the tool expects instead of as text. (#1837)
- A step that names an MCP server that `troupe-server` merged into another one keeps working. (#1830)

**Phone and web**
- With a hardware keyboard on a phone, Enter no longer deletes or restores a snapshot, and the Search key in New Worktree no longer creates the worktree. Select text mode clears when you leave a chat. (#1910, thanks @julianromli)
- Project color swatches in Settings have 44px tap targets on phones. (#1968)

**Desktop sharing**
- On Windows, desktop sharing starts again after the first time instead of rejecting its own token file. (#1913)
- On macOS, desktop sharing finds `cloudflared` from Homebrew when Troupe starts from the Dock or Finder. (#1896, thanks @julianromli)
- Long errors wrap inside the remote access popover. (#1817)

**Browser and canvas**
- File uploads in the agent browser on macOS and Linux finish on pages that clear the file input after a change. (#1769)
- Canvas tools ignore a `document` argument, so agents no longer get "no open document". (#1824)

**troupe-server**
- The server no longer warns at start that `dist-web/index.html` is missing when the web app is built in. (#1806)
- A conditional `HEAD` request gets a 304 without a `Content-Length` header. (#1810)

### Contributors

Thanks to @julianromli (#1716, #1840, #1842, #1856, #1857, #1858, #1859, #1862, #1864, #1868, #1875, #1884, #1885, #1888, #1896, #1901, #1908, #1910, #1918, #1919, #1927, #1933, #1961, #1973), @rfxlamia (#1758), and @gnoviawan for their work in this release.

**Full changelog:** https://github.com/gnoviawan/troupe/compare/v0.4.60...v0.5.0

### Downloads

Pick the build for your platform from the assets below. macOS and Linux artifacts are signed, and the auto-updater picks this release up.

## [0.4.60] - 2026-11-07

This is the last release under the Termul name, and the next one, 0.5.0, renames the app to Troupe.

### Highlights

- **Automations.** Ask an agent in chat to set up repeatable work, and it drafts an automation for you to review. Agents can create, edit and propose automations, but only you can approve, enable, run or delete one. Automations live in a new Automations tab on desktop, web and phone, and run on demand or on a schedule in the desktop app and in `termul-server`. A run can call a web endpoint, run an agent, transform and check data, repeat steps for each item, merge branches, wait (a wait survives a host restart), and call a tool on one of the project's MCP servers directly. Each agent step gets only the tools you approve on its card. Termul also notices when you ask for the same thing on 3 or more days in 30 and offers to automate it. The check runs locally, and a setting turns it off. (#1582, #1583, #1584, #1585, #1586, #1587, #1600, #1608, #1687, #1690, #1693, #1694, #1718)

  ![Automations tab with an automation open on its node diagram](https://raw.githubusercontent.com/gnoviawan/termul/release-assets/v0.4.60/automations.png)

- **A new look for subagents.** When an agent fans out work, its subagents show as one group in the turn, with live progress, a timer and a status summary. Click a row to open that subagent beside the chat on desktop, or as a full-height sheet on a phone. You can stop one subagent and leave the rest running. Subagents from Claude, Codex, Cursor, OpenCode and Gemini CLI now map to one record on the host, so live, reopened, desktop and web chats show the same state, and subagents stay in the chat history after the turn ends. Background subagents keep running after their turn, and a chat shows as working while its agent works outside a prompt turn. Changed files and subagents share one status pill in the top right, so the composer keeps its full height. (#1494, #1642, #1646, #1647, #1680, #1681, #1683, #1685, #1686, #1688, #1695, thanks @julianromli)

  ![A chat turn with a subagent group and the subagent side panel open](https://raw.githubusercontent.com/gnoviawan/termul/release-assets/v0.4.60/subagents.png)

- **Browse and install MCP servers.** Settings > MCP servers lists the servers in the official MCP Registry, with search, logos, and a plain reason when a server can't be installed. The catalog syncs live from the Registry by default, and a host setting turns sync off. Only the host fetches logos, so your browser or phone never contacts a publisher. Install is one flow. Fill in what the server needs, review the exact command or URL, and Termul saves it, checks that it responds, and offers **Sign in** when it needs OAuth. To add a server that isn't in the catalog, a guided form asks where it runs and for its name and URL or command, so you no longer have to paste JSON. (#1543, #1547, #1603, #1604, #1613, #1627, #1737)

  ![Settings MCP servers catalog with search results and logos](https://raw.githubusercontent.com/gnoviawan/termul/release-assets/v0.4.60/mcp-catalog.png)

- **GitHub issues and pull requests.** A GitHub tab in the Side panel and a GitHub section in the phone drawer list the project's issues and PRs, with filters for text, labels, assignee and sort order. Open one to read it, comment, reply to review threads, edit labels and assignees, or close and reopen it. You can also hand an item to an agent. Start an issue in a new worktree chat, Ask about it in a read-only chat, Review a PR, Fix CI from its failing checks, or Resolve comments from its unresolved threads. Termul stores no GitHub token. It runs your `gh` CLI on the host. (#1610, #1611, #1612, #1670)

  ![GitHub issues and pull requests in the side panel with a pull request open](https://raw.githubusercontent.com/gnoviawan/termul/release-assets/v0.4.60/github.png)

- **A + menu in the composer.** On desktop, the chat composer and the agent launcher share one + menu for files and photos, mentions, slash commands, Termul tools (Browser, Canvas and Automations) and Connectors. Pick a Termul tool and Termul tells the agent to use it, whichever agent you run. Connectors lists your MCP servers with an on/off switch for each. A bare `@` now opens Conversations and Explorer, so you can attach another chat or pick a project file without typing. Select text in any message and press Cmd+L or Ctrl+L to add it to the composer as a context chip. The context window bar shows what fills the window and how much is left, warns before it fills, and has Compact in the same place. The V2 model picker is now the default. (#1751, #1657, #1532, #1578, #1571, thanks @julianromli)

  ![The composer + menu with Termul tools open](https://raw.githubusercontent.com/gnoviawan/termul/release-assets/v0.4.60/composer-plus.png)

- **Images, videos and PDFs.** Images, videos and audio that an agent produces show inline in chat on desktop and web, whatever shape the agent sends them in. Two or more images in a row show as a strip of tiles, with a full-screen viewer that swipes and downloads. Images, videos and PDFs also open in read-only editor tabs from the file tree and every other place you open files, with a pdf.js viewer for PDFs. (#1564, #1565, #1566, #1567, #1666)
- **Claude accounts switch for you at the limit.** When a Claude chat hits its account's usage limit, Termul moves it to the signed-in account with the most room and resends the last prompt, even if the chat is on another tab. If no account has room, the chat continues when the limit resets. "Switch accounts automatically" in Settings > Provider controls this and is on by default. The account chip shows the 5-hour window with a label, and the weekly window too once it passes 80%. The browser version can now add, sign in to, rename and remove Claude accounts. On Windows, accounts share projects, settings and skills without Developer Mode, so a chat resumes after a switch. (#1559, #1561, #1570, #1602, #1516)
- **Agent browsing on every desktop OS, and a newer canvas.** Agent browser automation now works on Windows, macOS and Linux. Agents browse in their own login profile, Termul checks each site, asks before risky actions, and a Stop button ends the run. The design canvas moved to figcraft termul-v0.6.0 (upstream v0.15.1 and later), which gives agents `wait_for_changes`. A new termul-canvas skill gives agents design defaults and a working order. (#1371, #1463, #1544, #1734)
- **Installing and updating.** On a Wayland session with Mesa 25 or newer, the Linux AppImage no longer opens a blank window. `scripts/install.sh` now installs a `termul-manager` launcher that loads the host's Wayland library (#1757, fixes #1682, thanks @dapzero). The pre-update backup skips agent binaries and other data Termul can download again, skips symlinks, and removes its partial folder when it fails, so an update no longer sits at 0% or stops on a backup error. The download now times out after 30 minutes instead of stalling (#1671, #1678, #1715). Nightly builds update in the app with the same signed flow as Stable (#1549, #1551, thanks @julianromli).
- **Smaller touches.** Report a bug from inside the app, with redacted diagnostics you can read before you send (#1433). The code editor has find and replace and go to line (#1493, thanks @FnDXueyi). Oh My Pi is a bundled agent, and you can remove a custom agent from Agent chats settings (#1502, #1504, thanks @mannnrachman). Skills installed for Claude Code, plugin skills included, show in the slash menu (#1456, thanks @rfxlamia), and clicking a skill chip opens its SKILL.md (#1485, thanks @julianromli). Thoughts and tool calls between two messages fold into one row (#1450). The phone composer and its Select model and Add to chat sheets follow the Claude mobile app (#1633, #1641). Termul Light has a v2 palette, and Termul Light and Dark share one brand blue (#1507, thanks @julianromli). The Side panel keeps its tabs when the Rail collapses (#1720), and the Side panel chat menu has Reload this chat (#1514, thanks @julianromli). A fresh `termul-server` is set up in the browser instead of a terminal wizard (#1577).

### Fixes

**Agent chats**
- A long agent turn renders its latest 40 steps, with a control to load earlier ones, so it no longer grows memory without limit. (#1674)
- Reopening an idle chat on the agent that holds it keeps the live session, so its background shells and subagents keep running. (#1689, #1692)
- A chat that reopens mid-turn gets its pending question and model controls back. (#1661)
- A chat reopened any way other than Reload this chat gets its MCP servers again. (#1510)
- A first prompt is no longer lost when another client reopens the chat at the same moment, and a follow-up no longer jumps ahead of it. (#1677, #1736)
- The Changed files list shows only the files this session edited that are still uncommitted. (#1525, #1703, #1706, thanks @julianromli)
- Effort, Fast and other model options come back together with the model, and your picks return when an agent resets a session to its defaults. Option values an agent rejects are disabled in the selector. (#1679, #1488, #1489, thanks @davidgrldo)
- External links open only after you click **Open** in the confirm dialog. (#1545, thanks @julianromli)
- Stream and rate-limit errors read as short messages, and npx startup errors name the real cause. (#530, #1454, #1615, thanks @julianromli and @rfxlamia)
- The Claude limit warning sits inside the composer card, so its Switch button works, and the Add to context button stays opaque on hover. (#1491, #1562, thanks @julianromli)
- Claude agents can see Termul's plan and title tools, so the plan panel fills in. (#1723)
- The launcher no longer starts the default agent at boot when your saved agent is another one. (#1673)
- The agent icon stays on chat rows after the agent goes idle, and on the composer while a launch creates its worktree. (#1667, #1701)
- Keyboard focus rings use one dim color in every theme, and a horizontal scrollbar no longer flashes under the transcript. (#1652, #1654, #1658, #1659, #1675, #1676)

**Chat list**
- Every device shows each chat's Working, Needs you and New activity, and each project's counts, from the host. A chat started on one device shows its title on the others. (#1707, #1708, #1740)

**Agent sign-in**
- You can sign in with a typed API key on desktop and the web, and with Codex's device code. (#1644, #1645)
- You can sign in to a signed-out agent from its model tab and from the launcher, and a terminal sign-in completes after the selector closes. (#1639, #1640, #1651)
- A long sign-in link stays inside the sign-in dialog. (#1419, thanks @yolkmonday)

**Phone and web**
- After 20 seconds of stable connection the reconnect delay resets, so "Reconnecting" no longer drags on behind a flaky tunnel. (#1621)
- Pending tool approvals come back after a reconnect. (#1599)
- The phone composer returns to the bottom after the keyboard closes, keeps its placeholder on one line, and focuses on tap. (#1634, #1638, #1662)
- On a phone, long-press selection and Paste are left to the OS, and the model sheet focuses itself instead of its search field. (#1619, #1625)
- The phone drawer shows open chats under Active and a fixed created time in Recents, with tidier sections and footer. (#1620, #1632, #1653, #1664)

**Editor, explorer and terminal**
- Show in File Explorer works on Windows verbatim paths. (#1733)
- Untracked folders expand in the Changes panel instead of showing an error. (#1499, thanks @davidgrldo)
- Nerd Font icons render in the terminal. (#1631, thanks @superrexy)
- Closing a terminal returns at once. (#1515)
- Old agent browser tabs no longer come back after a restart. (#1614)

**Linux, macOS and Windows**
- On macOS, terminal voice tools can use the microphone. (#1410, thanks @alfianm)
- On macOS, the workspace no longer rubber-bands under a trackpad scroll. (#1704, thanks @insankhamil)
- On Linux, closing the window hides it to the tray, and programs Termul starts no longer inherit the AppImage environment. (#1444, #1517)
- The Linux .deb and .rpm install alongside the distro ripgrep package. (#1576)

**Canvas**
- The canvas loads in macOS and Linux release builds instead of staying on "Loading editor". (#1684)
- Canvas fixes cover data loss, exports, document names, symbol fonts on Windows and HEAD requests. (#1418, #1460, #1461, #1462, #1466, #1739)

**MCP**
- MCP OAuth refuses unsafe sign-in links, and the web sign-in popup opens again. (#1539, #1542)
- The catalog's source card keeps its space while it loads, and an upgrade no longer forces a full Registry crawl. (#1754, #1756)

**termul-server**
- `termul-server --check-update` verifies signatures again, so the server can update itself. (#1558)
- The systemd unit from `onboard` stays up when one child process is killed for running out of memory. (#1650)
- `termul-server` no longer needs GTK or WebKit on the host. (#1505, thanks @mannnrachman)

### Under the hood

- Groundwork for a native command editor in the terminal: authenticated shell sessions, client transports, session lifecycle and a CodeMirror editor. Shell activation comes later, so terminals start as before. (#1538, #1550, #1556, #1574, thanks @rfxlamia)
- `termul-server` serves an OpenAPI 3.1 spec at `/openapi.json`, `termul-server openapi` prints it, and every release attaches `openapi.yaml`. (#1635, #1636, #1637)
- The bundled MCP Registry snapshot is parsed on the first catalog request, so `termul-server` idles at about 29 MB instead of 258 MB. (#1732)
- A release build fails if the canvas bundle is missing, and nightly builds embed the canvas bundle and the live MCP Registry snapshot. (#1719, #1721, #1722)
- Nightly macOS builds are signed and notarized, and old nightly files are pruned from the moving release. (#1500, #1579, thanks @davidgrldo)
- Termul caps the length of an agent's output line before buffering it. (#1598)
- The docs cover automations, the GitHub tab, the MCP catalog and its live sync, subagents, media in chat and in the editor, agent sign-in, the phone composer, Claude accounts and Nightly updates. (#1509, #1523, #1524, #1552, #1554, #1555, #1560, #1563, #1569, #1573, #1605, #1607, #1609, #1616, #1649, #1660, #1668, #1696, #1697, #1698, #1699, #1700)
- The design system is a live canvas at https://termul-design.pages.dev/. (#1507, thanks @julianromli)
- CI gains an automatic PR reviewer, the perf toolkit runs on dev, a lab times reconnect recovery, and flaky tests are steadier. (#1537, #1572, #1622, #1628, #1629, #1630, #1506, #1672, #1713, #1714, #1742, #1755, thanks @julianromli, @AndreIsraelAO and @mannnrachman)
- The landing site has privacy and terms pages, a real 404 page, and a salted rate-limit hash. (#1520, #1521, #1533, thanks @davidgrldo)
- The build pins the Tauri CLI and the figcraft bundle, and the canvas skill teaches named batch steps and fixing reported layout issues. (#1522, #1526, #1527, #1530, #1536)

### Installing on Windows

If Windows SmartScreen shows "Windows protected your PC", the installer isn't code-signed yet. Click **More info**, then **Run anyway**.

### Contributors

Thanks to @julianromli (#530, #1485, #1491, #1494, #1507, #1514, #1525, #1532, #1537, #1545, #1549, #1551, #1562, #1571, #1578), @rfxlamia (#1454, #1456, #1538, #1550, #1556, #1574), @davidgrldo (#1488, #1489, #1499, #1500, #1533, #1579), @mannnrachman (#1502, #1504, #1505, #1506), @dapzero (#1757), @FnDXueyi (#1493), @alfianm (#1410), @insankhamil (#1704), @superrexy (#1631), @AndreIsraelAO (#1630), @yolkmonday (#1419), and @gnoviawan for their work in this release.

**Full changelog:** https://github.com/gnoviawan/termul/compare/v0.4.52...v0.4.60

## [0.4.52] - 2026-10-19

### Highlights

- **Canvas Editor (experimental).** There's a new canvas for drawing and diagrams, built on figcraft. Agents can work on it through MCP tools — from the desktop app or a browser — with an agent cursor and inline guidance while they work. Canvases autosave, keep their state when closed (up to a limit), and cope with files changing underneath them and editor crashes. It's early days, so treat it as experimental. (#1369, #1396–#1405 via #1414)
- **Settings, one page each.** Every Preferences and Project settings category now gets its own page instead of one long scroll. (#1380, thanks @julianromli)
- **Smaller chat touches.** You can see which Claude account an agent is using right in the launcher, a pill shows the subagents running in the current turn, and what an agent supports moved into the model tooltip. (#1394, #1368, #1387, thanks @yolkmonday)
- **Syntax colors match Cursor.** The editor highlighting now matches the Cursor theme. (#963, thanks @julianromli)

### Fixes

**Agent chats**
- An idle agent no longer burns a thread — it now parks properly between turns. (#1398)
- You can't switch a chat to a Claude account that isn't signed in. (#1395)

**Editor and explorer**
- Browsing files on Windows works again on verbatim paths. (#1383)

**Web**
- Saving a file over a dead connection fails fast instead of hanging forever. (#1366)

### Under the hood

- There's a new docs site covering setup, terminals, files, git, the browser, AI agents, SSH, phones, settings and troubleshooting. (#1373–#1379)
- The macOS release build retries the stapling step and logs what the stapler says. (#1389)

### Contributors

Thanks to @julianromli (#963, #1380), @yolkmonday (#1368), and @gnoviawan for their work in this release.

**Full changelog:** https://github.com/gnoviawan/termul/compare/v0.4.50...v0.4.52

## [0.4.0] - 2026-05-31

### Features
- **Git** — Read-only Git History graph view (#202)
- **Git** — Commit, amend, and push from the Git panel (#200)
- **Git** — Git panel staging, unstaging, and discard (#190)
- **Git** — Git changes tab with terminal session/crash recovery and project settings (#143)
- **SSH** — SSH & Remote Connection Manager with full SFTP support (#146)
- **UI** — Platform-adaptive title bar with VSCode-style activity rail (#194)
- **Command Palette** — Project-first ordering, pinning, and condensed layout (#193)
- **Sidebar** — Declutter project list with truncation + search (#192)
- **Terminal** — Desktop notification + highlight for finished terminal on exit (#187)
- **Terminal** — Default renderer to WebGL with DOM fallback + AppSettings toggle (#175)
- **Worktree** — Git worktree as sub project (#171)
- **Worktree** — Simplified worktree UX for non-technical users (#186)
- **Tabs** — Middle-click close for terminal and browser tabs (#176)
- **Project** — Per-project settings gear button and compact context menu (#159)
- **Linux** — UI polish + HiDPI dropdown menu fixes (closes #129) (#165)
- **Landing** — New landing page (#153) with Google Tag Manager snippet (#181)

### Bug Fixes
- **Window** — Restore window geometry in logical pixels to prevent off-screen windows (#206)
- **Sidebar** — Only expand worktrees via chevron (#203)
- **SSH** — Repair SSH connection status, DNS connect, keychain persistence, and host-key verification (#198)
- **Security** — Add browser tab IPC caller validation (#196)
- **Security** — Implement secure storage for project environment variables (#167)
- **Security** — Redact persisted project env vars (#164)
- **Updater** — Repair auto-update download/install flow with confirm-before-restart (#191)
- **Git** — Align git tab height and remove panel gap (#189)
- **Worktree** — Run git from repo dir when removing worktree (#188)
- **Worktree** — Suppress git console window flashing on Windows (#183)
- **Terminal** — Prevent grid collapse to 1-2 rows on minimize/restore (#185)
- **Terminal** — Support clipboard image paste passthrough to CLI apps (#182)
- **Terminal** — Prevent terminal spawn storm during hidden window bootstrap (#174)
- **Terminal** — Resolve terminal skew after minimize/restore (#173)
- **Terminal** — Move xterm container ref inside padding wrapper (#172)
- **Browser** — Fix element selector annotation not working inside form tags (gh-127) (#140)
- **Explorer** — Prevent search from opening console window on Windows (#157)
- **UI** — Resolve new project modal/browser layering and require root directory (#158)
- **Mermaid** — Prevent DOM leak on syntax error (#156)

### CI & Chores
- **CI** — Migrate landing Docker hosting to Cloudflare Pages (#179)
- **CI** — Add macOS Intel release target (#163)
- **CI** — Migrate package workflow to bun (#166); pin bun CI to 1.3.x and bun action to v2 (#168)
- **Build** — Bump Vite to v8 (#169); remove obsolete vite config (#170)
- **TSConfig** — Use bundler moduleResolution in tsconfig.node.json (#161)

## [0.3.8] - 2026-05-18

### Features
- **Terminal** — Ctrl/Cmd+click URLs open in internal browser, respecting default browser preference (#125)
- **Terminal** — Upgrade xterm.js to 6.1-beta; fix terminal truncation on minimize/project-switch and memory leaks (#135)
- **UI** — Pane-level fullscreen toggle with smooth animation (#141)
- **UI** — Redesigned command palette power tools (#142)
- **UI** — Shortcut reference menu for quick keyboard shortcut lookup (#145)
- **Search** — Ripgrep-powered sidecar file search with explorer resize handles and tooltip UX (#124)

### Bug Fixes
- **Shortcuts** — App shortcuts now work consistently from terminal, editor, and browser focus (#128)
- **Terminal** — Shortcut passthrough: app shortcuts fire correctly from terminal focus (#138)
- **Editor** — Fix visibility hidden for editor panels + window permissions (#116)

### CI & Chores
- **GitHub** — Add community templates & CI security hardening (#144)

### Documentation
- Professionalize README with extended feature list and star tracking (#136, #137)
- Add project context documentation and docs index updates (#132)

## [0.3.6] - 2026-05-08

### Features
- **Browser** — Built-in browser tab foundation (#112)
- **Browser** — Web annotation tool for marking and annotating web pages (#113)
- **Terminal** — Open file paths on Ctrl/Cmd+click in terminal output (#110)
- **UI** — Close button added to AppPreferences and ProjectSettings headers (#118)

### Bug Fixes
- **macOS** — Platform-aware keyboard shortcuts, native traffic lights & error resilience (#114)
- **Mermaid** — Fix text invisible due to DOMPurify stripping style & foreignObject (#119)
- **Terminal** — Terminal stability improvements (#109)
- **Signing** — Update ed25519 public key for new key pair

## [0.3.4] - 2026-05-01

### Features
- **AUR** — Add Arch Linux (AUR) update support (#89)
- **Editor** — Add mermaid chart viewer to markdown editor
- **Editor** — Interactive mermaid charts with zoom, pan, and drag
- **Terminal** — Remember close confirmation preference and show tab close loading state

### Bug Fixes
- **Terminal** — Fix padding gap blending and container color cohesion
- **Terminal** — Remove artificial 300ms timeout on terminal kill
- **Editor** — Fix TOC active indicator not updating on click
- **Editor** — Scroll heading to top instead of center on TOC click
- **Editor** — Add smooth scroll to BlockNote TOC heading navigation
- **Editor** — Capture wheel events natively to prevent page scroll; fix zoom blur on mermaid charts
- **MermaidBlock** — Remove DOMPurify to preserve mermaid SVG styles
- **MermaidBlock** — Fix mermaid.initialize to preserve inline styles
- **MermaidBlock** — Use ref callback for wheel listener so zoom works after BlockNote re-mounts
- **Explorer** — Fix delete not working for folders and nested files
- **Editor** — Fix editor store operation status, close guards, XSS sanitization
- **Editor** — Fix pasting, deletion, rename error handling, tab status, and test leaks
- **Updater** — Recover from missing latest.json and harden CI pipeline
- **Security** — Replace env var pubkey with hardcoded ed25519 public key

### Styling
- Compact sidebar projects layout & remove bold text
- Compact tabbar layout
- Add split pane border via ResizableHandle bg-border
