// The ACP store's switched-chat reopen scenarios for `acp.ts`:
// `redirectSwitchedReopen` and `respliceLiveSwitchTarget` (`slices/session.ts`)
// inside `openHistorySession`. Each restates a case of `agent-switch.test.ts`
// or `history-session.test.ts`. A step that ends a reopen is followed by a
// `tick` step: the replay window closes on a timer the step does not wait for.
import type { Scenario } from '../record'
import { AGENT, ev, message } from './acp-fixtures'

const NUL = '\u0000'
const GEMINI = { id: 'gemini', name: 'Gemini', command: 'gemini-acp' }
const CLAUDE = { id: 'claude', name: 'Claude', command: 'claude-acp' }
const tick = ev('clearPendingSignIn', 'none')
const open = (id = 's1') => ev('openHistorySession', id)
const LOADS = { loadSession: true }
const PROJECTS = [{ id: 'p1', name: 'Work', path: '/work' }]

const user = (id: string, text: string, seq: number) => message(id, 'user', text, { seq })
const agent = (id: string, text: string, seq: number) => message(id, 'agent', text, { seq })

/** A switch marker at `seq` from `from` to `to`, continuing in `next`. */
function marker(next: string, seq: number, from = 'gemini', to = 'claude') {
  return {
    id: `switch:seq-${seq}`,
    seq,
    fromConfigId: from,
    toConfigId: to,
    newSessionId: next,
    summaryText: 'Handoff',
    timestamp: 50 + seq
  }
}

/** The durable payload of `id`: one turn, a tool card, `switches`. */
function payload(
  id: string,
  config: string,
  switches: unknown[] = [],
  meta: Record<string, unknown> = {}
): Record<string, unknown> {
  return {
    metadata: {
      id,
      agentId: AGENT,
      agentConfigId: config,
      cwd: '/work',
      projectId: 'p1',
      title: `Chat ${id}`,
      status: 'closed',
      createdAt: 5,
      lastSeq: 4,
      lastActivityAt: 77,
      ...meta
    },
    messages: [user(`turn:${id}-1`, `ask ${id}`, 1), agent(`${id}-m2`, `answer ${id}`, 2)],
    toolCalls: [{ toolCallId: `${id}-tc`, title: 'Read', status: 'completed', seq: 3 }],
    switches
  }
}

/** `AGENT` connected for both configs in `/work`. */
function live(extra: Record<string, unknown> = {}) {
  return {
    agentConfigs: [GEMINI, CLAUDE],
    agents: { [AGENT]: { id: AGENT, capabilities: LOADS, authMethods: [] } },
    agentStatus: { [AGENT]: 'connected' },
    configToLiveAgent: {
      [`gemini${NUL}/work${NUL}project${NUL}p1`]: AGENT,
      [`claude${NUL}/work${NUL}project${NUL}p1`]: AGENT
    },
    sessionIndex: [
      { id: 's1', title: 'Chat s1', agentConfigId: 'gemini', projectId: 'p1' },
      { id: 's2', title: 'Chat s2', agentConfigId: 'claude', projectId: 'p1', createdAt: 60 },
      { id: 's3', title: 'Chat s3', agentConfigId: 'gemini', projectId: 'p1', createdAt: 70 }
    ],
    ...extra
  }
}

const S1 = payload('s1', 'gemini', [marker('s2', 4)])

/** `s1` to `s<n>`, each switched to the next, the configs alternating. */
function chain(n: number): Record<string, unknown> {
  const config = (k: number) => (k % 2 === 1 ? 'gemini' : 'claude')
  const out: Record<string, unknown> = {}
  for (let k = 1; k <= n; k++) {
    const next = k < n ? [marker(`s${k + 1}`, 4, config(k), config(k + 1))] : []
    out[`s${k}`] = payload(`s${k}`, config(k), next)
  }
  return out
}

const chainIndex = (n: number) =>
  Array.from({ length: n }, (_, i) => ({
    id: `s${i + 1}`,
    title: `Chat s${i + 1}`,
    agentConfigId: i % 2 === 0 ? 'gemini' : 'claude',
    projectId: 'p1',
    createdAt: 60 + i
  }))

export const reopenSwitchScenarios: Scenario[] = [
  {
    name: 'reopen_switch_redirects_to_its_continuation',
    context: {
      payloads: { s1: S1, s2: payload('s2', 'claude') },
      agentReplies: [{}],
      projects: PROJECTS,
      openChatTabs: ['s1']
    },
    initial: live(),
    steps: [open(), tick]
  },
  {
    name: 'reopen_switch_redirect_walks_every_hop_to_the_last_session',
    context: {
      payloads: {
        s1: S1,
        s2: payload('s2', 'claude', [marker('s3', 4, 'claude', 'gemini')]),
        s3: payload('s3', 'gemini')
      },
      agentReplies: [{}],
      projects: PROJECTS,
      openChatTabs: ['s1']
    },
    initial: live(),
    steps: [open(), tick]
  },
  {
    name: 'reopen_switch_redirect_walks_three_hops',
    context: {
      payloads: {
        s1: S1,
        s2: payload('s2', 'claude', [marker('s3', 4, 'claude', 'gemini')]),
        s3: payload('s3', 'gemini', [marker('s4', 4)]),
        s4: payload('s4', 'claude')
      },
      agentReplies: [{}],
      projects: PROJECTS,
      openChatTabs: ['s1']
    },
    initial: live(),
    steps: [open(), tick]
  },
  {
    name: 'reopen_switch_redirect_reads_the_host_copy_first',
    context: {
      payloads: { s1: S1, s2: payload('s2', 'claude'), s3: payload('s3', 'gemini') },
      hostPayloads: {
        s2: payload('s2', 'claude', [marker('s3', 4, 'claude', 'gemini')], { lastSeq: 9 }),
        s3: payload('s3', 'gemini')
      },
      agentReplies: [{}],
      projects: PROJECTS
    },
    initial: live(),
    steps: [open(), tick]
  },
  {
    name: 'reopen_switch_redirect_uses_the_cache_when_the_host_reread_fails',
    context: {
      payloads: { s1: S1, s2: payload('s2', 'claude') },
      hostPayloads: { s2: { reject: 'host unreachable' } },
      agentReplies: [{}],
      projects: PROJECTS
    },
    initial: live(),
    steps: [open(), tick]
  },
  {
    name: 'reopen_switch_redirect_falls_back_when_the_continuation_has_no_history',
    context: {
      payloads: { s1: S1 },
      agentReplies: [{}],
      projects: PROJECTS,
      openChatTabs: ['s1']
    },
    initial: live(),
    steps: [open(), tick]
  },
  {
    name: 'reopen_switch_redirect_falls_back_when_the_continuation_fails_to_load',
    context: {
      payloads: { s1: S1, s2: payload('s2', 'claude') },
      agentReplies: [{ reject: 'load blew up' }, {}],
      projects: PROJECTS,
      openChatTabs: ['s1']
    },
    initial: live(),
    steps: [open(), tick]
  },
  {
    name: 'reopen_switch_redirect_moves_the_queued_prompts',
    // The continuation still runs a turn, so the moved prompt waits in its
    // queue: a flushed prompt's receipt would race the action's answer.
    context: {
      payloads: {
        s1: S1,
        s2: payload('s2', 'claude', [], { status: 'active', turnActive: true, lastSeq: 6 })
      },
      agentReplies: [{}],
      projects: PROJECTS,
      openChatTabs: ['s1']
    },
    initial: live({
      promptQueues: {
        s1: [{ id: 'q1', text: 'later', blocks: [{ type: 'text', text: 'later' }] }]
      }
    }),
    steps: [open(), tick]
  },
  {
    name: 'reopen_switch_redirect_to_a_live_continuation_replaces_its_live_splice',
    context: {
      payloads: { s1: S1, s2: payload('s2', 'claude') },
      projects: PROJECTS,
      openChatTabs: ['s1']
    },
    initial: live({
      sessions: {
        s2: {
          id: 's2',
          agentId: AGENT,
          status: 'active',
          title: 'Chat s2',
          activeTurn: false,
          openTurnId: null,
          cwd: '/work',
          createdAt: 60
        }
      },
      messages: {
        s2: [
          message('switch-splice:s1:live-u1', 'user', 'ask s1', { seq: -3 }),
          message('s2-live', 'agent', 'answer s2', { seq: 1 })
        ]
      },
      toolCalls: { s2: [] },
      agentSwitches: {
        s2: [{ ...marker('s2', -1), id: 'switch-splice:s1:switch:fabricated:1' }]
      }
    }),
    steps: [open(), tick]
  },
  {
    name: 'reopen_switch_ignores_a_marker_without_a_new_session',
    context: {
      payloads: { s1: payload('s1', 'gemini', [{ ...marker('', 4) }]) },
      agentReplies: [{}],
      projects: PROJECTS
    },
    initial: live(),
    steps: [open(), tick]
  },
  {
    name: 'reopen_switch_ignores_a_marker_without_a_target_config',
    context: {
      payloads: { s1: payload('s1', 'gemini', [{ ...marker('s2', 4), toConfigId: '' }]) },
      agentReplies: [{}],
      projects: PROJECTS
    },
    initial: live(),
    steps: [open(), tick]
  },
  {
    name: 'reopen_switch_redirect_stops_at_a_marker_cycle',
    context: {
      payloads: {
        s1: S1,
        s2: payload('s2', 'claude', [marker('s1', 4, 'claude', 'gemini')])
      },
      agentReplies: [{}],
      projects: PROJECTS,
      openChatTabs: ['s1']
    },
    initial: live(),
    steps: [open(), tick]
  },
  {
    name: 'reopen_switch_redirect_walks_at_most_eight_hops',
    context: {
      payloads: chain(11),
      agentReplies: [{}, {}, {}],
      projects: PROJECTS,
      openChatTabs: ['s1']
    },
    initial: live({ sessionIndex: chainIndex(11) }),
    steps: [open(), tick, tick]
  },
  {
    name: 'reopen_switch_redirect_stops_at_a_cycle_past_the_chat',
    context: {
      payloads: {
        s1: S1,
        s2: payload('s2', 'claude', [marker('s3', 4, 'claude', 'gemini')]),
        s3: payload('s3', 'gemini', [marker('s2', 4, 'gemini', 'claude')])
      },
      agentReplies: [{}, {}, {}],
      projects: PROJECTS,
      openChatTabs: ['s1']
    },
    initial: live(),
    steps: [open(), tick, tick]
  },
  {
    name: 'reopen_switch_ignores_a_marker_naming_the_chat_itself',
    context: {
      payloads: { s1: payload('s1', 'gemini', [marker('s1', 4)]) },
      agentReplies: [{}],
      projects: PROJECTS
    },
    initial: live(),
    steps: [open(), tick]
  },
  {
    name: 'reopen_switch_redirect_stops_at_a_hop_naming_itself',
    context: {
      payloads: { s1: S1, s2: payload('s2', 'claude', [marker('s2', 4, 'claude', 'gemini')]) },
      agentReplies: [{}],
      projects: PROJECTS,
      openChatTabs: ['s1']
    },
    initial: live(),
    steps: [open(), tick]
  },
  {
    name: 'reopen_switch_redirect_skips_the_splice_when_the_chat_was_deleted',
    context: {
      payloads: { s1: S1, s2: payload('s2', 'claude') },
      agentReplies: [{ hold: true, release: {} }, null],
      projects: PROJECTS,
      openChatTabs: ['s1']
    },
    initial: live(),
    steps: [open(), ev('deleteHistorySession', 's1'), ev('releaseHeld'), tick]
  },
  {
    name: 'reopen_resplice_skips_the_splice_when_the_chat_was_deleted',
    context: {
      payloads: {
        s1: { hold: true, release: S1 },
        s2: payload('s2', 'claude', [], { continuedFrom: 's1' })
      },
      agentReplies: [null],
      projects: PROJECTS
    },
    initial: live(),
    steps: [open('s2'), ev('deleteHistorySession', 's2'), ev('releaseHeld'), tick]
  },
  {
    name: 'reopen_resplices_the_sources_of_a_switch_target',
    context: {
      payloads: {
        s1: S1,
        s2: payload('s2', 'claude', [], { continuedFrom: 's1' })
      },
      agentReplies: [{}],
      projects: PROJECTS
    },
    initial: live(),
    steps: [open('s2'), tick]
  },
  {
    name: 'reopen_resplices_two_sources_oldest_first',
    context: {
      payloads: {
        s1: S1,
        s2: payload('s2', 'claude', [marker('s3', 4, 'claude', 'gemini')], {
          continuedFrom: 's1'
        }),
        s3: payload('s3', 'gemini', [], { continuedFrom: 's2' })
      },
      agentReplies: [{}],
      projects: PROJECTS
    },
    initial: live(),
    steps: [open('s3'), tick]
  },
  {
    name: 'reopen_resplice_fabricates_a_boundary_for_a_source_without_a_marker',
    context: {
      payloads: {
        s1: payload('s1', 'gemini'),
        s2: payload('s2', 'claude', [], { continuedFrom: 's1' })
      },
      agentReplies: [{}],
      projects: PROJECTS
    },
    initial: live(),
    steps: [open('s2'), tick]
  },
  {
    name: 'reopen_resplice_fabricates_a_boundary_from_the_activity_time',
    context: {
      payloads: {
        s1: payload('s1', 'gemini'),
        s2: payload('s2', 'claude', [], { continuedFrom: 's1' })
      },
      agentReplies: [{}],
      projects: PROJECTS
    },
    initial: live({
      sessionIndex: [
        { id: 's1', title: 'Chat s1', projectId: 'p1' },
        { id: 's2', title: 'Chat s2', projectId: 'p1' }
      ]
    }),
    steps: [open('s2'), tick]
  },
  {
    name: 'reopen_resplice_stops_when_a_source_has_no_history',
    context: {
      payloads: { s2: payload('s2', 'claude', [], { continuedFrom: 's1' }) },
      agentReplies: [{}],
      projects: PROJECTS
    },
    initial: live(),
    steps: [open('s2'), tick]
  },
  {
    name: 'reopen_resplice_stops_at_an_edge_cycle',
    context: {
      payloads: {
        s1: payload('s1', 'gemini', [marker('s2', 4)], { continuedFrom: 's2' }),
        s2: payload('s2', 'claude', [], { continuedFrom: 's1' })
      },
      agentReplies: [{}],
      projects: PROJECTS
    },
    initial: live(),
    steps: [open('s2'), tick]
  },
  {
    name: 'reopen_resplice_reads_the_host_copy_of_a_source',
    context: {
      payloads: { s1: S1, s2: payload('s2', 'claude', [], { continuedFrom: 's1' }) },
      hostPayloads: { s1: payload('s1', 'gemini', [marker('s2', 6)], { lastSeq: 8 }) },
      agentReplies: [{}],
      projects: PROJECTS
    },
    initial: live(),
    steps: [open('s2'), tick]
  },
  {
    name: 'create_session_lookup_of_a_switched_chat_answers_once_the_continuation_opens',
    context: {
      payloads: { s1: S1, s2: payload('s2', 'claude') },
      agentReplies: [{ sessionId: 's1', lookup: true }, {}],
      projects: PROJECTS
    },
    initial: live(),
    steps: [ev('createSession', AGENT, '/work', [], 'p1'), tick]
  }
]
