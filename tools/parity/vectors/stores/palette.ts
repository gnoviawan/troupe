import { defaultFilter } from 'cmdk'
import {
  type ChatCardData,
  filterChatCards
} from '../../../../src/renderer/components/shell/chat-cards'
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
    'src/renderer/components/CommandPalette.tsx',
    'src/renderer/components/CommandPaletteChats.tsx',
    'src/renderer/components/CommandPaletteQuickChats.tsx',
    'src/renderer/components/shell/chat-cards.ts'
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
    if (action === 'commandScore') {
      return defaultFilter(
        args[0] as string,
        args[1] as string,
        (args[2] as string[] | undefined) ?? []
      )
    }
    if (action === 'filterChatCards') {
      return filterChatCards(args[0] as ChatCardData[], args[1] as string).map(
        (card) => card.entry.id
      )
    }
    const recent = useRecentCommandsStore.getState() as unknown as Record<string, unknown>
    const pinned = usePinnedCommandsStore.getState() as unknown as Record<string, unknown>
    const fn = (recent[action] ?? pinned[action]) as ((...a: unknown[]) => unknown) | undefined
    if (typeof fn !== 'function') throw new Error(`palette has no action ${action}`)
    return fn(...args)
  }
}

const ev = (action: string, ...args: unknown[]): VectorEvent => ({ action, args })
const score = (
  value: string,
  search: string,
  keywords?: string[]
): VectorEvent & { query: true } => ({
  action: 'commandScore',
  args: keywords ? [value, search, keywords] : [value, search],
  query: true
})
const card = (
  id: string,
  title: string,
  agentName: string | null,
  indexModel: string | null,
  branch: string
) => ({ entry: { id, title }, agentName, indexModel, branch }) as unknown as ChatCardData
/** Cards as `buildChatCards` makes them, with only the fields the filter reads. */
const CARDS = [
  card('c1', 'Fix the login redirect', 'Claude Code', 'Opus 4.5', 'main'),
  card('c2', 'Port settings', null, null, 'rune/r9c'),
  card('c3', 'Untitled Chat', 'Codex', null, ''),
  card('c4', 'Review PR', null, 'gpt-5', 'feature/LOGIN')
]
const CHAT_QUERIES = ['', 'login', 'LOGIN', 'agent', 'codex', 'r9c', 'gpt', ' opus ', 'zzz']

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
    name: 'chat_filter',
    steps: CHAT_QUERIES.map((query) => ({
      action: 'filterChatCards',
      args: [CARDS, query],
      query: true as const
    }))
  },
  {
    name: 'chat_item_scores',
    steps: [
      score('chat:c1', 'login', ['Fix the login redirect', 'Claude Code', 'main', 'Opus 4.5']),
      score('chat:c2', 'r9c', ['Port settings', 'rune/r9c', 'Agent']),
      score('chat:c4', 'gpt', ['Review PR', 'feature/LOGIN', 'gpt-5']),
      score('chat:c3', 'agent', ['Untitled Chat', 'Codex']),
      score('Fix the login redirect quick-chat:q1', 'login', [
        'Fix the login redirect',
        'quick chat'
      ]),
      score('Notes quick-chat:q2', 'no', ['Notes', 'quick chat']),
      score('Plan quick-chat:q3', 'quick', ['Plan', 'quick chat'])
    ]
  },
  {
    name: 'command_scores',
    steps: ROWS.flatMap((row) => QUERIES.map((query) => score(row, query)))
  }
]
