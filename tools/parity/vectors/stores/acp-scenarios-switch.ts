// The ACP store's agent-switch scenarios for `acp.ts`: `armAgentSwitch` and
// its busy gate, `_onAgentSwitch`, `cancelAgentSwitch` and the continuation edges
// (`slices/switch.ts`, `stampSwitchContinuation`). Each restates a case of
// `agent-switch.test.ts`.
import type { Scenario } from '../record'
import { AGENT, ev, idle, S, seeded } from './acp-fixtures'

const switched = (extra: Record<string, unknown> = {}, seq?: number) =>
  ev(
    '_onAgentSwitch',
    {
      agentId: AGENT,
      sessionId: S,
      fromConfigId: 'claude',
      toConfigId: 'codex',
      newSessionId: 's2',
      summaryText: 'Handoff summary',
      ...extra
    },
    ...(seq === undefined ? [] : [seq])
  )
const index = (s2: Record<string, unknown> = {}) => ({
  sessionIndex: [
    { id: S, projectId: 'p1' },
    { id: 's2', projectId: 'p1', ...s2 }
  ]
})
const NUL = '\u0000'
const CONFIGS = [
  { id: 'claude', name: 'Claude', command: 'claude-acp' },
  { id: 'codex', name: 'Codex', command: 'codex-acp' }
]
/** `s1` on `AGENT`, which runs the `claude` config, idle. */
const armable = (extra: Record<string, unknown> = {}, overrides: Record<string, unknown> = {}) =>
  seeded(
    { ...idle, ...overrides },
    {
      agentConfigs: CONFIGS,
      configToLiveAgent: { [`claude${NUL}/work${NUL}project${NUL}p1`]: AGENT },
      agentStatus: { [AGENT]: 'connected' },
      ...extra
    }
  )
const arm = (to = 'codex') => ev('armAgentSwitch', S, to)
const opened = { sessionId: 'w1', models: null }
const SPAWNED = { agentId: 'agent-9', capabilities: {}, authMethods: [] }

export const armScenarios: Scenario[] = [
  {
    name: 'arm_agent_switch_marks_the_chat_and_warms_the_target',
    context: {
      projects: [{ id: 'p1', name: 'Work', path: '/work' }],
      agentReplies: [SPAWNED, opened]
    },
    initial: armable(),
    steps: [arm()]
  },
  {
    name: 'arm_agent_switch_rejects_the_agent_that_owns_the_chat',
    initial: armable(),
    steps: [arm('claude')]
  },
  {
    name: 'arm_agent_switch_reads_the_owner_from_the_index',
    initial: armable({ configToLiveAgent: {}, sessionIndex: [{ id: S, agentConfigId: 'claude' }] }),
    steps: [arm('claude')]
  },
  {
    name: 'arm_agent_switch_rejects_an_unknown_config',
    initial: armable(),
    steps: [arm('gemini')]
  },
  {
    name: 'arm_agent_switch_waits_for_the_turn',
    initial: armable({}, { activeTurn: true, openTurnId: 'turn-1' }),
    steps: [arm()]
  },
  {
    name: 'arm_agent_switch_waits_for_a_restore',
    initial: armable({}, { replaying: 'pending' }),
    steps: [arm()]
  },
  {
    name: 'arm_agent_switch_waits_for_a_launch',
    initial: armable({ launchingSessionIds: { [S]: true } }),
    steps: [arm()]
  },
  {
    name: 'arm_agent_switch_waits_for_queued_prompts',
    initial: armable({
      promptQueues: { [S]: [{ id: 'q1', blocks: [{ type: 'text', text: 'later' }], createdAt: 1 }] }
    }),
    steps: [arm()]
  },
  {
    name: 'arm_agent_switch_waits_for_a_permission',
    initial: armable({
      pendingPermissions: {
        r1: { requestId: 'r1', agentId: AGENT, sessionId: S, toolCall: {}, options: [] }
      }
    }),
    steps: [arm()]
  },
  {
    name: 'arm_agent_switch_waits_for_a_sign_in',
    initial: armable({ pendingSignIn: { [AGENT]: { kind: 'browser', url: 'https://x' } } }),
    steps: [arm()]
  },
  {
    name: 'arm_agent_switch_waits_for_background_commands',
    initial: armable(
      {},
      {
        backgroundCommands: {
          agentId: AGENT,
          revision: 1,
          commands: [{ id: 'c1', command: 'npm run dev', state: 'running' }]
        }
      }
    ),
    steps: [arm()]
  },
  {
    name: 'arm_agent_switch_ignores_finished_or_foreign_background_commands',
    context: {
      projects: [{ id: 'p1', name: 'Work', path: '/work' }],
      agentReplies: [SPAWNED, opened]
    },
    initial: armable(
      {},
      {
        backgroundCommands: {
          agentId: 'agent-old',
          revision: 1,
          commands: [{ id: 'c1', command: 'npm run dev', state: 'running' }]
        }
      }
    ),
    steps: [arm()]
  },
  {
    name: 'arm_agent_switch_waits_for_a_question',
    initial: armable({
      pendingQuestions: {
        q1: { questionId: 'q1', agentId: AGENT, sessionId: S, question: 'q', options: [] }
      }
    }),
    steps: [arm()]
  },
  {
    name: 'arm_agent_switch_waits_for_an_elicitation',
    initial: armable({
      pendingElicitations: {
        e1: {
          requestId: 'e1',
          agentId: AGENT,
          sessionId: S,
          mode: 'form',
          message: 'm',
          fields: []
        }
      }
    }),
    steps: [arm()]
  },
  {
    name: 'arm_agent_switch_waits_for_background_subagents',
    initial: armable({
      toolCalls: {
        [S]: [
          {
            toolCallId: 't1',
            status: 'completed',
            subagent: { background: true, activity: 'running', caps: { liveProgress: false } }
          }
        ]
      }
    }),
    steps: [arm()]
  },
  {
    name: 'arm_agent_switch_ignores_background_commands_that_finished',
    context: {
      projects: [{ id: 'p1', name: 'Work', path: '/work' }],
      agentReplies: [SPAWNED, opened]
    },
    initial: armable(
      {},
      {
        backgroundCommands: {
          agentId: AGENT,
          revision: 1,
          commands: [{ id: 'c1', command: 'npm test', state: 'exited' }]
        }
      }
    ),
    steps: [arm()]
  },
  {
    name: 'arm_agent_switch_ignores_the_commands_of_a_failed_chat',
    context: {
      projects: [{ id: 'p1', name: 'Work', path: '/work' }],
      agentReplies: [SPAWNED, opened]
    },
    initial: armable(
      {},
      {
        status: 'error',
        backgroundCommands: {
          agentId: AGENT,
          revision: 1,
          commands: [{ id: 'c1', command: 'npm run dev', state: 'running' }]
        }
      }
    ),
    steps: [arm()]
  },
  {
    name: 'arm_agent_switch_on_a_missing_chat_is_refused',
    initial: armable(),
    steps: [ev('armAgentSwitch', 's9', 'codex')]
  },
  {
    name: 'cancel_agent_switch_drops_an_armed_switch',
    context: {
      projects: [{ id: 'p1', name: 'Work', path: '/work' }],
      agentReplies: [SPAWNED, opened]
    },
    initial: armable(),
    steps: [arm(), ev('cancelAgentSwitch', S)]
  }
]

const installed = {
  id: 'switch:seq-3',
  fromConfigId: 'claude',
  toConfigId: 'codex',
  newSessionId: 's2',
  summaryText: 'Handoff summary',
  timestamp: 1,
  seq: 3
}

export const switchScenarios: Scenario[] = [
  {
    name: 'agent_switch_adds_a_marker_and_stamps_the_continuation',
    initial: seeded(idle, index()),
    steps: [switched({}, 7)]
  },
  {
    name: 'agent_switch_without_a_seq_takes_the_next_one',
    initial: seeded(idle, index()),
    steps: [switched()]
  },
  {
    name: 'agent_switch_updates_the_installed_marker_with_the_same_content',
    initial: seeded(idle, { ...index(), agentSwitches: { [S]: [installed] } }),
    steps: [switched({ fromConfigId: 'gemini' }, 9)]
  },
  {
    name: 'agent_switch_updates_the_marker_with_the_same_id',
    initial: seeded(idle, { ...index(), agentSwitches: { [S]: [installed] } }),
    steps: [switched({ summaryText: 'Newer summary' }, 3)]
  },
  {
    name: 'agent_switch_with_another_summary_adds_a_marker',
    initial: seeded(idle, { ...index(), agentSwitches: { [S]: [installed] } }),
    steps: [switched({ summaryText: 'Second handoff' }, 9)]
  },
  {
    name: 'cancel_agent_switch_without_an_armed_switch_changes_nothing',
    initial: seeded(idle),
    steps: [ev('cancelAgentSwitch', S)]
  },
  {
    name: 'agent_switch_on_a_closed_chat_is_dropped',
    initial: seeded({ ...idle, status: 'closed' }, index()),
    steps: [switched({}, 7)]
  },
  {
    name: 'agent_switch_covered_by_history_is_dropped',
    initial: seeded(idle, index()),
    steps: [
      ev('noteHistoryWatermark', S, { metadata: { id: S, lastSeq: 8 }, messages: [] }),
      switched({}, 7)
    ]
  },
  {
    name: 'agent_switch_keeps_an_existing_continuation',
    initial: seeded(idle, index({ continuedFrom: 's0' })),
    steps: [switched({}, 7)]
  },
  {
    name: 'agent_switch_never_stamps_across_projects',
    initial: seeded(idle, index({ projectId: 'p2' })),
    steps: [switched({}, 7)]
  },
  {
    name: 'cancel_agent_switch_clears_the_armed_switch',
    initial: seeded({ ...idle, switching: { toConfigId: 'codex', status: 'pending' } }),
    steps: [ev('cancelAgentSwitch', S), ev('cancelAgentSwitch', 'missing')]
  },
  {
    name: 'history_watermark_counts_switch_markers',
    initial: seeded(idle),
    steps: [
      ev('noteHistoryWatermark', S, {
        metadata: { id: S, lastSeq: 2 },
        messages: [],
        switches: [{ ...installed, seq: 12 }, { seq: 40 }]
      })
    ]
  }
]
