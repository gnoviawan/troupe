import type { Surface } from '../types'
import { resetAlpha } from '../world'
import { click, css, DASHBOARD, label, press, see, step, txt, type } from './dsl'
import { openExplorer } from './explorer'

const OPEN_APP = [
  DASHBOARD,
  step('b-tree', [openExplorer, click(txt('src'))], see('app.ts')),
  step('c-open', [click(txt('app.ts'))], see('export function greet'))
]
const OPEN_GUIDE = [
  DASHBOARD,
  step('b-tree', [openExplorer, click(txt('docs'))], see('guide.md')),
  step('c-open', [click(txt('guide.md'))], see('Alpha guide', 'Setup'))
]

export const EDITOR: Surface[] = [
  {
    id: 'editor.code',
    area: 'editor',
    title: 'Code editor on a TypeScript file',
    source: { screen: 'pane', name: 'editor' },
    steps: OPEN_APP
  },
  {
    id: 'editor.markdown',
    area: 'editor',
    title: 'Markdown editor',
    source: { journey: 'starter' },
    steps: OPEN_GUIDE
  },
  {
    id: 'editor.go-to-line',
    area: 'editor',
    title: 'Go to line',
    source: { overlay: ['components/editor/find/GoToLineWidget.tsx'] },
    steps: [
      ...OPEN_APP,
      step('d-widget', [click(css('.cm-content')), press('ctrl-g')], see('Line or line:column'))
    ]
  },
  {
    id: 'editor.outline-depth',
    area: 'editor',
    title: 'Outline heading depth menu',
    source: { overlay: ['components/editor/outline/OutlineDepthMenu.tsx'] },
    steps: [
      ...OPEN_GUIDE,
      step('d-wide', [click(label('Close panel'))], see('On this page')),
      step('e-menu', [click(css('button[title="Outline depth"]'))], see('Show headings down to'))
    ]
  },
  {
    id: 'editor.unsaved-close',
    area: 'editor',
    title: 'Unsaved changes on close',
    source: { overlay: ['layouts/workspace-layout/WorkspaceCloseDialogs.tsx'] },
    prepare: resetAlpha,
    steps: [
      ...OPEN_APP,
      step('d-edit', [click(css('.cm-content')), type('x')], []),
      step('e-confirm', [click(label('Close tab'))], see('Unsaved Changes'))
    ]
  },
  {
    id: 'editor.app-close',
    area: 'editor',
    title: 'Unsaved changes on app close',
    source: { overlay: ['layouts/workspace-layout/WorkspaceCloseDialogs.tsx'] },
    reference: 'tauri',
    blocked:
      'opens only when the desktop window is closed with unsaved files; the web client has no window to close',
    steps: []
  }
]
