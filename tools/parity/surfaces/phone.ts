import type { Source, Surface } from '../types'
import {
  HISTORY_COMMAND,
  resetAlpha,
  SNAPSHOT_NAME,
  seedCommandHistory,
  seedSnapshot
} from '../world'
import {
  CHAT_SETTLED,
  CHAT_TITLE,
  click,
  css,
  DASHBOARD,
  label,
  rclick,
  see,
  step,
  txt,
  type,
  wait
} from './dsl'

/** The phone shell's header controls (390x844, touch). */
const openDrawer = click(label('Open menu'))
const openMore = click(label('More'))
const openProjectSheet = click(label('alpha · main, switch project'))
const openPhoneChat = [openDrawer, click(txt(CHAT_TITLE, false)), wait(1500)]
const DRAWER = step('b-drawer', [openDrawer], see('Recents', 'Search chats', 'Import chats'))
const MORE = step('b-more', [openMore], see('Git changes', 'Command palette', 'Project settings'))
const CHAT = step('b-chat', openPhoneChat, CHAT_SETTLED)
const TERMINAL = step('c-terminal', [click(txt('New terminal')), wait(2500)], see('Paste'))
const GIT_SHEET = step(
  'c-git',
  [click(txt('Git changes'))],
  see('Staged', 'notes.txt', 'Commit to main')
)
const FILES = step('c-files', [click(txt('Files'))], see('Browse project files', 'README.md'))
const PALETTE = step(
  'c-palette',
  [click(txt('Command palette'))],
  see('New Terminal', 'Open a new shell in the active pane')
)
const PREFERENCES = step(
  'c-preferences',
  [click(txt('Project settings'))],
  see('Basic project identification and location.')
)

/** A surface of the phone profile. */
const phone = (
  id: string,
  area: Surface['area'],
  title: string,
  source: Source,
  steps: Surface['steps'],
  extra: Partial<Surface> = {}
): Surface => ({
  id: `phone.${id}`,
  area,
  title,
  source,
  profile: 'phone',
  // A surface that starts on another route opens there, not on the dashboard.
  steps: extra.start ? steps : [DASHBOARD, ...steps],
  ...extra
})

/** A phone journey: one of the `tests/e2e/mobile-*.spec.ts` flows, driven in the seeded world. */
const journey = (
  spec: string,
  area: Surface['area'],
  title: string,
  steps: Surface['steps'],
  extra: Partial<Surface> = {}
): Surface =>
  phone(
    `journey-${spec}`,
    area,
    title,
    { journey: `tests/e2e/mobile-${spec}.spec.ts` },
    steps,
    extra
  )

/** The phone-only overlays of the inventory. */
const PHONE_OVERLAYS: Surface[] = [
  phone(
    'drawer',
    'rail',
    'Shell drawer',
    { overlay: ['components/mobile/MobileShellDrawer.tsx'] },
    [DRAWER]
  ),
  phone(
    'header-more',
    'rail',
    'Header more sheet',
    { overlay: ['components/mobile/MobileHeaderMoreSheet.tsx'] },
    [MORE]
  ),
  phone(
    'project-sheet',
    'rail',
    'Project switcher sheet',
    {
      overlay: [
        'components/chat/ProjectSwitcherDrawer.tsx',
        'components/chat/ProjectSwitcherDrawer.tsx'
      ]
    },
    [step('b-sheet', [openProjectSheet], see('Projects', 'Add project'))]
  ),
  phone(
    'composer-add',
    'composer',
    'Composer add sheet',
    { overlay: ['components/chat/composer/ComposerAddSheet.tsx'] },
    [step('b-sheet', [click(label('Add to chat'))], see('Camera', 'Photos', 'Troupe tools'))]
  ),
  phone(
    'message-actions',
    'chat',
    'Message actions sheet',
    { overlay: ['components/chat/MessageActions.tsx'] },
    [
      CHAT,
      step(
        'c-sheet',
        [rclick(txt('Where else is the chat route parsed?'))],
        see('Copy', 'Select text')
      )
    ]
  ),
  phone(
    'recents-actions',
    'rail',
    'Recents row actions sheet',
    { overlay: ['components/mobile/MobileRecentsActionsSheet.tsx'] },
    [
      DRAWER,
      step(
        'c-sheet',
        [click(css('button[aria-label^="More actions for How does"]'))],
        see('Chat actions', 'Copy chat reference')
      )
    ]
  ),
  phone(
    'terminal-actions',
    'terminal',
    'Terminal actions sheet',
    { overlay: ['components/mobile/MobileTerminalActionsSheet.tsx'] },
    [MORE, TERMINAL, step('d-sheet', [click(label('Terminal actions'))], see('Terminal 1'))]
  ),
  phone(
    'git-sheet',
    'git',
    'Git changes sheet',
    { overlay: ['layouts/workspace-layout/MobileWorkspaceShell.tsx'] },
    [MORE, GIT_SHEET],
    { prepare: resetAlpha }
  ),
  phone(
    'commit-more',
    'git',
    'Commit composer more-actions menu',
    { overlay: ['components/git/CommitComposer.tsx'] },
    [MORE, GIT_SHEET, step('d-menu', [click(label('More actions'))], see('Publish branch'))],
    { prepare: resetAlpha }
  ),
  phone(
    'files',
    'explorer',
    'Project files sheet',
    { overlay: ['components/mobile/MobileFileExplorer.tsx'] },
    [MORE, FILES]
  ),
  phone(
    'file-actions',
    'explorer',
    'File actions sheet',
    { overlay: ['components/mobile/MobileFileExplorer.tsx'] },
    [MORE, FILES, step('d-sheet', [click(label('Actions for notes.txt'))], see('Rename', 'Delete'))]
  ),
  phone(
    'file-delete',
    'explorer',
    'Delete file confirm',
    { overlay: ['components/mobile/MobileFileExplorer.tsx'] },
    [
      MORE,
      FILES,
      step('d-sheet', [click(label('Actions for notes.txt'))], see('Delete')),
      step(
        'e-confirm',
        [click(css('[role="dialog"] button:has-text("Delete")'))],
        see('Delete notes.txt')
      )
    ],
    { prepare: resetAlpha }
  )
]

/** The 20 phone journeys, one per `tests/e2e/mobile-*.spec.ts`. */
const JOURNEYS: Surface[] = [
  journey('a11y-floor', 'rail', 'Live region behind an open drawer', [DRAWER]),
  journey('chat-dock-approvals', 'chat', 'Chat dock above the transcript', [CHAT]),
  journey('composer-row', 'composer', 'Composer row and the model sheet', [
    step(
      'b-sheet',
      [click(css('[data-testid="agent-model-selector-trigger"]'))],
      see('This agent gives no model choice.')
    )
  ]),
  journey('drawer-home', 'rail', 'Drawer home: sections, search, Recents', [
    DRAWER,
    step('c-terminals', [click(txt('Terminals'))], see('Terminals'))
  ]),
  journey('message-actions', 'chat', 'Message action row on a settled turn', [CHAT]),
  journey('mobile-approval-edge-states', 'chat', 'Approval and elicitation edge states', [], {
    blocked: 'needs an agent elicitation or approval; the scripted agent asks for neither'
  }),
  journey(
    'mobile-back-stack-tail',
    'snapshots',
    'Snapshot modals on the phone',
    [
      step('a-list', [], see(SNAPSHOT_NAME, 'Restore')),
      step('b-modal', [click(txt('New'))], see('Create Snapshot'))
    ],
    { start: { hash: '#/snapshots', ready: SNAPSHOT_NAME }, prepare: seedSnapshot }
  ),
  journey('mobile-drawer-and-files-gaps', 'explorer', 'Files from the More sheet', [MORE, FILES]),
  journey(
    'mobile-git-history-layout',
    'git',
    'Git history rows on the phone',
    [DRAWER, step('c-history', [click(label('Git history'))], see('initial commit', 'add utils'))],
    { prepare: resetAlpha }
  ),
  journey('mobile-header-palette-parity', 'palette', 'Command palette from the header', [
    MORE,
    PALETTE
  ]),
  journey('mobile-integration-reconcile', 'git', 'Git sheet focus return', [MORE, GIT_SHEET], {
    prepare: resetAlpha
  }),
  journey('mobile-terminal-screen-reader-mode', 'settings', 'Terminal screen reader mode switch', [
    step('b-palette', [openMore, click(txt('Command palette'))], see('App Preferences')),
    step('c-preferences', [click(txt('App Preferences'))], see('Interface zoom')),
    step('d-appearance', [click(txt('Appearance'))], see('Screen reader mode'))
  ]),
  journey('mobile-text-field-zoom', 'composer', 'Focused composer on the phone', [
    step('b-focus', [click(css('[contenteditable="true"]')), type('Hello')], see('Hello'))
  ]),
  journey('mobile-toast-dock-clearance', 'notifications', 'Toast above the composer dock', [
    DRAWER,
    step(
      'c-sheet',
      [click(css('button[aria-label^="More actions for How does"]'))],
      see('Copy chat reference')
    ),
    step('d-toast', [click(txt('Copy chat reference'))], see('Chat reference copied'))
  ]),
  journey('native-text-selection', 'chat', 'Chat prose on the phone', [CHAT]),
  journey('overlay-back-stack', 'settings', 'Drawer to project settings in one tap', [
    MORE,
    PREFERENCES
  ]),
  journey('shell-fixes', 'palette', 'Palette New Project opens its modal', [
    MORE,
    PALETTE,
    step(
      'd-modal',
      [type('New Project'), click(css('[role="option"]:has-text("New Project")'))],
      see('Create New Project')
    )
  ]),
  journey('shell-header', 'rail', 'Header subtitle opens the project sheet', [
    step('b-sheet', [openProjectSheet], see('Projects', 'Add project'))
  ]),
  journey('terminal-key-bar', 'terminal', 'Terminal key bar', [MORE, TERMINAL]),
  journey(
    'text-field-zoom',
    'terminal',
    'Command history search on the phone',
    [
      step(
        'b-history',
        [openMore, click(txt('Command palette')), click(txt('Command History'))],
        see(HISTORY_COMMAND)
      )
    ],
    { prepare: seedCommandHistory }
  )
]

export const PHONE: Surface[] = [...PHONE_OVERLAYS, ...JOURNEYS]
