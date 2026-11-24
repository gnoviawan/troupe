// The ACP store's agent roster and sign-in scenarios for `acp.ts`. Each
// restates a case of `events.test.ts`, `agent-roster-watch.test.ts`,
// `auth-recovery.test.ts` or `agent-browser-pane.test.ts`.
import type { Scenario } from '../record'
import { AGENT, ev, idle, message, S, seeded } from './acp-fixtures'

const spawned = (agentId: string, extra: Record<string, unknown> = {}) =>
  ev('_onAgentSpawned', { agentId, capabilities: { loadSession: true }, ...extra })

const browserOpen = (agentId: string, url: string) => ev('_onBrowserOpenRequest', { agentId, url })

const signIn = (requestId: string, url = 'https://login.example/x', agentId = AGENT) =>
  ev('_onAgentSignInRequest', { agentId, requestId, url, message: 'Sign in to continue' })

const agentError = (extra: Record<string, unknown> = {}) =>
  ev('_onAgentError', { agentId: AGENT, sessionId: S, message: 'boom', ...extra })

const crashed = (extra: Record<string, unknown> = {}) =>
  ev('_onAgentCrashed', { agentId: AGENT, sessionId: S, message: 'exit 1', ...extra })

const disconnected = (reason: string, agentId = AGENT) =>
  ev('_onAgentDisconnected', { agentId, reason })

/** Antigravity's plan-limit error data. */
const antigravityLimit = {
  agent: 'antigravity',
  errorKind: 'quota_exhausted',
  resetsAt: 1_700_000_123_456
}

const registered = { agents: { [AGENT]: { id: AGENT, capabilities: {}, authMethods: [] } } }

function running(id: string, seq: number): Record<string, unknown> {
  return {
    toolCallId: id,
    title: 'Task',
    status: 'pending',
    seq,
    timestamp: seq,
    subagent: {
      id: `sub-${id}`,
      rootSessionId: S,
      dispatchToolCallId: id,
      source: 'claude-meta',
      title: 'Explore',
      background: false,
      caps: { stop: 'session-cancel', transcript: 'live', liveProgress: true, usage: true },
      activity: 'running'
    }
  }
}

/** Agent 1 runs s1 (a reply streaming) and s2 (empty); s3 is closed; s9 is another agent's. */
function fleet(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  const base = seeded({ activeTurn: true, openTurnId: 'turn:t1', ...overrides })
  const one = (base.sessions as Record<string, Record<string, unknown>>)[S]
  return {
    ...base,
    ...registered,
    sessions: {
      [S]: one,
      s2: { ...one, id: 's2', activeTurn: false, openTurnId: null },
      s3: { ...one, id: 's3', status: 'closed', activeTurn: false, openTurnId: null },
      s9: { ...one, id: 's9', agentId: 'agent-2' }
    },
    messages: {
      [S]: [
        message('u1', 'user', 'go', { seq: 1 }),
        message('a1', 'agent', 'work', { seq: 2, streaming: true })
      ],
      s2: [],
      s9: [message('a9', 'agent', 'other', { seq: 3, streaming: true })]
    },
    toolCalls: { [S]: [running('d1', 3)], s2: [], s9: [running('d9', 4)] },
    sessionIndex: [{ id: S }, { id: 's2' }],
    agentStatus: { [AGENT]: 'connected', 'agent-2': 'connected' },
    configToLiveAgent: { 'cfg-1\u0000/work\u0000p1': AGENT, 'cfg-2\u0000/work\u0000p1': 'agent-2' },
    launchingSessionIds: { [S]: true, s9: true },
    preparedSessions: { 'prep-1': 's2', 'prep-9': 's9' },
    discoveredSessions: { [`${AGENT}\u0000/work`]: [], 'agent-2\u0000/work': [] },
    pendingPermissions: {
      p1: { requestId: 'p1', agentId: AGENT, sessionId: S, options: [], toolCall: null },
      p9: { requestId: 'p9', agentId: 'agent-2', sessionId: 's9', options: [], toolCall: null }
    },
    pendingQuestions: {
      q1: { questionId: 'q1', agentId: AGENT, sessionId: S, question: 'q', options: [] }
    },
    pendingElicitations: {
      e1: { requestId: 'e1', agentId: AGENT, sessionId: S, mode: 'form', message: 'm', fields: [] }
    },
    pendingSignIn: { [AGENT]: { kind: 'browser', url: 'https://login.example/a' } },
    autoOpenBrowserUrl: { [AGENT]: 'https://login.example/a' }
  }
}

export const agentScenarios: Scenario[] = [
  {
    name: 'agent_spawned_keeps_what_the_spawn_response_set',
    initial: {
      agents: {
        [AGENT]: { id: AGENT, capabilities: { image: true }, authMethods: [{ id: 'oauth' }] }
      }
    },
    steps: [
      spawned(AGENT, { authMethods: [{ id: 'other' }], hostAuthReady: true }),
      spawned('agent-2'),
      spawned('agent-3', { authMethods: [{ id: 'key' }], hostAuthReady: false })
    ]
  },
  {
    name: 'browser_open_before_the_spawn_is_promoted_by_it',
    steps: [
      browserOpen('agent-3', 'not a url'),
      browserOpen('agent-3', 'ftp://login.example'),
      browserOpen('', 'https://login.example'),
      browserOpen('agent-3', 'HTTPS://login.example/early'),
      spawned('agent-3'),
      spawned('agent-3')
    ]
  },
  {
    name: 'browser_open_for_a_known_agent_shows_the_sign_in',
    initial: {
      ...registered,
      autoOpenBrowserUrl: { [AGENT]: 'https://login.example/old', other: 'https://x.example' }
    },
    steps: [
      browserOpen(AGENT, 'https://login.example/new'),
      browserOpen(AGENT, 'http://login.example/plain')
    ]
  },
  {
    name: 'the_early_browser_open_buffer_forgets_the_oldest',
    steps: [
      ...Array.from({ length: 33 }, (_, i) => browserOpen(`early-${i}`, `https://l.example/${i}`)),
      spawned('early-0'),
      spawned('early-32')
    ]
  },
  {
    name: 'a_sign_in_prompt_waits_for_its_own_resolution',
    initial: registered,
    steps: [
      signIn('r1', 'http://login.example/plain'),
      signIn('r1', 'https://login.example/x', 'agent-unknown'),
      signIn('r1'),
      ev('_onAgentSignInResolved', { agentId: AGENT, requestId: 'r2' }),
      ev('_onAgentSignInResolved', { agentId: AGENT, requestId: 'r1' }),
      signIn('r3'),
      ev('clearPendingSignIn', AGENT)
    ]
  },
  {
    name: 'agent_error_ends_the_turn_of_its_session',
    initial: fleet(),
    steps: [agentError({ code: -32603, data: { detail: 'x' } }), agentError({ sessionId: 's3' })]
  },
  {
    name: 'agent_error_auth_required_marks_the_agent',
    initial: fleet(),
    steps: [agentError({ code: -32000, message: 'Authentication required' })]
  },
  {
    name: 'agent_error_at_the_provider_limit_rejects_the_rate_limit',
    initial: fleet({}),
    steps: [
      agentError({ providerCode: 'provider_quota_exceeded', message: 'You hit your 5-hour limit' }),
      agentError({ providerCode: 'provider_session_limit', message: 'provider_session_limit' }),
      agentError({ data: { errorKind: 'rate_limited' }, message: 'weekly cap', code: 1.5 }),
      agentError({ sessionId: 's9', data: { codexErrorInfo: 'usageLimitExceeded' } })
    ]
  },
  {
    name: 'agent_error_at_an_antigravity_limit_counts_only_for_an_antigravity_chat',
    initial: {
      ...fleet(),
      configToLiveAgent: {
        'acp-registry:antigravity-acp\u0000/work\u0000p1': AGENT,
        'cfg-2\u0000/work\u0000p1': 'agent-2'
      }
    },
    steps: [
      agentError({ data: { errorKind: 'rate_limited' }, message: 'weekly cap' }),
      agentError({ data: { ...antigravityLimit, resetsAt: -5 } }),
      agentError({ data: antigravityLimit, message: 'You hit your 5-hour limit' }),
      agentError({ sessionId: 's9', agentId: 'agent-2', data: antigravityLimit })
    ]
  },
  {
    name: 'agent_error_reads_an_antigravity_chat_from_the_index',
    initial: {
      ...fleet(),
      configToLiveAgent: {},
      sessionIndex: [{ id: S, agentConfigId: 'acp-registry:antigravity-acp' }]
    },
    steps: [agentError({ data: { ...antigravityLimit, resetsAt: 1_700_000_123_999.5 } })]
  },
  {
    name: 'agent_error_without_a_session_marks_every_live_one',
    initial: fleet(),
    steps: [agentError({ sessionId: null, code: 7, providerCode: 'provider_quota_exceeded' })]
  },
  {
    name: 'agent_crashed_on_a_session_marks_it_dead',
    initial: fleet(),
    steps: [crashed({ providerCode: 'provider_request_rejected', message: '  ' })]
  },
  {
    name: 'agent_crashed_without_a_session_marks_every_live_one',
    initial: fleet(),
    steps: [crashed({ sessionId: undefined, data: { signal: 9 } })]
  },
  {
    name: 'agent_crashed_on_a_closed_session_marks_the_others',
    initial: fleet(),
    steps: [crashed({ sessionId: 's3' })]
  },
  {
    name: 'agent_disconnected_on_purpose_drops_the_agent',
    initial: fleet(),
    steps: [disconnected('killed'), disconnected('killed')]
  },
  {
    name: 'agent_disconnected_after_a_crash_keeps_the_error',
    initial: fleet(),
    steps: [crashed(), disconnected('crashed')]
  },
  {
    name: 'agent_disconnected_keeps_an_idle_chat_with_content',
    initial: seeded(idle, {
      ...registered,
      agentStatus: { [AGENT]: 'connected' },
      messages: { [S]: [message('u1', 'user', 'hi', { seq: 1 })] }
    }),
    steps: [disconnected('reaped')]
  }
]
