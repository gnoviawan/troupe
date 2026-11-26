import type { Surface } from '../types'
import { HISTORY_COMMAND, seedCommandHistory } from '../world'
import { click, css, DASHBOARD, id, label, rclick, see, step, txt, wait } from './dsl'

const OPEN_TERMINAL = step(
  'b-open',
  [click(label('New terminal')), wait(2500)],
  see('Terminal 1', 'main')
)
const openPalette = click(label('Search'))

export const TERMINAL: Surface[] = [
  {
    id: 'terminal.new-tab-menu',
    area: 'terminal',
    title: 'New tab menu',
    source: { overlay: ['components/workspace/NewTabMenu.tsx'] },
    steps: [
      DASHBOARD,
      step('b-menu', [click(id('new-tab-menu-trigger'))], see('New terminal with'))
    ]
  },
  {
    id: 'terminal.open',
    area: 'terminal',
    title: 'Open a terminal from the tool strip',
    source: { screen: 'pane', name: 'terminal' },
    steps: [DASHBOARD, OPEN_TERMINAL]
  },
  {
    id: 'terminal.context-menu',
    area: 'terminal',
    title: 'Terminal context menu',
    source: { overlay: ['components/terminal/ConnectedTerminal.tsx'] },
    steps: [
      DASHBOARD,
      OPEN_TERMINAL,
      step('c-menu', [rclick(css('.xterm-screen'))], see('Explain with AI', 'Restart Terminal'))
    ]
  },
  {
    id: 'terminal.strip-shells',
    area: 'terminal',
    title: 'Tool strip shell menu',
    source: { overlay: ['components/shell/tool-strip/TerminalStripButton.tsx'] },
    steps: [DASHBOARD, step('b-menu', [rclick(label('New terminal'))], see('Default Shell'))]
  },
  {
    id: 'terminal.close-confirm',
    area: 'terminal',
    title: 'Close terminal confirm',
    source: { overlay: ['layouts/workspace-layout/WorkspaceCloseDialogs.tsx'] },
    steps: [
      DASHBOARD,
      OPEN_TERMINAL,
      step(
        'c-confirm',
        [click(label('Close tab'))],
        see('Close Terminal', "Don't ask again when closing terminals")
      )
    ]
  },
  {
    id: 'terminal.tab-menu',
    area: 'terminal',
    title: 'Tab context menu',
    source: { overlay: ['components/workspace/tab-context-menu.tsx'] },
    steps: [
      DASHBOARD,
      OPEN_TERMINAL,
      step('c-menu', [rclick(txt('Terminal 1'))], see('Close All Terminals', 'Close Other Tabs'))
    ]
  },
  {
    id: 'terminal.close-tabs',
    area: 'terminal',
    title: 'Close tabs confirm',
    source: { overlay: ['layouts/workspace-layout/WorkspaceCloseDialogs.tsx'] },
    steps: [
      DASHBOARD,
      OPEN_TERMINAL,
      step('c-menu', [rclick(txt('Terminal 1'))], see('Close All Tabs')),
      step('d-confirm', [click(txt('Close All Tabs'))], see('Close Tabs'))
    ]
  },
  {
    id: 'terminal.command-history',
    area: 'terminal',
    title: 'Command history',
    source: { overlay: ['components/CommandHistoryModal.tsx'] },
    steps: [
      DASHBOARD,
      step(
        'b-modal',
        [openPalette, click(txt('Command History'))],
        see('No command history yet', 'Clear History')
      )
    ]
  },
  {
    id: 'terminal.command-history-clear',
    area: 'terminal',
    title: 'Clear command history confirm',
    source: { overlay: ['components/CommandHistoryModal.tsx'] },
    prepare: seedCommandHistory,
    steps: [
      DASHBOARD,
      step('b-modal', [openPalette, click(txt('Command History'))], see(HISTORY_COMMAND)),
      step('c-confirm', [click(txt('Clear History'))], see('Clear Command History'))
    ]
  }
]
