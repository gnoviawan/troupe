import {
  resetShellLayoutStoreForTests,
  useShellLayoutStore
} from '../../../../src/renderer/stores/shell-layout-store'
import { readFailures, reads, type Scenario, type StoreRecorder, type VectorEvent } from '../record'

const KEY = 'shell-layout'

const ev = (action: string, ...args: unknown[]): VectorEvent => ({ action, args })

/**
 * `hydrate`'s argument is what the host read returned: `{ data }` for a saved
 * record, `{ code }` for a failed read, `null` for no record yet (the migration
 * path, with no legacy sources on this device).
 */
type ReadOutcome = { data: unknown } | { code: string } | null
const hydrate = (outcome: ReadOutcome) => ev('hydrate', outcome)

const SAVED = {
  version: 3,
  railExpanded: true,
  railWidth: 260,
  sidePanelVisible: true,
  sidePanelWidth: 300,
  sshSectionVisible: true,
  sshSectionHeight: 140,
  gitHistoryDetailWidth: 400,
  lastColumnTabByProject: { p1: 'changes', p2: 'github' },
  worktreePickByProject: { p1: '/repo/wt' },
  chatsHiddenByProject: { p1: true }
}

/** Mirrors the cases of `stores/shell-layout-store.test.ts` that need no legacy source. */
export const shellLayoutScenarios: Scenario[] = [
  {
    name: 'gate_then_fresh_install',
    steps: [
      ev('setRailExpanded', false),
      ev('setSidePanelWidth', 400),
      ev('removeProject', 'p1'),
      hydrate(null),
      ev('setSidePanelVisible', true),
      ev('setSidePanelWidth', 9999),
      ev('setSidePanelWidth', 100.4),
      ev('setRailWidth', 250.6),
      ev('setRailWidth', 'wide'),
      ev('setRailExpanded', 'yes'),
      ev('setRailExpanded', false)
    ]
  },
  {
    name: 'load_existing_record_clamped',
    steps: [
      hydrate({
        data: {
          version: 3,
          railExpanded: 'no',
          railWidth: 500,
          sidePanelVisible: true,
          sidePanelWidth: 300.2,
          sshSectionHeight: Number.NaN,
          lastColumnTabByProject: { p1: 'changes', p2: 'ssh', p3: 'chats', '': 'explorer' },
          worktreePickByProject: { p1: '/x', p2: '' },
          chatsHiddenByProject: { p1: true, p2: false },
          extra: 1
        }
      }),
      ev('setSidePanelVisible', false)
    ]
  },
  {
    name: 'version_1_record_is_saved_as_current',
    steps: [
      hydrate({ data: { version: 1, graphHeight: 200, railExpanded: false, sidePanelWidth: 260 } })
    ]
  },
  {
    name: 'version_2_chats_view_becomes_explorer',
    steps: [
      hydrate({
        data: {
          version: 2,
          sidePanelVisible: true,
          sidePanelWidth: 420,
          lastColumnTabByProject: { p1: 'chats', p2: 'github' }
        }
      })
    ]
  },
  {
    name: 'non_object_record_loads_defaults',
    steps: [hydrate({ data: 'garbage' })]
  },
  {
    name: 'read_error_goes_unavailable',
    steps: [hydrate({ code: 'IO_ERROR' }), ev('setRailExpanded', false), ev('setRailWidth', 300)]
  },
  {
    name: 'newer_version_goes_unavailable',
    steps: [hydrate({ data: { version: 4, railWidth: 300 } }), ev('setSidePanelVisible', true)]
  },
  {
    name: 'tabs_picks_and_sections',
    steps: [
      hydrate(null),
      ev('setLastColumnTab', 'p1', 'changes'),
      ev('setLastColumnTab', 'p1', 'ssh'),
      ev('setLastColumnTab', '', 'explorer'),
      ev('setLastColumnTab', '__proto__', 'github'),
      ev('setLastColumnTab', 'p2', 'github'),
      ev('setWorktreePick', 'p1', '/repo/./wt/'),
      ev('setWorktreePick', 'p2', ''),
      ev('setWorktreePick', 'p1', null),
      ev('setWorktreePick', '__proto__', '/x'),
      ev('setProjectChatsHidden', 'p1', true),
      ev('setProjectChatsHidden', 'p2', true),
      ev('setProjectChatsHidden', 'p1', false),
      ev('setSshSectionVisible', false),
      ev('setSshSectionVisible', 1),
      ev('setSshSectionHeight', 10),
      ev('setGitHistoryDetailWidth', 1000)
    ]
  },
  {
    name: 'remove_project_when_ready',
    steps: [hydrate({ data: SAVED }), ev('removeProject', 'p1'), ev('removeProject', 'p9')]
  },
  {
    name: 'remove_project_before_hydration',
    steps: [ev('removeProject', 'p1'), hydrate({ data: SAVED })]
  },
  {
    name: 'remove_project_before_fresh_install',
    steps: [ev('removeProject', 'p1'), hydrate(null)]
  },
  {
    name: 'remove_project_while_unavailable',
    steps: [ev('removeProject', 'p2'), hydrate({ code: 'IO_ERROR' }), ev('removeProject', 'p1')]
  }
]

export const shellLayoutRecorder: StoreRecorder = {
  store: 'shell-layout',
  sources: [
    'src/renderer/stores/shell-layout-store.ts',
    'src/renderer/lib/shell-layout-record.ts',
    'src/renderer/lib/shell-layout-migration.ts'
  ],
  reset() {
    resetShellLayoutStoreForTests()
    reads.delete(KEY)
    readFailures.delete(KEY)
  },
  seed(initial) {
    useShellLayoutStore.setState(structuredClone(initial))
  },
  snapshot() {
    const { status, ...rest } = useShellLayoutStore.getState()
    const record = Object.fromEntries(
      Object.entries(rest).filter(([, value]) => typeof value !== 'function')
    )
    return { ...record, status }
  },
  async dispatch({ action, args }) {
    const store = useShellLayoutStore.getState()
    if (action === 'hydrate') {
      const outcome = args[0] as ReadOutcome
      reads.delete(KEY)
      readFailures.delete(KEY)
      if (outcome && 'data' in outcome) reads.set(KEY, outcome.data)
      if (outcome && 'code' in outcome) readFailures.set(KEY, outcome.code)
      return store.hydrate()
    }
    const method = store[action as keyof typeof store]
    if (typeof method !== 'function') throw new Error(`shell-layout has no action ${action}`)
    return (method as (...a: unknown[]) => unknown)(...args)
  }
}
