//! The Settings pages in rail order: `pages/app-preferences/categories.tsx` and
//! `pages/project-settings/categories.tsx`. A page this client does not draw yet names the unit
//! that ports it; `pages_tests.rs` checks every id of both files is listed here.

/// Who draws a page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owner {
    /// This client draws it.
    Here,
    /// A later unit of the Troupe on Rune plan ports it; the page shows a placeholder.
    Later(&'static str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Page {
    pub id: &'static str,
    pub label: &'static str,
    /// The rail group heading; Preferences has none.
    pub group: Option<&'static str>,
    pub description: &'static str,
    /// A `components/icons` export.
    pub icon: &'static str,
    pub owner: Owner,
}

const fn page(
    id: &'static str,
    label: &'static str,
    group: Option<&'static str>,
    icon: &'static str,
    description: &'static str,
    owner: Owner,
) -> Page {
    Page {
        id,
        label,
        group,
        description,
        icon,
        owner,
    }
}

/// The web client's About description: it updates with its server.
pub const WEB_ABOUT: &str = "This Troupe server, logs for bug reports, and a full reset.";

pub const APP_PAGES: [Page; 11] = [
    page(
        "preferences",
        "Preferences",
        None,
        "Sliders",
        "How Troupe looks, when it notifies you, where links open and how files save.",
        Owner::Here,
    ),
    page(
        "terminal",
        "Terminal",
        Some("Terminal"),
        "Terminal",
        "How terminals draw text, and what new terminals start with.",
        Owner::Here,
    ),
    page(
        "terminal-profiles",
        "Profiles",
        Some("Terminal"),
        "Bookmark",
        "Saved terminals with their own shell, arguments, folder and variables. Open one from the command palette.",
        Owner::Here,
    ),
    page(
        "provider",
        "Accounts",
        Some("Agents"),
        "KeyRound",
        "The accounts that agent chats use, and how much of each limit is used.",
        Owner::Later("R9c follow-up"),
    ),
    page(
        "ai-agents",
        "Agents",
        Some("Agents"),
        "Bot",
        "The agents Troupe can start, and how long their chats may run.",
        Owner::Here,
    ),
    page(
        "subagents",
        "Sub-agents",
        Some("Agents"),
        "GitBranch",
        "Agents your chats can hand work to, and what each one runs with.",
        Owner::Later("R9c follow-up"),
    ),
    page(
        "skills",
        "Skills",
        Some("Agents"),
        "Sparkles",
        "Skills in your home folder and in the open project. The slash menu uses this list.",
        Owner::Here,
    ),
    page(
        "mcp-servers",
        "MCP servers",
        Some("Agents"),
        "Network",
        "Servers that new agent chats can use. Each enabled server is offered when a chat starts.",
        Owner::Later("R9c follow-up"),
    ),
    page(
        "browser",
        "Browser",
        Some("Agents"),
        "Globe",
        "Which sites agents may browse without asking, and whether they keep logins between chats.",
        Owner::Later("R11"),
    ),
    page(
        "shortcuts",
        "Keyboard shortcuts",
        Some("App"),
        "Keyboard",
        "Select a shortcut, then press the new keys. Esc cancels.",
        Owner::Here,
    ),
    page("about", "About", Some("App"), "Info", WEB_ABOUT, Owner::Here),
];

pub const PROJECT_PAGES: [Page; 6] = [
    page(
        "general",
        "General",
        Some("Project"),
        "Settings",
        "Basic project identification and location.",
        Owner::Here,
    ),
    page(
        "shell",
        "Shell",
        Some("Project"),
        "Terminal",
        "The shell new terminals in this project open with.",
        Owner::Here,
    ),
    page(
        "env-vars",
        "Environment variables",
        Some("Project"),
        "Braces",
        "Secrets and config injected into your shell session. Secret values are cleared on app restart until secure storage is added.",
        Owner::Here,
    ),
    page(
        "symlinks",
        "Worktree symlinks",
        Some("Project"),
        "Link2",
        "Directories to symlink from the project root into worktrees, so shared dependencies such as node_modules work without reinstalling.",
        Owner::Here,
    ),
    page(
        "provider",
        "Accounts",
        Some("Agents"),
        "KeyRound",
        "Choose the account that this project's agent chats use.",
        Owner::Later("R9c follow-up"),
    ),
    page(
        "browser",
        "Browser",
        Some("Agents"),
        "Globe",
        "Which sites agents may browse in this project without asking.",
        Owner::Later("R11"),
    ),
];

/// Old App Preferences ids still open the page that holds their settings (`APP_PAGE_ALIASES`).
const ALIASES: [(&str, &str); 9] = [
    ("interface", "preferences"),
    ("notifications", "preferences"),
    ("editor", "preferences"),
    ("appearance", "terminal"),
    ("shell", "terminal"),
    ("updates", "about"),
    ("server", "about"),
    ("diagnostics", "about"),
    ("reset", "about"),
];

/// `resolveAppPage`.
#[must_use]
pub fn resolve_app_page(id: &str) -> &str {
    ALIASES.iter().find(|(old, _)| *old == id).map_or(id, |(_, new)| new)
}

#[must_use]
pub fn find(pages: &'static [Page], id: &str) -> &'static Page {
    pages.iter().find(|p| p.id == id).unwrap_or(&pages[0])
}

#[cfg(test)]
#[path = "pages_tests.rs"]
mod tests;
