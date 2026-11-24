// The chat index snapshot for `acp.ts`: `applySessionIndexSnapshot` merges
// the host's rows with the local projection. Each restates a case of
// `history-session.test.ts`, `persistence.test.ts`, `chat-tab-prune.test.ts`
// or `continued-from.test.ts`.
import type { Scenario } from '../record'
import { ev, idle, S, seeded } from './acp-fixtures'

const row = (id: string, extra: Record<string, unknown> = {}) => ({
  id,
  title: `Chat ${id}`,
  agentId: 'agent-1',
  projectId: 'p1',
  cwd: '/work',
  createdAt: 100,
  lastActivityAt: 10,
  messageCount: 1,
  ...extra
})

const apply = (...rows: unknown[]) => ev('applySessionIndexSnapshot', rows)

export const indexScenarios: Scenario[] = [
  {
    name: 'index_snapshot_on_an_empty_index_takes_the_host_rows',
    initial: seeded({ title: 'Untitled Chat 3' }),
    steps: [apply(row(S, { title: 'Fix the build' }), row('s2')), apply(row('s2'))]
  },
  {
    name: 'index_snapshot_keeps_a_newer_local_row',
    initial: seeded(idle, {
      sessionIndex: [
        row(S, {
          title: 'Renamed',
          lastActivityAt: 20,
          messageCount: 5,
          lastSeq: 3,
          createdAt: 200,
          continuedFrom: 'local-parent',
          turnActive: false
        }),
        row('s2', { title: 'Untitled Chat 2', lastActivityAt: 30, agentConfigId: 'cfg-l' }),
        row('s3', { title: 'Same time', lastActivityAt: 10, kind: 'quick' })
      ]
    }),
    steps: [
      apply(
        row(S, {
          title: 'Host',
          messageCount: 7,
          lastSeq: 9,
          agentConfigId: 'cfg-h',
          continuedFrom: 'host-parent',
          startedBy: 'automation',
          kind: 'chat',
          turnActive: true
        }),
        row('s2', { title: 'Host title', createdAt: 0 }),
        row('s3', { title: 'Host same time', kind: 'chat', createdAt: null, agentConfigId: null })
      )
    ]
  },
  {
    name: 'index_snapshot_newer_host_row_keeps_what_the_host_cannot_know',
    initial: seeded(idle, {
      sessionIndex: [
        row(S, { title: 'Fix the bug', turnActive: true, continuedFrom: 'parent' }),
        row('s2', { title: 'Local', turnActive: true }),
        row('s3', { title: 'Local three' })
      ]
    }),
    steps: [
      apply(
        row(S, { title: 'Untitled Chat', lastActivityAt: 50 }),
        row('s2', { title: 'Host', lastActivityAt: 50, turnActive: false }),
        row('s3', { title: 'Host three', lastActivityAt: 50 })
      )
    ]
  },
  {
    name: 'index_snapshot_keeps_a_live_row_the_host_lacks',
    initial: {
      ...seeded({ activeTurn: false, openTurnId: 'turn:open' }),
      sessionIndex: [row(S), row('gone'), row('s2')]
    },
    steps: [apply(row('s2', { lastActivityAt: 40 }))]
  },
  {
    name: 'index_snapshot_cleans_handoff_titles',
    initial: { sessionIndex: [row('x')] },
    steps: [
      apply(
        row('h1', { title: '# Conversation handoff' }),
        row('h2', {
          title: '# Conversation handoff\n\nSummary\n\nUser: fix it\nAgent: done that'
        }),
        row('h3', { title: '# Conversation handoff\n\nSummary\n\n---\n\nShip the fix\nmore' }),
        row('h4', { title: '# Conversation handoff\n\nno turns here' }),
        row('h5', { title: 'About # Conversation handoff' })
      )
    ]
  },
  {
    name: 'index_snapshot_closes_tabs_of_chats_that_are_gone',
    context: { openChatTabs: [S, 'ghost', 's5'] },
    initial: seeded(idle),
    steps: [apply(row('s5'))]
  }
]
