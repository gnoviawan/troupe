import type { Check, Surface } from '../types'
import { ensureProject } from '../world'
import { click, css, DASHBOARD, label, press, rclick, see, step, type } from './dsl'

const openPreferences = click(label('Settings'))
const page = (name: string) =>
  click(css(`nav[aria-label="Settings pages"] >> role=button[name="${name}"s]`))
const PREFERENCES = step('b-preferences', [openPreferences], see('Preferences', 'Interface zoom'))

/** An App Preferences page: its nav label and a text only that page paints. */
const pref = (id: string, name: string, text: string, settled: Check[] = []): Surface => ({
  id: `settings.app-${id}`,
  area: 'settings',
  title: `App preferences: ${name}`,
  source: { screen: 'settings', name: `app:${id}` },
  steps: [DASHBOARD, PREFERENCES, step('c-page', [page(name)], [...see(text), ...settled])]
})

const openProjectSettings = [
  rclick(css('[data-testid="project-item-matrix-beta"]')),
  click(label('Settings', 'menuitem'))
]
const PROJECT_SETTINGS = step(
  'b-settings',
  openProjectSettings,
  see('Basic project identification and location.')
)

/** A Project settings page of beta. */
const projectPage = (id: string, name: string, text: string): Surface => ({
  id: `settings.project-${id}`,
  area: 'settings',
  title: `Project settings: ${name}`,
  source: { screen: 'settings', name: `project:${id}` },
  prepare: (world) => ensureProject(world, 'beta'),
  steps: [
    DASHBOARD,
    PROJECT_SETTINGS,
    step('c-page', [click(css(`nav >> role=button[name="${name}"s]`))], see(text))
  ]
})

/** A dialog opened from an App Preferences page by a button with `trigger` as its text. */
const prefDialog = (
  id: string,
  title: string,
  file: string,
  pageName: string,
  trigger: string,
  text: string[]
): Surface => ({
  id: `settings.${id}`,
  area: 'settings',
  title,
  source: { overlay: [file] },
  steps: [
    DASHBOARD,
    PREFERENCES,
    step('c-page', [page(pageName)], see(trigger)),
    step('d-dialog', [click(css(`[role="dialog"] button:has-text("${trigger}")`))], see(...text))
  ]
})

const NO_ACCOUNT_ROWS =
  'needs a signed-in Claude or Codex account; the seeded host has only signed-out default rows'
const NO_MCP_SERVER = 'needs a configured MCP server, which the seeded world does not hold'

export const SETTINGS: Surface[] = [
  {
    id: 'settings.app',
    area: 'settings',
    title: 'App preferences: Interface',
    source: { overlay: ['components/settings/SettingsModal.tsx'] },
    steps: [DASHBOARD, PREFERENCES]
  },
  pref('interface', 'Interface', 'Size of the whole interface and defaults for new projects.'),
  pref('notifications', 'Notifications', 'Choose when Troupe sends you a system notification.'),
  pref('editor', 'Editor', 'How code files save while you work.'),
  pref('appearance', 'Appearance', 'How terminals look, draw text and open links.'),
  pref('shell', 'Shell', 'The shell that new terminals start with.'),
  pref('terminal-profiles', 'Profiles', 'No terminal profiles'),
  pref('ai-agents', 'Agent chats', 'Time limits for agent chats, and the agents Troupe can start.'),
  pref(
    'subagents',
    'Sub-agents',
    'Agents your chats can hand work to, and what each one runs with.'
  ),
  pref('skills', 'Skills', 'Skills in your home folder and in the open project.'),
  pref('provider', 'Accounts', 'The Claude and Codex accounts that agent chats use'),
  pref('mcp-servers', 'MCP servers', 'No MCP servers configured', [
    { text: 'Loading the catalog', gone: true, timeoutMs: 15_000 }
  ]),
  pref('browser', 'Browser', "Browser rules aren't available on this host."),
  pref('shortcuts', 'Keyboard shortcuts', 'Reset all shortcuts'),
  pref('server', 'Server', 'Where this Troupe server can be reached, and how it updates.'),
  pref('diagnostics', 'Logs', 'Copy or export the application log to report a problem.'),
  pref('reset', 'Reset', 'Every setting goes back to its default'),
  {
    id: 'settings.app-updates',
    area: 'settings',
    title: 'App preferences: Updates',
    source: { screen: 'settings', name: 'app:updates' },
    reference: 'tauri',
    blocked: 'the Updates page is desktop-only (the web client updates with its server)',
    steps: []
  },
  projectPage('shell', 'Shell', 'The shell new terminals in this project open with.'),
  projectPage(
    'env-vars',
    'Environment variables',
    'Secrets and config injected into your shell session.'
  ),
  projectPage(
    'symlinks',
    'Worktree symlinks',
    'Directories to symlink from the project root into worktrees'
  ),
  projectPage('provider', 'Accounts', "Choose the account that this project's agent chats use."),
  projectPage(
    'browser',
    'Browser',
    'Which sites agents may browse in this project without asking.'
  ),
  {
    id: 'settings.project-unsaved',
    area: 'settings',
    title: 'Project settings unsaved changes confirm',
    source: { overlay: ['pages/ProjectSettings.tsx'] },
    prepare: (world) => ensureProject(world, 'beta'),
    steps: [
      DASHBOARD,
      PROJECT_SETTINGS,
      step('c-edit', [click(css('input[value="beta"]')), type('x')], see('Save')),
      step('d-confirm', [press('escape')], see('Unsaved Changes'))
    ]
  },
  prefDialog(
    'reset-all',
    'Reset all settings confirm',
    'pages/AppPreferences.tsx',
    'Reset',
    'Reset settings…',
    ['Reset all settings?']
  ),
  prefDialog(
    'reset-shortcuts',
    'Reset all shortcuts confirm',
    'pages/AppPreferences.tsx',
    'Keyboard shortcuts',
    'Reset all shortcuts',
    ['Reset all shortcuts?']
  ),
  prefDialog(
    'terminal-profile',
    'New terminal profile dialog',
    'pages/app-preferences/terminal-profiles-section.tsx',
    'Profiles',
    'Add profile',
    ['New terminal profile']
  ),
  prefDialog(
    'custom-agent',
    'Add custom agent dialog',
    'components/agents/CustomAcpAgentDialog.tsx',
    'Agent chats',
    'Add custom agent',
    ['Add custom agent']
  ),
  {
    id: 'settings.icon-picker',
    area: 'settings',
    title: 'Custom agent icon picker',
    source: { overlay: ['components/agents/IconPicker.tsx'] },
    steps: [
      DASHBOARD,
      PREFERENCES,
      step('c-page', [page('Agent chats')], see('Add custom agent')),
      step('d-dialog', [click(css('[role="dialog"] button:has-text("Add custom agent")'))]),
      step('e-picker', [click(css('button[title="Choose icon"]'))], see('Choose icon'))
    ]
  },
  {
    id: 'settings.remove-custom-agent',
    area: 'settings',
    title: 'Remove custom agent confirm',
    source: { overlay: ['components/settings/AcpAgentsSettings.tsx'] },
    blocked:
      'needs a custom agent added in settings; the scripted agent is registered per device and is not listed as removable',
    steps: []
  },
  prefDialog(
    'mcp-server',
    'Add MCP server dialog',
    'components/settings/McpServerDialog.tsx',
    'MCP servers',
    'Add server',
    ['Add MCP server']
  ),
  {
    id: 'settings.mcp-row-menu',
    area: 'settings',
    title: 'MCP server row menu',
    source: { overlay: ['components/settings/RowMenu.tsx'] },
    blocked: NO_MCP_SERVER,
    steps: []
  },
  {
    id: 'settings.mcp-catalog-install',
    area: 'settings',
    title: 'MCP catalog install dialog',
    source: { overlay: ['components/settings/McpCatalogInstallDialog.tsx'] },
    steps: [
      DASHBOARD,
      PREFERENCES,
      step(
        'c-page',
        [page('MCP servers')],
        [...see('MCP catalog'), { text: 'Loading the catalog', gone: true, timeoutMs: 15_000 }]
      ),
      step(
        'd-dialog',
        [click(css('[role="dialog"] >> role=button[name="Install"s]'))],
        [
          ...see('Cancel'),
          { text: 'Loading what this server needs', gone: true, timeoutMs: 15_000 },
          { text: 'Checking…', gone: true, timeoutMs: 15_000 }
        ]
      )
    ]
  },
  {
    id: 'settings.mcp-consent',
    area: 'settings',
    title: 'MCP server consent dialog',
    source: { overlay: ['components/settings/McpConsentDialog.tsx'] },
    blocked:
      'opens when an agent or a link asks to add an MCP server; the scripted agent asks none',
    steps: []
  },
  {
    id: 'settings.mcp-oauth-paste',
    area: 'settings',
    title: 'MCP OAuth code paste dialog',
    source: { overlay: ['components/settings/McpOAuthPasteDialog.tsx'] },
    blocked: 'needs an MCP server that signs in with OAuth; the seeded world has none',
    steps: []
  },
  {
    id: 'settings.clear-browser-profile',
    area: 'settings',
    title: 'Clear agent browser profile confirm',
    source: { overlay: ['components/settings/BrowserPolicySettings.tsx'] },
    blocked: 'browser rules are not available on the seeded host (no agent browser)',
    steps: []
  },
  prefDialog(
    'add-account',
    'Add account flow',
    'components/settings/provider/AddAccountFlow.tsx',
    'Accounts',
    'Add account',
    ['Add']
  ),
  {
    id: 'settings.claude-account-menu',
    area: 'settings',
    title: 'Claude account actions menu',
    source: { overlay: ['components/settings/provider/ClaudeAccountActions.tsx'] },
    steps: [
      DASHBOARD,
      PREFERENCES,
      step('c-page', [page('Accounts')], see('Claude Code')),
      step('d-menu', [click(css('button[aria-label="Actions for Default"]'))], see('Rename'))
    ]
  },
  {
    id: 'settings.codex-account-menu',
    area: 'settings',
    title: 'Codex account actions menu',
    source: { overlay: ['components/settings/provider/CodexAccountActions.tsx'] },
    steps: [
      DASHBOARD,
      PREFERENCES,
      step('c-page', [page('Accounts')], see('Codex')),
      step(
        'd-menu',
        [click(css('button[aria-label="Actions for Default" i] >> nth=1'))],
        see('Rename')
      )
    ]
  },
  {
    id: 'settings.rename-account',
    area: 'settings',
    title: 'Rename account dialog',
    source: { overlay: ['components/settings/provider/AccountNameDialog.tsx'] },
    steps: [
      DASHBOARD,
      PREFERENCES,
      step('c-page', [page('Accounts')], see('Claude Code')),
      step('d-menu', [click(css('button[aria-label="Actions for Default"]'))], see('Rename')),
      step('e-dialog', [click(css('[role="menuitem"]:has-text("Rename")'))], see('Rename account'))
    ]
  },
  {
    id: 'settings.claude-remove-account',
    area: 'settings',
    title: 'Remove Claude account confirm',
    source: { overlay: ['components/settings/provider/ProviderSettings.tsx'] },
    blocked: NO_ACCOUNT_ROWS,
    steps: []
  },
  {
    id: 'settings.codex-remove-account',
    area: 'settings',
    title: 'Remove Codex account confirm',
    source: { overlay: ['components/settings/provider/CodexProviderSettings.tsx'] },
    blocked: NO_ACCOUNT_ROWS,
    steps: []
  },
  {
    id: 'settings.claude-sign-in',
    area: 'settings',
    title: 'Claude sign-in link dialog',
    source: { overlay: ['components/settings/provider/LinkSignInDialog.tsx'] },
    blocked: 'starts a real Claude sign-in, which needs the claude CLI and the network',
    steps: []
  },
  {
    id: 'settings.codex-sign-in',
    area: 'settings',
    title: 'Codex sign-in dialog',
    source: { overlay: ['components/settings/provider/CodexSignInDialog.tsx'] },
    blocked: 'starts a real Codex sign-in, which needs the codex CLI and the network',
    steps: []
  }
]
