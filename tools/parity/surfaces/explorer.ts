import type { Surface } from '../types'
import { resetAlpha } from '../world'
import { click, css, DASHBOARD, label, rclick, see, step, txt } from './dsl'

export const openExplorer = click(label('Explorer'))
const TREE = step('b-tree', [openExplorer], see('README.md'))

export const EXPLORER: Surface[] = [
  {
    id: 'explorer.tree',
    area: 'explorer',
    title: 'File tree',
    source: { screen: 'side-panel', name: 'explorer' },
    steps: [DASHBOARD, step('b-tree', [openExplorer], see('src', 'docs', 'README.md', 'notes.txt'))]
  },
  {
    id: 'explorer.expand',
    area: 'explorer',
    title: 'Expand a folder',
    source: { journey: 'starter' },
    steps: [
      DASHBOARD,
      step('b-tree', [openExplorer], see('src')),
      step('c-expanded', [click(txt('src'))], see('app.ts', 'utils.ts'))
    ]
  },
  {
    id: 'explorer.context-menu',
    area: 'explorer',
    title: 'File context menu',
    source: { overlay: ['components/file-explorer/FileTreeContextMenu.tsx'] },
    steps: [DASHBOARD, TREE, step('c-menu', [rclick(txt('README.md'))], see('Rename', 'Delete'))]
  },
  {
    id: 'explorer.delete-confirm',
    area: 'explorer',
    title: 'Delete file confirm',
    source: { overlay: ['components/file-explorer/FileExplorer.tsx'] },
    prepare: resetAlpha,
    steps: [
      DASHBOARD,
      TREE,
      step('c-menu', [rclick(txt('README.md'))], see('Delete')),
      step(
        'd-confirm',
        [click(css('[role="menuitem"]:has-text("Delete")'))],
        see('Delete file', 'This cannot be undone.')
      )
    ]
  },
  {
    id: 'explorer.more-menu',
    area: 'explorer',
    title: 'Explorer more-actions menu',
    source: { overlay: ['components/file-explorer/explorer-header.tsx'] },
    steps: [
      DASHBOARD,
      TREE,
      step('c-menu', [click(label('More explorer actions'))], see('Refresh'))
    ]
  }
]
