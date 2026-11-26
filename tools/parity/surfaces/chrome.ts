import type { Surface } from '../types'
import { resetAlpha } from '../world'
import { click, css, DASHBOARD, label, openChat, rclick, see, step, wait, wheel } from './dsl'

/** The rail footer and the command palette. */
export const CHROME: Surface[] = [
  {
    id: 'theme.picker',
    area: 'theme',
    title: 'Colour theme picker',
    source: { overlay: ['components/ThemePicker.tsx'] },
    steps: [
      DASHBOARD,
      step('b-picker', [click(label('Color themes'))], see('Color Themes', 'Catppuccin Light'))
    ]
  },
  {
    id: 'shortcuts.menu',
    area: 'shortcuts',
    title: 'Keyboard shortcuts menu',
    source: { overlay: ['components/TitleBarShortcutsPopover.tsx'] },
    steps: [
      DASHBOARD,
      step(
        'b-menu',
        [click(label('Open keyboard shortcuts menu'))],
        see('Shortcut Menu', 'Command palette')
      )
    ]
  },
  {
    id: 'palette.open',
    area: 'palette',
    title: 'Command palette from Search',
    source: { overlay: ['components/CommandPalette.tsx'] },
    steps: [
      DASHBOARD,
      step(
        'b-open',
        [click(label('Search'))],
        see('New Terminal', 'Open a new shell in the active pane')
      )
    ]
  },
  {
    id: 'dialogs.report-bug',
    area: 'dialogs',
    title: 'Report a bug dialog',
    source: { overlay: ['components/bug-report/ReportBugDialog.tsx'] },
    steps: [DASHBOARD, step('b-dialog', [click(label('Report a bug'))], see('Report a bug'))]
  },
  {
    id: 'dialogs.text-context-menu',
    area: 'dialogs',
    title: 'Text field context menu',
    source: { overlay: ['components/GlobalContextMenu.tsx'] },
    prepare: resetAlpha,
    steps: [
      DASHBOARD,
      step('b-changes', [click(label('Changes'))], see('README.md')),
      step(
        'c-menu',
        [rclick(css('input[aria-label="Commit summary"]'))],
        see('Paste', 'Select All')
      )
    ]
  },
  {
    id: 'dialogs.external-link',
    area: 'dialogs',
    title: 'Open external link confirm',
    source: { overlay: ['components/external-link/ExternalLinkConfirm.tsx'] },
    steps: [
      DASHBOARD,
      // The transcript does not always rest at its end (#1991); from its top the link lands in one place.
      step(
        'b-open',
        [
          openChat,
          wait(1500),
          wheel(css('[data-slot="message-scroller-viewport"]'), -50_000),
          wait(500)
        ],
        see('Where else is the chat route parsed?')
      ),
      step(
        'c-confirm',
        [click(css('a[href^="https://reactrouter.com"]'))],
        see('Open external link?')
      )
    ]
  },
  {
    id: 'dialogs.whats-new',
    area: 'dialogs',
    title: "What's new after an update",
    source: { overlay: ['components/WhatsNewModal.tsx'] },
    blocked: 'opens once after the app updates to a newer version; the seeded world never updates',
    steps: []
  },
  {
    id: 'remote.access',
    area: 'remote',
    title: 'Remote agent access popover',
    source: { overlay: ['components/RemoteAccessPopover.tsx'] },
    reference: 'tauri',
    blocked:
      'the shared-live host is desktop only; the web client shows its connection status in that slot',
    steps: []
  }
]
