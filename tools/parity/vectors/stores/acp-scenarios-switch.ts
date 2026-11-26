// The ACP store's agent-switch scenarios for `acp.ts`: `armAgentSwitch` and
// its busy gate, `setSwitchPendingOption`, `_onAgentSwitch`, `cancelAgentSwitch`
// and the continuation edges (`slices/switch.ts`, `stampSwitchContinuation`).
// Each restates a case of `agent-switch.test.ts`.
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

const option = (id: string, current: string, values: string[], category?: string) => ({
  id,
  name: id,
  type: 'select',
  currentValue: current,
  options: values.map((value) => ({ value, name: value.toUpperCase() })),
  ...(category ? { category } : {})
})
const TARGET_OPTIONS = [
  option('model', 'm1', ['m1', 'm2'], 'model'),
  option('thought_level', 'low', ['low', 'max']),
  option('permission', 'ask', ['ask', 'auto'], 'mode')
]
/** `s1` armed for `codex`, with `pendingOptions` when given. */
const armed = (pendingOptions?: Record<string, unknown>, extra: Record<string, unknown> = {}) =>
  armable(extra, {
    switching: {
      toConfigId: 'codex',
      status: 'pending',
      ...(pendingOptions ? { pendingOptions } : {})
    }
  })
/** The warm session prepared for `codex` in `/work`, and its key. */
const WARM_KEY = `codex${NUL}/work${NUL}`
const warm = (configOptions: unknown[], models: unknown = null) => ({
  id: 'w1',
  agentId: 'agent-9',
  cwd: '/work',
  projectId: 'p1',
  status: 'active',
  title: null,
  activeTurn: false,
  openTurnId: null,
  modes: null,
  models,
  configOptions,
  lastError: null,
  createdAt: 1
})
const pick = (patch: Record<string, unknown>, session = S) =>
  ev('setSwitchPendingOption', session, patch)

export const pendingPickScenarios: Scenario[] = [
  {
    name: 'set_switch_pending_option_queues_picks_for_the_target',
    initial: armed(undefined, {
      preparedSessions: { [WARM_KEY]: 'w1' }
    }),
    steps: [pick({ modeId: 'code', configValues: { thought_level: 'max' } })]
  },
  {
    name: 'set_switch_pending_option_saves_scoped_values_under_the_warm_model',
    initial: (() => {
      const state = armed(undefined, { preparedSessions: { [WARM_KEY]: 'w1' } })
      const sessions = state.sessions as Record<string, unknown>
      return { ...state, sessions: { ...sessions, w1: warm(TARGET_OPTIONS) } }
    })(),
    steps: [pick({ configValues: { thought_level: 'max', permission: 'auto', gone: 'x' } })]
  },
  {
    name: 'set_switch_pending_option_model_pick_keeps_only_agent_wide_values',
    initial: armed(
      { modeId: 'code', configValues: { thought_level: 'max', permission: 'auto' } },
      { agentOptionsCache: { codex: { configOptions: TARGET_OPTIONS, models: null } } }
    ),
    steps: [pick({ modelId: 'm2', configValues: { thought_level: 'low' } })]
  },
  {
    name: 'set_switch_pending_option_model_pick_drops_the_last_models_values',
    initial: armed(
      { modelId: 'm1', configValues: { thought_level: 'max', permission: 'auto' } },
      { agentOptionsCache: { codex: { configOptions: TARGET_OPTIONS, models: null } } }
    ),
    steps: [pick({ modelId: 'm2' })]
  },
  {
    name: 'set_switch_pending_option_after_a_cancel_is_dropped',
    initial: armable({}, { switching: null }),
    steps: [pick({ modeId: 'code' })]
  },
  {
    name: 'set_switch_pending_option_model_pick_keeps_every_value_with_no_options',
    initial: armed({ modelId: 'm1', configValues: { thought_level: 'max' } }),
    steps: [pick({ modelId: 'm2' })]
  },
  {
    name: 'set_switch_pending_option_value_pick_keeps_the_armed_model',
    initial: armed(
      { modelId: 'm2', configValues: { permission: 'auto' } },
      { agentOptionsCache: { codex: { configOptions: TARGET_OPTIONS, models: null } } }
    ),
    steps: [pick({ configValues: { thought_level: 'max' } }), pick({ modeId: 'plan' })]
  },
  {
    name: 'set_switch_pending_option_reads_the_cached_native_model',
    initial: armed(undefined, {
      agentOptionsCache: {
        codex: {
          configOptions: [option('thought_level', 'low', ['low', 'max'])],
          models: {
            currentModelId: 'gpt-5',
            availableModels: [{ modelId: 'gpt-5', name: 'GPT-5' }]
          }
        }
      }
    }),
    steps: [pick({ configValues: { thought_level: 'max' } })]
  },
  {
    name: 'set_switch_pending_option_prefers_the_warm_session_over_the_cache',
    initial: (() => {
      const state = armed(undefined, {
        preparedSessions: { [WARM_KEY]: 'w1' },
        agentOptionsCache: { codex: { configOptions: TARGET_OPTIONS, models: null } }
      })
      const sessions = state.sessions as Record<string, unknown>
      return { ...state, sessions: { ...sessions, w1: warm([]) } }
    })(),
    steps: [pick({ modelId: 'm2', configValues: { thought_level: 'max' } })]
  },
  {
    name: 'set_switch_pending_option_without_a_pick_only_sets_the_overlay',
    initial: armed({ modeId: 'code' }),
    steps: [pick({}), pick({ configValues: {} })]
  },
  {
    name: 'set_switch_pending_option_drops_picks_when_no_switch_is_armed',
    initial: armable(),
    steps: [pick({ modeId: 'code' }), pick({ modeId: 'code' }, 's9')]
  }
]
