// The ACP store's session discovery scenarios for `acp.ts`: `discoverSessions`,
// the agent's own `session/list` with its sign-in retry. Each restates a case
// of `discovery.test.ts`.
import type { Scenario } from '../record'
import { AGENT, ev } from './acp-fixtures'

const lists = { sessionCapabilities: { list: {} } }

/** `AGENT` connected, running `claude`, with `capabilities`. */
function agent(capabilities: unknown, status = 'connected', methods: unknown[] = []) {
  return {
    agents: { [AGENT]: { id: AGENT, capabilities, authMethods: methods } },
    agentStatus: { [AGENT]: status },
    configToLiveAgent: { 'claude\u0000/work': AGENT }
  }
}

const page = (ids: string[], nextCursor?: string | null) => ({
  sessions: ids.map((sessionId) => ({ sessionId, cwd: '/work', title: `t-${sessionId}` })),
  ...(nextCursor === undefined ? {} : { nextCursor })
})

const discover = (cwd = '/work/') => ev('discoverSessions', AGENT, cwd)
const KEY = `${AGENT}\u0000/work`

export const discoveryScenarios: Scenario[] = [
  {
    name: 'discover_sessions_signs_in_on_a_ws_auth_code',
    context: {
      agentReplies: [{ reject: 'nope', code: 'agent_auth_required' }, null, page(['a'])]
    },
    initial: agent(lists, 'connected', [{ id: 'oauth', name: 'OAuth' }]),
    steps: [discover()]
  },
  {
    name: 'discover_sessions_pages_until_the_cursor_ends_and_drops_repeats',
    context: { agentReplies: [page(['a', 'b'], ''), page(['b', 'c'], 'next'), page(['d'], null)] },
    initial: agent(lists),
    steps: [discover()]
  },
  {
    name: 'discover_sessions_needs_the_list_capability_and_a_live_agent',
    initial: agent(null),
    steps: [
      discover(),
      ev('_onAgentSpawned', { agentId: 'agent-2', capabilities: {} }),
      ev('discoverSessions', 'agent-2', '/work'),
      ev('_onAgentSpawned', { agentId: 'agent-3', capabilities: lists }),
      ev('discoverSessions', 'agent-3', '/work')
    ]
  },
  {
    name: 'discover_sessions_skips_an_agent_that_is_not_connected',
    initial: agent(lists, 'error'),
    steps: [discover()]
  },
  {
    name: 'discover_sessions_runs_once_per_agent_and_folder',
    context: { agentReplies: [{ hold: true, release: page(['a']) }, page(['z'])] },
    initial: agent(lists),
    steps: [discover(), discover('/work'), ev('discoverSessions', AGENT, ''), ev('releaseHeld')]
  },
  {
    name: 'discover_sessions_failure_clears_the_old_result',
    context: { agentReplies: [{ reject: 'boom' }] },
    initial: { ...agent(lists), discoveredSessions: { [KEY]: [{ sessionId: 'old' }] } },
    steps: [discover()]
  },
  {
    name: 'discover_sessions_drops_a_result_after_a_disconnect',
    context: { agentReplies: [{ hold: true, release: page(['a']) }] },
    initial: agent(lists),
    steps: [
      discover(),
      ev('_onAgentDisconnected', { agentId: AGENT, reason: 'crashed' }),
      ev('releaseHeld')
    ]
  },
  {
    name: 'discover_sessions_stops_after_ten_pages',
    context: { agentReplies: Array.from({ length: 11 }, (_, i) => page([`s${i}`], `c${i}`)) },
    initial: agent(lists),
    steps: [discover()]
  },
  {
    name: 'discover_sessions_signs_in_on_demand_and_lists_again',
    context: {
      agentReplies: [{ reject: 'ACP_AUTH_REQUIRED: sign in' }, null, page(['a'])]
    },
    initial: agent(lists, 'connected', [{ id: 'oauth', name: 'OAuth' }]),
    steps: [discover()]
  }
]
