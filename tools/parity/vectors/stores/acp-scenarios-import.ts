// The ACP store's import scenarios for `acp.ts`: `listImportableSessions` and
// `importAgentSessions` (`slices/import-sessions.ts`), with the sign-in retry
// around `session/list` and `session/load`. Each restates a case of
// `import-sessions.test.ts`.
import type { Scenario } from '../record'
import { AGENT, ev, idle, seeded } from './acp-fixtures'

const CLAUDE = { id: 'claude', name: 'Claude', command: 'claude-acp' }
const WORK = { id: 'p1', name: 'Work', path: '/work' }
const NUL = '\u0000'
const caps = { loadSession: true, sessionCapabilities: { list: {} } }

/** `AGENT` connected for `claude` in `/work`, with `capabilities`. */
function live(capabilities: unknown = caps, extra: Record<string, unknown> = {}) {
  return {
    agentConfigs: [CLAUDE],
    agents: { [AGENT]: { id: AGENT, capabilities, authMethods: [] } },
    agentStatus: { [AGENT]: 'connected' },
    configToLiveAgent: { [`claude${NUL}/work`]: AGENT },
    ...extra
  }
}

const page = (ids: string[], nextCursor?: string | null) => ({
  sessions: ids.map((sessionId) => ({ sessionId, cwd: '/work', title: `t-${sessionId}` })),
  ...(nextCursor === undefined ? {} : { nextCursor })
})
const spawned = (agentId: string, capabilities: unknown = caps) => ({
  agentId,
  capabilities,
  authMethods: []
})

const list = (cwd = ' /work ', ...project: string[]) =>
  ev('listImportableSessions', 'claude', cwd, ...project)
const importing = (sessions: unknown[], projectId?: string) =>
  ev('importAgentSessions', {
    agentId: AGENT,
    cwd: '/work',
    sessions,
    ...(projectId ? { projectId } : {})
  })

export const importScenarios: Scenario[] = [
  {
    name: 'list_importable_sessions_reuses_the_live_agent_and_skips_known_chats',
    context: { agentReplies: [page(['a', 'b'], 'c1'), page(['b', 'c'])] },
    initial: live(caps, { sessionIndex: [{ id: 'a' }] }),
    steps: [list()]
  },
  {
    name: 'list_importable_sessions_spawns_an_agent_for_the_project',
    context: { projects: [WORK], agentReplies: [spawned('agent-9'), page(['a'], null)] },
    initial: { agentConfigs: [CLAUDE] },
    steps: [list('/work', 'p1')]
  },
  {
    name: 'list_importable_sessions_replaces_an_agent_on_an_old_launch',
    context: {
      agentReplies: [spawned('agent-8'), page([]), spawned('agent-9'), page(['a'])]
    },
    initial: { agentConfigs: [CLAUDE] },
    steps: [list(), ev('renewAgentCredentials', 'claude', ''), list()]
  },
  {
    name: 'list_importable_sessions_needs_load_and_list',
    initial: live({ sessionCapabilities: { list: {} } }),
    steps: [
      list(),
      ev('_onAgentSpawned', { agentId: AGENT, capabilities: { loadSession: true } }),
      list()
    ]
  },
  {
    name: 'list_importable_sessions_needs_session_list',
    initial: live({ loadSession: true }),
    steps: [list()]
  },
  {
    name: 'list_importable_sessions_lists_through_an_agent_a_chat_holds',
    context: { agentReplies: [page(['a'])] },
    initial: seeded(idle, live()),
    steps: [list()]
  },
  {
    name: 'list_importable_sessions_spawns_past_a_disconnected_agent',
    context: { agentReplies: [spawned('agent-9'), page([])] },
    initial: live(caps, { agentStatus: { [AGENT]: 'error' } }),
    steps: [list()]
  },
  {
    name: 'list_importable_sessions_without_an_agent_is_unavailable',
    context: { agentReplies: [{ reject: 'boom' }] },
    initial: { agentConfigs: [CLAUDE] },
    steps: [ev('listImportableSessions', 'missing', '/work'), list('   '), list()]
  },
  {
    name: 'list_importable_sessions_stops_on_a_repeated_cursor',
    context: { agentReplies: [page(['a'], 'x'), page(['b'], 'y'), page(['c'], 'x')] },
    initial: live(),
    steps: [list()]
  },
  {
    name: 'list_importable_sessions_stops_at_the_page_cap',
    context: {
      agentReplies: Array.from({ length: 201 }, (_, i) => ({ sessions: [], nextCursor: `c${i}` }))
    },
    initial: live(),
    steps: [list()]
  },
  {
    name: 'list_importable_sessions_drops_a_listing_whose_agent_disconnected',
    context: { agentReplies: [{ hold: true, release: page(['a']) }] },
    initial: live(),
    steps: [
      list(),
      ev('_onAgentDisconnected', { agentId: AGENT, reason: 'crashed' }),
      ev('releaseHeld')
    ]
  },
  {
    name: 'list_importable_sessions_failure_rejects',
    context: { agentReplies: [{ reject: 'list broke' }] },
    initial: live(),
    steps: [list()]
  },
  {
    name: 'list_importable_sessions_signs_in_on_demand',
    context: { agentReplies: [{ reject: 'ACP_AUTH_REQUIRED: sign in' }, null, page(['a'])] },
    initial: live(caps, {
      agents: {
        [AGENT]: { id: AGENT, capabilities: caps, authMethods: [{ id: 'oauth', name: 'OAuth' }] }
      }
    }),
    steps: [list()]
  },
  {
    name: 'import_agent_sessions_reports_each_outcome_then_reloads_the_index',
    context: {
      agentReplies: [null, { reject: 'already there', code: 'duplicate' }, { reject: 'boom' }, null]
    },
    initial: live(),
    steps: [
      importing(
        [
          { sessionId: 'a', cwd: '/x', title: 'A', updatedAt: '2026-01-02T03:04:05.678Z' },
          { sessionId: 'b', cwd: '', title: 'B', updatedAt: '2026-01-02T03:04:05+02:00' },
          { sessionId: 'c', updatedAt: 'yesterday' },
          { sessionId: 'd', updatedAt: '2026-01-02' }
        ],
        'p1'
      )
    ]
  },
  {
    name: 'import_agent_sessions_of_nothing_still_reloads_the_index',
    initial: live(),
    steps: [importing([])]
  },
  {
    name: 'import_agent_sessions_stops_before_the_next_session',
    context: { agentReplies: [{ hold: true, release: null }] },
    initial: live(),
    steps: [
      importing([{ sessionId: 'a' }, { sessionId: 'b' }, { sessionId: 'c' }]),
      ev('stopImport'),
      ev('releaseHeld')
    ]
  },
  {
    name: 'import_agent_sessions_without_a_sign_in_keeps_the_first_error',
    context: { agentReplies: [{ reject: 'ACP_AUTH_REQUIRED: taken', code: 'duplicate' }] },
    initial: live(),
    steps: [importing([{ sessionId: 'a' }])]
  },
  {
    name: 'import_agent_sessions_signs_in_on_demand',
    context: {
      agentReplies: [{ reject: 'ACP_AUTH_REQUIRED: sign in' }, null, null, { reject: '401' }]
    },
    initial: live(caps, {
      agents: {
        [AGENT]: { id: AGENT, capabilities: caps, authMethods: [{ id: 'oauth', name: 'OAuth' }] }
      }
    }),
    steps: [importing([{ sessionId: 'a' }, { sessionId: 'b' }])]
  }
]
