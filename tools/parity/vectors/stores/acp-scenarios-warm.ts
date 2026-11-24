// The ACP store's warm-pool scenarios for `acp.ts`: `prepareChat`,
// `claimPreparedChat` and `cancelPreparedChat` (`slices/launch.ts`) with the
// orphan reap and the ephemeral sessions a close or disconnect drops. Each
// restates a case of `warm-pool.test.ts` or `spawn-prepare.test.ts`.
import type { Scenario } from '../record'
import { AGENT, ev } from './acp-fixtures'

const NUL = '\u0000'
const GEMINI = { id: 'gemini', name: 'Gemini', command: 'gemini-acp' }
const KEY = `gemini${NUL}/work${NUL}`
const WORK = { id: 'p1', name: 'Work', path: '/work' }
const models = { currentModelId: 'm1', availableModels: [{ modelId: 'm1', name: 'M1' }] }
const opened = (sessionId: string) => ({ sessionId, models })
const SPAWNED = { agentId: 'agent-9', capabilities: {}, authMethods: [] }

/** `AGENT` connected for the selected `gemini` config in Project `p1`. */
function live(extra: Record<string, unknown> = {}) {
  return {
    agentConfigs: [GEMINI],
    selectedAgentConfigId: 'gemini',
    agents: { [AGENT]: { id: AGENT, capabilities: {}, authMethods: [] } },
    agentStatus: { [AGENT]: 'connected' },
    configToLiveAgent: { [`gemini${NUL}/work${NUL}project${NUL}p1`]: AGENT },
    ...extra
  }
}

const warm = (sessionId: string, status = 'active') => ({
  [sessionId]: {
    id: sessionId,
    agentId: AGENT,
    cwd: '/work',
    projectId: 'p1',
    status,
    title: null,
    activeTurn: false,
    openTurnId: null,
    createdAt: 1
  }
})
const prepare = (...opts: unknown[]) =>
  ev('prepareChat', 'gemini', ' /work ', undefined, 'p1', ...opts)
const claim = (projectId = 'p1') => ev('claimPreparedChat', KEY, projectId)
const DROID = { id: 'factory-droid', name: 'Droid', command: 'droid' }
const DOCS = { name: 'docs', command: 'docs-mcp', args: [], env: [] }
const DOCS_KEY = `gemini${NUL}/work${NUL}${JSON.stringify(DOCS)}`

export const warmScenarios: Scenario[] = [
  {
    name: 'prepare_chat_fills_a_warm_slot',
    context: { projects: [WORK], agentReplies: [opened('w1')] },
    initial: live(),
    steps: [prepare()]
  },
  {
    name: 'prepare_chat_records_a_spawn_failure_and_toasts',
    context: { projects: [WORK], agentReplies: [{ reject: 'spawn gemini-acp ENOENT' }] },
    initial: live({ configToLiveAgent: {} }),
    steps: [prepare()]
  },
  {
    name: 'prepare_chat_in_the_background_does_not_toast',
    context: { projects: [WORK], agentReplies: [{ reject: 'spawn gemini-acp ENOENT' }] },
    initial: live({ configToLiveAgent: {} }),
    steps: [prepare({ silent: true })]
  },
  {
    name: 'prepare_chat_in_the_background_skips_a_definite_rejection',
    context: { projects: [WORK], agentReplies: [opened('w1')] },
    initial: live({
      prepareChatErrors: {
        [KEY]: { category: 'spawn', label: 'Agent unavailable', detail: 'gone', definite: true }
      }
    }),
    steps: [prepare({ silent: true }), prepare()]
  },
  {
    name: 'prepare_chat_classifies_a_session_failure',
    context: { projects: [WORK], agentReplies: [{ reject: 'session/new timed out' }] },
    initial: live(),
    steps: [prepare()]
  },
  {
    name: 'prepare_chat_keeps_a_live_slot',
    context: { projects: [WORK] },
    initial: live({
      sessions: warm('w1'),
      messages: { w1: [] },
      preparedSessions: { [KEY]: 'w1' }
    }),
    steps: [prepare()]
  },
  {
    name: 'claim_prepared_chat_reaps_a_stale_slot',
    context: { projects: [WORK], agentReplies: [null] },
    initial: live({
      sessions: warm('w1', 'closed'),
      messages: { w1: [] },
      preparedSessions: { [KEY]: 'w1' }
    }),
    steps: [claim()]
  },
  {
    name: 'claim_prepared_chat_promotes_and_seeds_the_next',
    context: { projects: [WORK], agentReplies: [opened('w1'), null, SPAWNED, opened('w2')] },
    initial: live(),
    steps: [prepare(), claim()]
  },
  {
    name: 'claim_prepared_chat_refuses_another_project',
    context: { projects: [WORK], agentReplies: [opened('w1')] },
    initial: live(),
    steps: [prepare(), claim('p2'), ev('claimPreparedChat', 'missing', 'p1')]
  },
  {
    name: 'claim_prepared_chat_warns_when_the_promotion_fails',
    context: {
      projects: [WORK],
      agentReplies: [opened('w1'), { reject: 'promote refused' }, SPAWNED, opened('w2')]
    },
    initial: live(),
    steps: [prepare(), claim()]
  },
  {
    name: 'cancel_prepared_chat_reaps_the_warm_session',
    context: { projects: [WORK], agentReplies: [opened('w1'), null] },
    initial: live(),
    steps: [prepare(), ev('cancelPreparedChat', KEY)]
  },
  {
    name: 'cancel_prepared_chat_logs_a_failed_dispose',
    context: { projects: [WORK], agentReplies: [opened('w1'), { reject: 'dispose refused' }] },
    initial: live(),
    steps: [prepare(), ev('cancelPreparedChat', KEY)]
  },
  {
    name: 'cancel_prepared_chat_during_the_prepare_reaps_its_session',
    context: { projects: [WORK], agentReplies: [{ hold: true, release: opened('w1') }, null] },
    initial: live(),
    steps: [prepare(), ev('cancelPreparedChat', KEY), ev('releaseHeld')]
  },
  {
    name: 'an_empty_warm_session_closed_by_the_host_is_dropped',
    context: { projects: [WORK], agentReplies: [opened('w1')] },
    initial: live(),
    steps: [prepare(), ev('_onSessionClosed', { agentId: AGENT, sessionId: 'w1' })]
  },
  {
    name: 'an_empty_warm_session_goes_with_its_agent',
    context: { projects: [WORK], agentReplies: [opened('w1')] },
    initial: live(),
    steps: [prepare(), ev('_onAgentDisconnected', { agentId: AGENT, reason: 'killed' })]
  },
  {
    name: 'a_warm_session_outdated_during_the_prepare_is_prepared_again',
    context: {
      projects: [WORK],
      agentReplies: [
        { hold: true, release: opened('w1') },
        null,
        { agentId: 'agent-9', capabilities: {}, authMethods: [] },
        opened('w2')
      ]
    },
    initial: live(),
    steps: [prepare(), ev('renewAgentCredentials', 'gemini', 'p1'), ev('releaseHeld')]
  },
  {
    name: 'prepare_chat_joins_the_prepare_in_flight',
    context: { projects: [WORK], agentReplies: [{ hold: true }] },
    initial: live(),
    steps: [prepare(), prepare()]
  },
  {
    name: 'prepare_chat_cancelled_while_its_agent_spawns_opens_nothing',
    context: { projects: [WORK], agentReplies: [{ hold: true, release: SPAWNED }] },
    initial: live({ configToLiveAgent: {} }),
    steps: [prepare(), ev('cancelPreparedChat', KEY), ev('releaseHeld')]
  },
  {
    name: 'prepare_chat_drops_a_session_whose_agent_died_meanwhile',
    context: { projects: [WORK], agentReplies: [{ hold: true, release: opened('w1') }] },
    initial: live(),
    steps: [
      prepare(),
      ev('_onAgentDisconnected', { agentId: AGENT, reason: 'killed' }),
      ev('releaseHeld')
    ]
  },
  {
    name: 'a_reaped_warm_session_frees_its_agent_for_a_chat',
    context: {
      projects: [WORK],
      agentReplies: [opened('w1'), null, {}],
      payloads: {
        s1: {
          metadata: {
            id: 's1',
            agentId: AGENT,
            agentConfigId: 'gemini',
            cwd: '/work',
            projectId: 'p1',
            title: 'Old chat',
            status: 'closed',
            createdAt: 5,
            lastSeq: 2
          },
          messages: [],
          toolCalls: []
        }
      }
    },
    initial: live({
      agents: { [AGENT]: { id: AGENT, capabilities: { loadSession: true }, authMethods: [] } },
      sessionIndex: [{ id: 's1' }]
    }),
    steps: [prepare(), ev('cancelPreparedChat', KEY), ev('openHistorySession', 's1')]
  },
  {
    name: 'cancel_prepared_chat_keeps_the_active_chat',
    context: { projects: [WORK] },
    initial: live({
      sessions: warm('w1'),
      messages: { w1: [] },
      preparedSessions: { [KEY]: 'w1' },
      activeSessionId: 'w1'
    }),
    steps: [ev('cancelPreparedChat', KEY)]
  },
  {
    name: 'cancel_prepared_chat_clears_the_slot_error',
    context: { projects: [WORK] },
    initial: live({
      prepareChatErrors: {
        [KEY]: { category: 'spawn', label: 'Agent unavailable', detail: 'gone', definite: true }
      }
    }),
    steps: [ev('cancelPreparedChat', KEY)]
  },
  {
    name: 'claim_prepared_chat_does_not_seed_an_isolating_agent',
    context: { projects: [WORK], agentReplies: [opened('w1'), null] },
    initial: live({
      agentConfigs: [DROID],
      selectedAgentConfigId: 'factory-droid',
      configToLiveAgent: { [`factory-droid${NUL}/work${NUL}project${NUL}p1`]: AGENT }
    }),
    steps: [
      ev('prepareChat', 'factory-droid', '/work', undefined, 'p1'),
      ev('claimPreparedChat', `factory-droid${NUL}/work${NUL}`, 'p1')
    ]
  },
  {
    name: 'claim_prepared_chat_with_its_own_mcp_servers_does_not_seed',
    context: { projects: [WORK], agentReplies: [opened('w1'), null] },
    initial: live(),
    steps: [
      ev('prepareChat', 'gemini', '/work', [DOCS], 'p1'),
      ev('claimPreparedChat', DOCS_KEY, 'p1')
    ]
  },
  {
    name: 'a_chat_whose_promotion_failed_goes_with_its_agent',
    context: {
      projects: [WORK],
      agentReplies: [opened('w1'), { reject: 'promote refused' }, SPAWNED, opened('w2')]
    },
    initial: live(),
    steps: [prepare(), claim(), ev('_onAgentDisconnected', { agentId: AGENT, reason: 'killed' })]
  },
  {
    name: 'claim_prepared_chat_drops_a_slot_whose_mcp_list_changed',
    context: { projects: [WORK], agentReplies: [opened('w1'), null], mcpReplies: [null] },
    initial: live({
      mcpServers: [
        { id: 'a', type: 'stdio', name: 'cmd-a', command: 'npx', args: [], env: [], enabled: true }
      ],
      mcpServersLoaded: true
    }),
    steps: [prepare(), ev('setMcpServerEnabled', 'a', false), claim()]
  },
  {
    name: 'claim_prepared_chat_never_reaps_an_indexed_chat',
    context: { projects: [WORK] },
    initial: live({
      sessions: warm('w1', 'closed'),
      messages: { w1: [] },
      preparedSessions: { [KEY]: 'w1' },
      sessionIndex: [{ id: 'w1' }]
    }),
    steps: [claim()]
  },
  {
    name: 'a_second_warm_key_shares_an_agent_that_only_holds_a_warm_session',
    context: { projects: [WORK], agentReplies: [opened('w1'), opened('w2')] },
    initial: live(),
    steps: [prepare(), ev('prepareChat', 'gemini', '/work', [DOCS], 'p1')]
  }
]
