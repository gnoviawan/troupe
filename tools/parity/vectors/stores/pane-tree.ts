import {
  deserializePaneTree,
  reconcileTerminalTabs,
  serializePaneTree
} from '../../../../src/renderer/hooks/editor-persistence/pane-tree'
import type { PersistedPaneNodeInput } from '../../../../src/renderer/hooks/editor-persistence/types'
import type { PaneNode } from '../../../../src/renderer/types/workspace.types'
import type { Scenario, StoreRecorder, VectorEvent } from '../record'

/**
 * The workspace record's pane tree (`hooks/editor-persistence/pane-tree.ts`): pure
 * functions, so the "store" has no state and each step's result is the function's.
 */
export const paneTreeRecorder: StoreRecorder = {
  store: 'pane-tree',
  sources: [
    'src/renderer/hooks/editor-persistence/pane-tree.ts',
    'src/renderer/lib/workspace-tab-kinds.ts'
  ],
  reset() {},
  seed() {},
  snapshot: () => ({}),
  dispatch({ action, args }: VectorEvent): unknown {
    switch (action) {
      case 'serializePaneTree':
        return serializePaneTree(args[0] as PaneNode)
      case 'deserializePaneTree':
        return deserializePaneTree(args[0] as PersistedPaneNodeInput)
      case 'reconcileTerminalTabs':
        return reconcileTerminalTabs(
          args[0] as PaneNode,
          new Set(args[1] as string[]),
          args[2] as Parameters<typeof reconcileTerminalTabs>[2],
          args[3] as Parameters<typeof reconcileTerminalTabs>[3]
        )
      default:
        throw new Error(`no pane-tree function ${action}`)
    }
  }
}

const ev = (action: string, ...args: unknown[]): VectorEvent => ({ action, args })
const leaf = (id: string, tabs: unknown[], activeTabId: string | null = null) => ({
  type: 'leaf',
  id,
  tabs,
  activeTabId
})
const split = (id: string, direction: string, children: unknown[], sizes: number[]) => ({
  type: 'split',
  id,
  direction,
  children,
  sizes
})
const term = (id: string) => ({ type: 'terminal', id: `term-${id}`, terminalId: id })
const edit = (path: string) => ({ type: 'editor', id: `edit-${path}`, filePath: path })
const USER_BROWSER = 'tab-0123abcd-4567-89ab-cdef-0123456789ab'

const everyKind = [
  edit('/p/a.ts'),
  term('t1'),
  { type: 'browser', id: `browser-${USER_BROWSER}`, browserTabId: USER_BROWSER },
  { type: 'git', id: 'git-/p', cwd: '/p' },
  { type: 'agent-chat', id: 'chat-s1', sessionId: 's1', mountKey: 'chat-s1' },
  { type: 'git-history', id: 'git-history-/p', cwd: '/p' },
  {
    type: 'github-item',
    id: 'github-item-/p-origin-7',
    cwd: '/p',
    repo: 'origin',
    kind: 'pr',
    number: 7,
    title: 'Fix it'
  },
  { type: 'diff', id: 'diff:x', cwd: '/p', path: 'a.ts', staged: false },
  { type: 'commit-diff', id: 'commit:y', cwd: '/p', commit: 'abc', path: 'b.ts', oldPath: 'c.ts' },
  { type: 'automations', id: 'automations-p1', projectId: 'p1' },
  { type: 'canvas', id: 'canvas-p1', projectId: 'p1', docPath: '/p/design.figcraft' }
]

const live = (id: string, name: string, shell = 'bash', cwd = '/p') => ({
  id,
  name,
  shell,
  cwd,
  projectId: 'p1'
})

export const paneTreeScenarios: Scenario[] = [
  {
    name: 'serialize_every_kind_and_splits',
    steps: [
      ev('serializePaneTree', leaf('l', everyKind, 'edit-/p/a.ts')),
      ev(
        'serializePaneTree',
        split(
          's',
          'vertical',
          [
            leaf('a', [term('t1')], 'term-t1'),
            split('t', 'horizontal', [leaf('b', []), leaf('c', [edit('/x')])], [25, 75])
          ],
          [40, 60]
        )
      )
    ]
  },
  {
    name: 'deserialize_every_kind',
    steps: [
      ev(
        'deserializePaneTree',
        leaf(
          'l',
          [
            { type: 'editor', filePath: '/p/a.ts' },
            { type: 'terminal', terminalId: 't1' },
            { type: 'browser', browserTabId: USER_BROWSER },
            { type: 'browser', browserTabId: 'agent-session-tab', url: 'https://example.com' },
            { type: 'git', id: 'git-/p', cwd: '/p' },
            { type: 'agent-chat', id: 'chat-s1', sessionId: 's1' },
            { type: 'git-history', id: 'git-history-/p', cwd: '/p' },
            {
              type: 'github-item',
              id: 'stale',
              cwd: '/p',
              repo: 'upstream',
              kind: 'issue',
              number: 12
            },
            { type: 'diff', cwd: '/p', path: 'src\\a.ts', staged: true },
            { type: 'diff', id: 'diff:kept', cwd: '/p', path: 'a.ts', staged: false },
            { type: 'commit-diff', cwd: '/p', commit: 'abc', path: 'b.ts', oldPath: 'c.ts' },
            { type: 'automations', id: 'ignored', projectId: 'p1' }
          ],
          'git-/p'
        )
      )
    ]
  },
  {
    name: 'deserialize_drops_corrupt_tabs',
    steps: [
      ev(
        'deserializePaneTree',
        leaf(
          'l',
          [
            { type: 'agent-chat', id: 'chat-launch-1', sessionId: 'launch-1' },
            { type: 'agent-chat', id: 'chat-x' },
            { type: 'automations', id: 'a' },
            { type: 'diff', cwd: '/p', path: 'a.ts', staged: 'yes' },
            { type: 'commit-diff', cwd: '/p', path: 'a.ts' },
            { type: 'github-item', cwd: '/p', repo: 'fork', kind: 'pr', number: 3 },
            { type: 'github-item', cwd: '/p', repo: 'origin', kind: 'pr', number: 1.5 },
            { type: 'hologram\u0007-with-a-very-long-kind-name-that-keeps-going', id: 'h' },
            { type: 42 },
            { id: 'no-type' },
            { type: 'git', id: 'git-/q', cwd: '/q' }
          ],
          'chat-launch-1'
        )
      )
    ]
  },
  {
    name: 'deserialize_legacy_leaf_and_split',
    steps: [
      ev('deserializePaneTree', {
        type: 'leaf',
        id: 'old',
        editorFilePaths: ['/a.md', '/b.md'],
        activeTabId: 'edit-/b.md'
      }),
      ev(
        'deserializePaneTree',
        split(
          's',
          'horizontal',
          [leaf('a', [{ type: 'editor', filePath: '/a.md' }], null), leaf('b', [], null)],
          [30, 70]
        )
      )
    ]
  },
  {
    name: 'reconcile_terminals_by_id_then_name',
    steps: [
      ev(
        'reconcileTerminalTabs',
        split(
          's',
          'horizontal',
          [
            leaf(
              'a',
              [term('t1'), term('old-2'), edit('/open.ts'), edit('/closed.ts')],
              'term-old-2'
            ),
            leaf('b', [term('old-3'), term('old-9')], 'term-old-9')
          ],
          [50, 50]
        ),
        ['/open.ts'],
        [live('t1', 'Terminal 1'), live('n2', 'Build', 'zsh', '/q'), live('n3', 'Logs', 'bash')],
        {
          activeTerminalId: null,
          updatedAt: '2026-01-01T00:00:00.000Z',
          terminals: [
            { id: 'old-2', name: 'Build', shell: 'zsh', cwd: '/elsewhere' },
            { id: 'old-3', name: 'Logs', shell: 'fish' },
            { id: 'old-9', name: 'Gone', shell: 'bash' }
          ]
        }
      )
    ]
  },
  {
    name: 'reconcile_keeps_saved_terminals_when_none_live',
    steps: [
      ev(
        'reconcileTerminalTabs',
        leaf(
          'a',
          [term('t1'), { type: 'canvas', id: 'canvas-p', projectId: 'p', docPath: '/d' }],
          'canvas-p'
        ),
        [],
        [],
        {
          activeTerminalId: 't1',
          updatedAt: '2026-01-01T00:00:00.000Z',
          terminals: [{ id: 't1', name: 'Terminal 1', shell: 'bash' }]
        }
      ),
      ev(
        'reconcileTerminalTabs',
        split('s', 'vertical', [leaf('a', [term('t1')], 'term-t1'), leaf('b', [], null)], [0, -5]),
        [],
        [],
        null
      )
    ]
  }
]
