// The ACP store's `createSession` scenarios for `acp.ts` (`slices/session.ts`):
// `session/new` through the sign-in retry, the MCP selection, the extra
// workspace roots and the chat it adds. Each restates a case of
// `lifecycle.test.ts`, `auth-recovery.test.ts` or `spawn-prepare.test.ts`.
import type { Scenario } from '../record'
import { AGENT, ev } from './acp-fixtures'

const NUL = '\u0000'
const modes = { currentModeId: 'ask', availableModes: [{ id: 'ask' }, { id: 'code' }] }
const models = { currentModelId: 'm1', availableModels: [{ modelId: 'm1', name: 'M1' }] }
const effort = {
  id: 'effort',
  name: 'Effort',
  category: 'thought_level',
  type: 'select',
  currentValue: 'low',
  options: [{ value: 'low', name: 'Low' }]
}
const method = (id: string, extra: Record<string, unknown> = {}) => ({ id, name: id, ...extra })

/** `AGENT` connected for `claude` in `/work`. */
function live(agent: Record<string, unknown> = {}, extra: Record<string, unknown> = {}) {
  return {
    agents: { [AGENT]: { id: AGENT, authMethods: [], ...agent } },
    agentStatus: { [AGENT]: 'connected' },
    configToLiveAgent: { [`claude${NUL}/work`]: AGENT },
    ...extra
  }
}

const created = (sessionId = 's5', extra: Record<string, unknown> = {}) => ({
  sessionId,
  modes,
  models,
  configOptions: [effort],
  ...extra
})
const create = (...args: unknown[]) => ev('createSession', AGENT, '/work', ...args)
const REQUIRED = { reject: 'ACP_AUTH_REQUIRED: sign in first' }

export const createScenarios: Scenario[] = [
  {
    name: 'create_session_adds_the_chat_and_persists_it',
    context: { agentReplies: [created()] },
    initial: live(),
    steps: [create([], 'p1')]
  },
  {
    name: 'create_session_keeps_the_active_chat',
    context: { agentReplies: [created('s6', { cwd: '/work/sub' })] },
    initial: live({}, { activeSessionId: 's1' }),
    steps: [create([], undefined, { kind: 'chat' })]
  },
  {
    name: 'create_session_starts_the_registry_servers_the_agent_can_run',
    context: { agentReplies: [created()] },
    initial: live(
      { capabilities: { mcpCapabilities: { http: true } } },
      {
        mcpServers: [
          { id: 'a', name: 'Files', command: 'files-mcp' },
          { id: 'b', name: 'Web', type: 'http', url: 'https://mcp.example/web' },
          { id: 'c', name: 'Events', type: 'sse', url: 'https://mcp.example/sse' },
          { id: 'd', name: 'Off', command: 'off', enabled: false }
        ]
      }
    ),
    steps: [create()]
  },
  {
    name: 'create_session_starts_every_server_before_the_agent_reports_capabilities',
    context: { agentReplies: [created()] },
    initial: live(
      {},
      {
        mcpServers: [
          { id: 'b', name: 'Web', type: 'http', url: 'https://mcp.example/web', headers: [] }
        ]
      }
    ),
    steps: [create()]
  },
  {
    name: 'create_session_sends_the_project_roots_worktree_and_quick_kind',
    context: {
      projects: [
        {
          id: 'p1',
          name: 'Work',
          path: '/work',
          worktrees: [
            { path: '/wt/a' },
            { path: 'relative' },
            { path: '/wt/a' },
            { path: 'C:\\wt' }
          ]
        }
      ],
      agentReplies: [created()]
    },
    initial: live({ capabilities: { sessionCapabilities: { additionalDirectories: {} } } }),
    steps: [
      create([], 'p1', {
        worktreePath: '/wt/a',
        worktreeBranch: 'feat',
        kind: 'quick',
        promotable: true
      })
    ]
  },
  {
    name: 'create_session_merges_what_events_set_during_the_call',
    context: { agentReplies: [{ hold: true, release: created() }] },
    initial: live(),
    steps: [
      create([], 'p1'),
      ev('_onSessionCreated', {
        agentId: AGENT,
        sessionId: 's5',
        modes: { currentModeId: 'code', availableModes: [{ id: 'code' }] }
      }),
      ev('_onAgentWorking', { agentId: AGENT, sessionId: 's5', working: true }),
      ev('releaseHeld')
    ]
  },
  {
    name: 'create_session_keeps_a_title_and_close_set_during_the_call',
    context: { agentReplies: [{ hold: true, release: created('s5', { cwd: '' }) }] },
    initial: live(),
    steps: [
      create([], 'p1'),
      ev('_onSessionCreated', { agentId: AGENT, sessionId: 's5' }),
      ev('_onSessionInfoUpdate', { agentId: AGENT, sessionId: 's5', title: 'Named' }),
      ev('_onSessionClosed', { agentId: AGENT, sessionId: 's5' }),
      ev('releaseHeld')
    ]
  },
  {
    name: 'create_session_keeps_a_turn_opened_during_the_call',
    context: { agentReplies: [{ hold: true, release: created() }] },
    initial: live(),
    steps: [
      create([], 'p1'),
      ev('_onSessionCreated', { agentId: AGENT, sessionId: 's5' }),
      ev('_onUserPrompt', {
        agentId: AGENT,
        sessionId: 's5',
        content: [{ type: 'text', text: 'hi' }]
      }),
      ev('releaseHeld')
    ]
  },
  {
    name: 'create_session_ephemeral_is_not_persisted_or_activated',
    context: { agentReplies: [created()] },
    initial: live(),
    steps: [create([], 'p1', { ephemeral: true, backendEphemeral: true })]
  },
  {
    name: 'create_session_lookup_of_an_ephemeral_session_fails',
    context: { agentReplies: [{ sessionId: 's5', lookup: true }] },
    initial: live(),
    steps: [create([], 'p1', { ephemeral: true })]
  },
  {
    name: 'create_session_lookup_of_a_host_ephemeral_session_fails',
    context: { agentReplies: [{ sessionId: 's5', lookup: true }] },
    initial: live(),
    steps: [create([], 'p1', { backendEphemeral: true })]
  },
  {
    name: 'create_session_signs_in_on_demand_and_retries',
    context: { agentReplies: [REQUIRED, null, created()] },
    initial: live({ authMethods: [method('oauth', { type: 'agent' })] }),
    steps: [create([])]
  },
  {
    name: 'create_session_shows_the_picker_for_several_methods',
    context: { agentReplies: [REQUIRED] },
    initial: live({ authMethods: [method('oauth'), method('key')] }),
    steps: [create([])]
  },
  {
    name: 'create_session_forgets_the_sign_in_when_the_retry_fails',
    context: { agentReplies: [REQUIRED, null, { reject: 'HTTP 401 unauthorized' }] },
    initial: live({ authMethods: [method('oauth')] }),
    steps: [create([])]
  },
  {
    name: 'create_session_evicts_the_agent_on_a_broken_connection',
    context: { agentReplies: [{ reject: 'stream closed by peer' }, null] },
    initial: live(),
    steps: [create([])]
  },
  {
    name: 'create_session_rejects_after_a_failed_eviction_kill',
    context: {
      agentReplies: [null, { reject: 'connection reset' }, { reject: 'no such agent' }]
    },
    initial: live({ authMethods: [method('oauth')] }),
    steps: [ev('authenticateAgent', AGENT, 'oauth'), create([])]
  },
  {
    name: 'create_session_rejects_a_definite_refusal_as_is',
    context: {
      agentReplies: [{ reject: 'the agent binary is missing', code: 'agent_unavailable' }]
    },
    initial: live(),
    steps: [create([])]
  }
]
