import { useTerminalStore } from '../../../../src/renderer/stores/terminal-store'
import { useWorkspaceStore } from '../../../../src/renderer/stores/workspace-store'
import type { Scenario, StoreRecorder, VectorEvent } from '../record'

const DATA_KEYS = [
  'activeProjectId',
  'parkedTrees',
  'root',
  'activePaneId',
  'fullscreenPaneId',
  'agentLauncherPaneId',
  'agentBrowserPaneId'
] as const

const term = (id: string) => ({ type: 'terminal', id: `term-${id}`, terminalId: id })
const edit = (path: string) => ({ type: 'editor', id: `edit-${path}`, filePath: path })
const chat = (session: string) => ({
  type: 'agent-chat',
  id: `chat-${session}`,
  sessionId: session,
  mountKey: `chat-${session}`
})
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
const ev = (action: string, ...args: unknown[]): VectorEvent => ({ action, args })
const q = (action: string, ...args: unknown[]): VectorEvent & { query: true } => ({
  action,
  args,
  query: true
})

/** Two panes side by side: `a` (terminal 1, x.ts) and `b` (y.ts). */
const twoPanes = {
  root: split(
    's1',
    'horizontal',
    [leaf('a', [term('1'), edit('/x.ts')], 'term-1'), leaf('b', [edit('/y.ts')], 'edit-/y.ts')],
    [50, 50]
  ),
  activePaneId: 'a'
}

/** `a` | (`b` over `c`). */
const threePanes = {
  root: split(
    's1',
    'horizontal',
    [
      leaf('a', [term('1')], 'term-1'),
      split(
        's2',
        'vertical',
        [leaf('b', [edit('/b.ts')], 'edit-/b.ts'), leaf('c', [chat('s9')], 'chat-s9')],
        [40, 60]
      )
    ],
    [30, 70]
  ),
  activePaneId: 'c'
}

const oneTab = { root: leaf('p', [term('1')], 'term-1'), activePaneId: 'p' }

export const workspaceScenarios: Scenario[] = [
  {
    name: 'split_nested_each_side',
    initial: oneTab,
    steps: [
      ev('splitPane', 'p', 'horizontal', edit('/r.ts')),
      ev('splitPane', 'p', 'vertical', edit('/t.ts'), 'top'),
      ev('splitPane', 'p', 'vertical', edit('/b.ts'), 'bottom'),
      ev('splitPane', 'missing', 'vertical', edit('/z.ts'))
    ]
  },
  {
    name: 'split_same_direction_inserts_sibling',
    initial: twoPanes,
    steps: [
      ev('splitPane', 'b', 'horizontal', edit('/n.ts'), 'right'),
      ev('splitPane', 'a', 'horizontal', edit('/m.ts'), 'left')
    ]
  },
  {
    name: 'split_custom_sizes',
    initial: twoPanes,
    steps: [
      ev('splitPane', 'a', 'vertical', edit('/v.ts'), 'bottom', { sizes: [70, 30] }),
      ev('splitPane', 'b', 'horizontal', edit('/h.ts'), 'right', { sizes: [1, 3] }),
      ev('splitPane', 'b', 'vertical', edit('/w.ts'), 'top', { sizes: [2, 1] }),
      ev('splitPane', 'b', 'vertical', edit('/bad.ts'), 'right', { sizes: [0, 5] })
    ]
  },
  {
    name: 'split_without_focus_keeps_fullscreen',
    initial: { ...twoPanes, fullscreenPaneId: 'a' },
    steps: [
      ev('splitPane', 'b', 'vertical', edit('/q.ts'), 'right', { focus: false }),
      ev('splitPane', 'a', 'vertical', edit('/f.ts'), 'right')
    ]
  },
  {
    name: 'add_tab_to_pane',
    initial: { ...twoPanes, agentLauncherPaneId: 'b' },
    steps: [
      ev('addTabToPane', 'b', edit('/new.ts')),
      ev('showAgentLauncher', 'a'),
      ev('addTabToPane', 'a', edit('/x.ts')),
      ev('addTabToPane', 'b', term('2'), { focus: false }),
      ev('togglePaneFullscreen', 'b'),
      ev('addTabToPane', 'a', term('3')),
      ev('addTabToPane', 'nope', term('4')),
      ev('hideAgentLauncher')
    ]
  },
  {
    name: 'move_tab_between_panes',
    initial: twoPanes,
    steps: [
      ev('moveTabToPane', 'term-1', 'a', 'b'),
      ev('moveTabToPane', 'edit-/x.ts', 'a', 'a'),
      ev('moveTabToPane', 'edit-/x.ts', 'a', 'b'),
      ev('moveTabToPane', 'ghost', 'b', 'b')
    ]
  },
  {
    name: 'move_tab_to_new_split',
    initial: threePanes,
    steps: [
      ev('moveTabToNewSplit', 'edit-/b.ts', 'b', 'c', 'right'),
      ev('moveTabToNewSplit', 'term-1', 'a', 'c', 'top'),
      ev('moveTabToNewSplit', 'chat-s9', 'c', 'id-2', 'center'),
      ev('moveTabToNewSplit', 'chat-s9', 'id-2', 'id-2', 'left')
    ]
  },
  {
    name: 'move_tab_to_stale_pane',
    initial: threePanes,
    steps: [ev('moveTabToNewSplit', 'chat-s9', 'c', 'gone', 'center')]
  },
  {
    name: 'move_tab_to_new_split_same_direction',
    initial: twoPanes,
    steps: [
      ev('moveTabToNewSplit', 'edit-/x.ts', 'a', 'b', 'right'),
      ev('moveTabToNewSplit', 'edit-/y.ts', 'b', 'a', 'left')
    ]
  },
  {
    name: 'close_tab_collapses_empty_pane',
    initial: threePanes,
    steps: [
      ev('closeTab', 'c', 'chat-s9'),
      ev('closeTab', 'a', 'missing'),
      ev('closeTab', 'b', 'edit-/b.ts'),
      ev('closeTab', 'a', 'term-1')
    ]
  },
  {
    name: 'close_active_middle_tab',
    initial: {
      root: leaf('p', [term('1'), term('2'), term('3')], 'term-2'),
      activePaneId: 'p'
    },
    steps: [
      ev('closeTab', 'p', 'term-2'),
      ev('closeTab', 'p', 'term-3'),
      ev('closeTab', 'p', 'term-1')
    ]
  },
  {
    name: 'set_active_tab_and_pane',
    initial: { ...threePanes, agentLauncherPaneId: 'c' },
    steps: [
      ev('setActiveTab', 'c', 'chat-s9'),
      ev('setActiveTab', 'b', 'edit-/b.ts'),
      ev('togglePaneFullscreen', 'a'),
      ev('setActivePane', 'b'),
      ev('setActivePane', 'a'),
      ev('setActiveTab', 'b', 'edit-/b.ts'),
      ev('clearFullscreenPane'),
      ev('setActivePane', 'b')
    ]
  },
  {
    name: 'toggle_fullscreen',
    initial: oneTab,
    steps: [
      ev('togglePaneFullscreen', 'p'),
      ev('splitPane', 'p', 'horizontal', edit('/r.ts')),
      ev('togglePaneFullscreen', 'p'),
      ev('togglePaneFullscreen', 'p'),
      ev('togglePaneFullscreen', 'id-2'),
      ev('collapsePane', 'p'),
      ev('togglePaneFullscreen', 'id-1')
    ]
  },
  {
    name: 'update_pane_sizes',
    initial: { ...threePanes, activeProjectId: 'p1' },
    steps: [
      ev('updatePaneSizes', 's2', [40.004, 59.996]),
      ev('updatePaneSizes', 's2', [25, 75]),
      ev('updatePaneSizes', 'a', [10, 90]),
      ev('updatePaneSizes', 's1', [50, 50], 'p1'),
      ev('updatePaneSizes', 's1', [20, 80], 'unknown'),
      ev('activateProject', 'p2'),
      ev('updatePaneSizes', 's2', [10, 90], 'p1'),
      ev('updatePaneSizes', 's2', [10, 90], 'p2')
    ]
  },
  {
    name: 'collapse_pane',
    initial: threePanes,
    steps: [ev('collapsePane', 's2'), ev('collapsePane', 'a'), ev('collapsePane', 'b')]
  },
  {
    name: 'collapse_pane_keeps_focus_on_sibling',
    initial: threePanes,
    steps: [ev('collapsePane', 'b'), ev('collapsePane', 'missing')]
  },
  {
    name: 'reorder_tabs',
    initial: { root: leaf('p', [term('1'), term('2'), term('3')], 'term-1'), activePaneId: 'p' },
    steps: [
      ev('reorderTabsInPane', 'p', ['term-3', 'ghost', 'term-1']),
      ev('reorderTabsInPane', 'p', [])
    ]
  },
  {
    name: 'terminal_tabs',
    initial: { ...twoPanes, agentLauncherPaneId: 'a' },
    steps: [
      ev('addTerminalTab', '1'),
      ev('addTerminalTab', '2', 'b'),
      ev('ensureTerminalTab', '2'),
      ev('ensureTerminalTab', '3'),
      ev('ensureTerminalTab', '4', 'a', true),
      ev('ensureTerminalTab', '5', 'missing'),
      ev('addTerminalTab', '1')
    ]
  },
  {
    name: 'editor_tabs',
    initial: { ...twoPanes, agentLauncherPaneId: 'b' },
    steps: [
      ev('addEditorTab', '/y.ts', 'b'),
      ev('addEditorTab', '/y.ts'),
      ev('addEditorTab', '/z.ts'),
      ev('renameEditorTab', '/z.ts', '/zz.ts'),
      ev('renameEditorTab', '/zz.ts', '/y.ts'),
      ev('renameEditorTab', '/none.ts', '/q.ts'),
      ev('clearEditorTabs')
    ]
  },
  {
    name: 'browser_and_agent_browser_tabs',
    initial: twoPanes,
    steps: [
      ev('addBrowserTab', 'b1'),
      ev('addBrowserTab', 'b1', 'b'),
      ev('openAgentBrowserTab', 'ag1', 'https://example.com'),
      ev('openAgentBrowserTab', 'ag2'),
      ev('openAgentBrowserTab', 'ag1'),
      ev('closeTab', 'id-1', 'browser-ag1'),
      ev('closeTab', 'id-1', 'browser-ag2'),
      ev('togglePaneFullscreen', 'b'),
      ev('openAgentBrowserTab', 'ag3')
    ]
  },
  {
    name: 'agent_browser_tab_fallback_to_first_leaf',
    initial: { ...twoPanes, activePaneId: 's1' },
    steps: [ev('openAgentBrowserTab', 'ag1')]
  },
  {
    name: 'agent_chat_tabs',
    initial: { ...threePanes, agentLauncherPaneId: 'b' },
    steps: [
      ev('addAgentChatTab', 's9'),
      ev('addAgentChatTab', 's9'),
      ev('setActivePane', 'a'),
      ev('addAgentChatTab', 's9'),
      ev('addAgentChatTab', 's10', 'b'),
      ev('insertAgentChatTab', 's10'),
      ev('insertAgentChatTab', 's11'),
      ev('remapAgentChatSession', 's11', 's11'),
      ev('remapAgentChatSession', 's10', 's12'),
      ev('remapAgentChatSession', 'gone', 's13'),
      ev('remapAgentChatSession', 's12', 's9')
    ]
  },
  {
    name: 'insert_chat_into_empty_pane',
    initial: { root: leaf('p', [], null), activePaneId: 'p' },
    steps: [ev('insertAgentChatTab', 's1'), ev('insertAgentChatTab', 's2')]
  },
  {
    name: 'git_and_github_tabs',
    initial: { ...twoPanes, agentLauncherPaneId: 'a' },
    steps: [
      ev('addGitTab', '/repo'),
      ev('addGitTab', '/repo', 'b'),
      ev('addGitHistoryTab', '/repo', 'b'),
      ev('addGitHistoryTab', '/repo'),
      ev('addGithubItemTab', {
        cwd: '/repo',
        repo: 'origin',
        kind: 'pr',
        number: 12,
        title: 'Fix it'
      }),
      ev('togglePaneFullscreen', 'a'),
      ev('addGithubItemTab', { cwd: '/repo', repo: 'origin', kind: 'pr', number: 12, title: 'x' })
    ]
  },
  {
    name: 'diff_tabs',
    initial: twoPanes,
    steps: [
      ev('addDiffTab', '/repo/', './src//a.ts', false),
      ev('addDiffTab', '/repo', 'src/a.ts', false, 'b'),
      ev('addDiffTab', 'C:\\Repo\\', 'src\\a.ts', true, 'b'),
      ev('addDiffTab', 'c:/repo', 'src/a.ts', true),
      ev('addCommitDiffTab', '/repo', 'e44b917abc', 'src/b.ts'),
      ev('addCommitDiffTab', '/repo', 'e44b917abc', 'src/c.ts', 'src/old.ts', 'b'),
      ev('addCommitDiffTab', '/repo/./x/..', 'e44b917abc', '/src/b.ts'),
      ev('addDiffTab', '//?/C:/Work/../Repo', 'naïve ✓.ts', false),
      ev('addDiffTab', '//srv/share//p/', '"q".ts', false)
    ]
  },
  {
    name: 'canvas_and_automations_tabs',
    initial: { ...twoPanes, activeProjectId: 'p1' },
    steps: [
      ev('addCanvasTab', 'p1', 'design.figcraft'),
      ev('addCanvasTab', 'p1', 'other.figcraft', 'b'),
      ev('addAutomationsTab', 'p1', 'b'),
      ev('addAutomationsTab', 'p1'),
      ev('addAutomationsTab', 'p2')
    ]
  },
  {
    name: 'remove_tab_visible_and_parked',
    initial: { ...twoPanes, activeProjectId: 'p1' },
    steps: [
      ev('activateProject', 'p2'),
      ev('addTerminalTab', '7'),
      ev('removeTab', 'term-1'),
      ev('removeTab', 'term-7'),
      ev('removeTab', 'ghost'),
      ev('activateProject', 'p1')
    ]
  },
  {
    name: 'sync_terminal_tabs',
    initial: {
      root: split(
        's1',
        'horizontal',
        [
          leaf('a', [term('1'), term('2'), edit('/x.ts')], 'term-2'),
          leaf('b', [term('3')], 'term-3')
        ],
        [50, 50]
      ),
      activePaneId: 'a',
      fullscreenPaneId: 'b'
    },
    steps: [
      ev('syncTerminalTabs', ['1', '2', '3'], ['1', '2', '3']),
      ev('syncTerminalTabs', ['1'], ['1', '2']),
      ev('syncTerminalTabs', ['1', '4', '5'], ['1', '4', '5']),
      ev('syncTerminalTabs', [], [])
    ]
  },
  {
    name: 'clear_pane_and_reset',
    initial: twoPanes,
    steps: [ev('clearPane', 'b'), ev('clearPane', 'b'), ev('clearPane', 's1'), ev('resetLayout')]
  },
  {
    name: 'load_project_workspace_normalizes',
    initial: { ...twoPanes, fullscreenPaneId: 'b' },
    steps: [
      ev(
        'loadProjectWorkspace',
        split(
          'r',
          'horizontal',
          [
            leaf('x', [term('1')], 'term-1'),
            split(
              'n',
              'horizontal',
              [leaf('y', [], null), split('e', 'vertical', [], [])],
              [30, 70]
            ),
            split('z', 'vertical', [leaf('w', [], null)], [0])
          ],
          [20, -1, 50]
        ),
        'y'
      ),
      ev('loadProjectWorkspace', leaf('solo', [], null), 'gone'),
      ev('loadProjectWorkspace', split('empty', 'vertical', [], []), null)
    ]
  },
  {
    name: 'sync_editor_tabs',
    initial: {
      root: split(
        's1',
        'horizontal',
        [
          leaf('a', [edit('/old.ts'), term('1'), chat('s1')], 'edit-/old.ts'),
          leaf('b', [edit('/y.ts')], 'edit-/y.ts')
        ],
        [50, 50]
      ),
      activePaneId: 'a'
    },
    steps: [
      ev('syncEditorTabs', ['/a.ts', '/b.ts', '/a.ts'], 'edit-/b.ts'),
      ev('syncEditorTabs', ['/c.ts'], 'edit-/missing.ts'),
      ev('syncEditorTabs', [])
    ]
  },
  {
    name: 'remap_terminal_tabs',
    initial: {
      root: split(
        's1',
        'horizontal',
        [
          leaf('a', [term('old1'), term('new1'), edit('/x.ts')], 'term-old1'),
          leaf('b', [term('old2')], 'term-old2')
        ],
        [50, 50]
      ),
      activePaneId: 'b'
    },
    steps: [
      ev('remapTerminalTabs', {}),
      ev('remapTerminalTabs', { '': 'x', old3: '' }),
      ev('remapTerminalTabs', { old1: 'new1', old2: 'new2' })
    ]
  },
  {
    name: 'next_tab_and_active_tab_queries',
    initial: { root: leaf('p', [term('1'), term('2'), term('3')], 'term-3'), activePaneId: 'p' },
    steps: [
      q('getNextTabId', 1),
      q('getNextTabId', -1),
      q('getActiveTab'),
      q('getActivePaneLeaf'),
      ev('setActiveTab', 'p', 'ghost'),
      q('getNextTabId', 1),
      q('getActiveTab'),
      ev('clearPane', 'p'),
      q('getNextTabId', 1),
      ev('setActivePane', 'nowhere'),
      q('getActivePaneLeaf')
    ]
  },
  {
    name: 'project_trees',
    initial: { ...twoPanes, activeProjectId: 'p1' },
    steps: [
      ev('activateProject', 'p1'),
      q('hasProjectTree', 'p1'),
      ev('activateProject', 'p2'),
      q('hasProjectTree', 'p1'),
      q('getProjectTree', 'p1'),
      q('getProjectTree', 'p2'),
      q('getProjectTree', 'p3'),
      ev('addTabToParkedPane', 'p1', 'b', term('9')),
      ev('addTabToParkedPane', 'p1', 'b', term('9')),
      ev('addTabToParkedPane', 'p1', 's1', term('9')),
      ev('addTabToParkedPane', 'p2', 'id-1', term('9')),
      ev('addTabToParkedPane', 'p9', 'b', term('9')),
      ev('activateProject', 'p1'),
      ev('activateProject', 'p1'),
      ev('activateProject', 'p1', {
        root: leaf('given', [], null),
        activePaneId: 'given',
        fullscreenPaneId: null,
        agentLauncherPaneId: null,
        agentBrowserPaneId: null
      }),
      ev('activateProject', 'p3', {
        root: leaf('seeded', [term('5')], 'term-5'),
        activePaneId: 'seeded',
        fullscreenPaneId: null,
        agentLauncherPaneId: 'seeded',
        agentBrowserPaneId: null
      }),
      ev('dropProjectTree', 'p3'),
      ev('dropProjectTree', 'p2'),
      q('hasProjectTree', 'p2')
    ]
  }
]

const INITIAL = {
  activeProjectId: '',
  parkedTrees: {},
  root: leaf('pane-0', [], null),
  activePaneId: 'pane-0',
  fullscreenPaneId: null,
  agentLauncherPaneId: null,
  agentBrowserPaneId: null
}

export const workspaceRecorder: StoreRecorder = {
  store: 'workspace',
  sources: [
    'src/renderer/stores/workspace-store/tree.ts',
    'src/renderer/stores/workspace-store/slices/pane.ts',
    'src/renderer/stores/workspace-store/slices/projects.ts',
    'src/renderer/stores/workspace-store/slices/sync.ts',
    'src/renderer/stores/workspace-store/slices/tabs.ts',
    'src/renderer/lib/diff-tab-identity.ts',
    'src/renderer/lib/working-copy.ts'
  ],
  reset() {
    useWorkspaceStore.setState(structuredClone(INITIAL))
    useTerminalStore.setState({ terminals: [] })
  },
  seed(initial) {
    useWorkspaceStore.setState(structuredClone(initial))
  },
  snapshot() {
    const state = useWorkspaceStore.getState()
    return Object.fromEntries(DATA_KEYS.map((key) => [key, state[key]]))
  },
  dispatch({ action, args }) {
    // The terminal store's ids are an input of the sync, not workspace state.
    if (action === 'syncTerminalTabs') {
      const known = args[1] as string[]
      useTerminalStore.setState({
        terminals: known.map((id) => ({ id }) as never)
      })
    }
    const fn = (useWorkspaceStore.getState() as unknown as Record<string, unknown>)[action]
    if (typeof fn !== 'function') throw new Error(`workspace store has no action ${action}`)
    return fn(...args)
  }
}
