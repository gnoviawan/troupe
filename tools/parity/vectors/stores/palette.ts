import { defaultFilter } from 'cmdk'
import { usePinnedCommandsStore } from '../../../../src/renderer/stores/pinned-commands-store'
import { useRecentCommandsStore } from '../../../../src/renderer/stores/recent-commands-store'
import type { Scenario, StoreRecorder, VectorEvent } from '../record'

/**
 * The command palette's own state: the recent and pinned command lists, and the score cmdk
 * gives a row's searchable value for a query (`commandScore`, which filters and orders the
 * rows). The palette's rows and groups are `CommandPalette.tsx`.
 */
export const paletteRecorder: StoreRecorder = {
  store: 'palette',
  sources: [
    'src/renderer/stores/recent-commands-store.ts',
    'src/renderer/stores/pinned-commands-store.ts',
    'src/renderer/components/CommandPalette.tsx'
  ],
  reset() {
    useRecentCommandsStore.setState({ recentCommandIds: [] })
    usePinnedCommandsStore.setState({ pinnedCommandIds: [] })
  },
  seed() {},
  snapshot() {
    return {
      recentCommandIds: useRecentCommandsStore.getState().recentCommandIds,
      pinnedCommandIds: usePinnedCommandsStore.getState().pinnedCommandIds
    }
  },
  dispatch({ action, args }) {
    if (action === 'commandScore') return defaultFilter(args[0] as string, args[1] as string, [])
    const recent = useRecentCommandsStore.getState() as unknown as Record<string, unknown>
    const pinned = usePinnedCommandsStore.getState() as unknown as Record<string, unknown>
    const fn = (recent[action] ?? pinned[action]) as ((...a: unknown[]) => unknown) | undefined
    if (typeof fn !== 'function') throw new Error(`palette has no action ${action}`)
    return fn(...args)
  }
}

const ev = (action: string, ...args: unknown[]): VectorEvent => ({ action, args })
const score = (value: string, search: string): VectorEvent & { query: true } => ({
  action: 'commandScore',
  args: [value, search],
  query: true
})

/** Searchable values as `getSearchableValue` joins them: label, description, category, keywords. */
const ROWS = [
  'App Preferences Open global application preferences navigation Navigation preferences prefs settings app global',
  "Open Canvas Open the project's .figcraft design document as a canvas tab workspace Workspace canvas figcraft design drawing vector",
  'Project Settings Configure the active project workspace navigation Navigation settings project configure config',
  'New Terminal Open a new shell in the active pane workspace Workspace shell console pty workspace',
  'Change Color Theme Preview and apply a UI color theme tools Tools theme color appearance palette dark dracula nord',
  'alpha /tmp/w/alpha projects Projects project switch alpha /tmp/w/alpha',
  'Command History Review and reuse recent terminal commands tools Tools history recent terminal commands shell',
  'New chat Start a new chat in the active pane workspace Workspace agent ai claude codex prompt chat new'
]
const QUERIES = [
  '',
  'App Pref',
  'app',
  'theme',
  'color theme',
  'nt',
  'alpha',
  'zzz',
  'canvas',
  'Settings',
  'his',
  'new-term'
]

export const paletteScenarios: Scenario[] = [
  {
    name: 'recent_keeps_five_newest_first',
    steps: [
      ev('addRecentCommand', 'a'),
      ev('addRecentCommand', 'b'),
      ev('addRecentCommand', 'a'),
      ev('addRecentCommand', 'c'),
      ev('addRecentCommand', 'd'),
      ev('addRecentCommand', 'e'),
      ev('addRecentCommand', 'f'),
      ev('setRecentCommands', ['x', 'y'])
    ]
  },
  {
    name: 'pins_toggle_in_order',
    steps: [
      ev('togglePinned', 'a'),
      ev('togglePinned', 'b'),
      ev('togglePinned', 'a'),
      ev('togglePinned', 'c'),
      ev('setPinned', ['z'])
    ]
  },
  {
    name: 'command_scores',
    steps: ROWS.flatMap((row) => QUERIES.map((query) => score(row, query)))
  }
]
