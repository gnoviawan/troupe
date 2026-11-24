// The ACP store's `switchAgent` scenarios for `acp.ts` (`slices/switch.ts`):
// the handoff summary, the new agent and session, the armed picks, the
// durable marker, the live splice, the tab remap and the handoff prompt, and
// the rollbacks. Each restates a case of `agent-switch.test.ts`.
import type { Scenario } from '../record'
import { AGENT, ev, idle, message, S, seeded } from './acp-fixtures'

const NUL = '\u0000'
const CONFIGS = [
  { id: 'claude', name: 'Claude', command: 'claude-acp' },
  { id: 'codex', name: 'Codex', command: 'codex-acp' }
]
const PROJECTS = [{ id: 'p1', name: 'Work', path: '/work' }]
const SPAWNED = { agentId: 'agent-9', capabilities: {}, authMethods: [] }
const OPENED = { sessionId: 's2', models: null }
const ROW = { id: S, agentConfigId: 'claude', projectId: 'p1', title: 'Old chat', createdAt: 1 }
const CONVERSATION = [
  message('m1', 'user', 'Fix the login bug', { seq: 1 }),
  message('m2', 'agent', 'Looking at auth.ts now.', { seq: 3 })
]
const READ_CALL = {
  toolCallId: 't1',
  title: 'Read file',
  kind: 'read',
  status: 'completed',
  rawInput: { path: '/work/src/auth.ts', offset: 10, limit: 30 },
  seq: 2
}

/** `s1` on `AGENT` running `claude`, idle, armed for `codex`. */
const armed = (extra: Record<string, unknown> = {}, overrides: Record<string, unknown> = {}) =>
  seeded(
    { ...idle, switching: { toConfigId: 'codex', status: 'pending' }, ...overrides },
    {
      agentConfigs: CONFIGS,
      configToLiveAgent: { [`claude${NUL}/work${NUL}project${NUL}p1`]: AGENT },
      agentStatus: { [AGENT]: 'connected' },
      messages: { [S]: CONVERSATION },
      toolCalls: { [S]: [READ_CALL] },
      sessionIndex: [ROW],
      ...extra
    }
  )
/** `armed()` plus `s5`, a `status` chat on `agent-2`, the live `codex` agent. */
const withCodexChat = (status: string) => {
  const state = armed({
    configToLiveAgent: {
      [`claude${NUL}/work${NUL}project${NUL}p1`]: AGENT,
      [`codex${NUL}/work${NUL}project${NUL}p1`]: 'agent-2'
    },
    agentStatus: { [AGENT]: 'connected', 'agent-2': 'connected' }
  })
  const sessions = state.sessions as Record<string, Record<string, unknown>>
  return {
    ...state,
    sessions: {
      ...sessions,
      s5: { ...sessions[S], id: 's5', agentId: 'agent-2', status, switching: null }
    }
  }
}
const happy = { projects: PROJECTS, agentReplies: [SPAWNED, OPENED, null] }
const switchTo = (pending?: Record<string, unknown>, to = 'codex', session = S) =>
  ev('switchAgent', session, to, ...(pending ? [pending] : []))

export const switchRunScenarios: Scenario[] = [
  {
    name: 'switch_agent_lands_the_conversation_on_the_new_session',
    context: { ...happy, openChatTabs: [S] },
    initial: armed({
      plans: { [S]: [{ content: 'Fix it', status: 'in_progress', priority: 'high' }] }
    }),
    steps: [switchTo({ pendingText: 'Now add a test' })]
  },
  {
    name: 'switch_agent_without_a_draft_sends_the_summary_alone',
    context: happy,
    initial: armed(),
    steps: [
      switchTo(),
      ev(
        '_onUserPrompt',
        {
          agentId: 'agent-9',
          sessionId: 's2',
          turnId: 'id-1',
          content: [{ type: 'text', text: '# Conversation handoff' }]
        },
        5
      ),
      ev('_onUserPrompt', {
        agentId: 'agent-9',
        sessionId: 's2',
        turnId: 'id-1',
        content: [{ type: 'text', text: 'a second echo shows' }]
      })
    ]
  },
  {
    name: 'switch_agent_summary_over_budget_keeps_the_newest_lines',
    context: happy,
    initial: armed({
      messages: {
        [S]: Array.from({ length: 14 }, (_, i) => [
          message(`u${i}`, 'user', `Ask ${i} ${'x'.repeat(300)}`, { seq: 10 * i + 1 }),
          message(`a${i}`, 'agent', `Reply ${i} ${'y'.repeat(150)}`, { seq: 10 * i + 3 })
        ]).flat()
      },
      toolCalls: {}
    }),
    steps: [switchTo({ pendingText: 'go' })]
  },
  {
    name: 'switch_agent_keeps_the_composer_blocks_after_the_summary',
    context: happy,
    initial: armed(),
    steps: [
      switchTo({
        wireBlocks: [
          { type: 'text', text: 'Use the skill  here' },
          { type: 'image', data: 'AAAA', mimeType: 'image/png' }
        ],
        displayBlocks: [{ type: 'text', text: 'Use skill here' }]
      })
    ]
  },
  {
    name: 'switch_agent_on_an_empty_chat_sends_the_draft_alone',
    context: happy,
    initial: armed({ messages: { [S]: [] }, toolCalls: {} }),
    steps: [switchTo({ pendingText: '  first words  ' })]
  },
  {
    name: 'switch_agent_on_an_empty_chat_sweeps_the_composer_blocks',
    context: happy,
    initial: armed({ messages: { [S]: [] }, toolCalls: {} }),
    steps: [
      switchTo({
        wireBlocks: [{ type: 'text', text: 'Run review now' }],
        displayBlocks: [{ type: 'text', text: 'Run review now' }]
      })
    ]
  },
  {
    name: 'switch_agent_stamps_the_continuation_on_an_indexed_new_session',
    context: happy,
    initial: armed({
      sessionIndex: [ROW, { id: 's2', agentConfigId: 'codex', projectId: 'p1', createdAt: 2 }]
    }),
    steps: [switchTo({ pendingText: 'go' })]
  },
  {
    name: 'switch_agent_on_an_empty_chat_without_a_draft_sends_nothing',
    context: happy,
    initial: armed({ messages: { [S]: [] }, toolCalls: {} }),
    steps: [switchTo()]
  },
  {
    name: 'switch_agent_spawn_failure_rolls_back',
    context: { projects: PROJECTS, agentReplies: [{ reject: 'codex-acp: command not found' }] },
    initial: armed(),
    steps: [switchTo({ pendingText: 'go' })]
  },
  {
    name: 'switch_agent_on_a_chat_with_no_folder_rolls_back',
    context: happy,
    initial: armed({}, { cwd: '' }),
    steps: [switchTo({ pendingText: 'go' })]
  },
  {
    name: 'switch_agent_session_failure_stops_the_agent_spawned_for_it',
    context: {
      projects: PROJECTS,
      agentReplies: [SPAWNED, { reject: 'session/new failed' }, null]
    },
    initial: armed(),
    steps: [switchTo({ pendingText: 'go' })]
  },
  {
    name: 'switch_agent_session_failure_logs_a_failed_kill',
    context: {
      projects: PROJECTS,
      agentReplies: [SPAWNED, { reject: 'session/new failed' }, { reject: 'kill failed' }]
    },
    initial: armed(),
    steps: [switchTo({ pendingText: 'go' })]
  },
  {
    name: 'switch_agent_spawns_its_own_agent_beside_a_chat_that_holds_one',
    context: { projects: PROJECTS, agentReplies: [{ reject: 'session/new failed' }] },
    initial: withCodexChat('active'),
    steps: [switchTo({ pendingText: 'go' })]
  },
  {
    name: 'switch_agent_session_failure_stops_a_reused_agent_no_chat_uses',
    context: { projects: PROJECTS, agentReplies: [{ reject: 'session/new failed' }, null] },
    initial: withCodexChat('closed'),
    steps: [switchTo({ pendingText: 'go' })]
  },
  {
    name: 'switch_agent_session_failure_keeps_an_agent_a_history_open_waits_on',
    context: {
      projects: PROJECTS,
      payloads: { s5: { hold: true, release: null } },
      agentReplies: [{ reject: 'session/new failed' }]
    },
    initial: withCodexChat('closed'),
    steps: [ev('openHistorySession', 's5'), switchTo({ pendingText: 'go' })]
  },
  {
    name: 'switch_agent_marker_failure_does_not_stop_the_switch',
    context: { projects: PROJECTS, agentReplies: [SPAWNED, OPENED, { reject: 'disk full' }] },
    initial: armed(),
    steps: [switchTo({ pendingText: 'go' })]
  },
  {
    name: 'switch_agent_dispatch_failure_banners_the_new_session',
    context: { ...happy, dispatch: [{ error: 'prompt refused' }] },
    initial: armed(),
    steps: [switchTo({ pendingText: 'go' })]
  },
  {
    name: 'switch_agent_busy_dispatch_queues_the_handoff',
    context: { ...happy, dispatch: [{ busy: true }] },
    initial: armed(),
    steps: [switchTo({ pendingText: 'go' })]
  },
  {
    name: 'switch_agent_busy_summary_only_dispatch_shows_its_bubble',
    context: { ...happy, dispatch: [{ busy: true }] },
    initial: armed(),
    steps: [switchTo()]
  },
  {
    name: 'switch_agent_is_regated_when_a_turn_started',
    initial: armed({}, { activeTurn: true, openTurnId: 'turn-1' }),
    steps: [switchTo({ pendingText: 'go' })]
  },
  {
    name: 'switch_agent_refuses_an_unknown_config',
    initial: armed(),
    steps: [switchTo({ pendingText: 'go' }, 'gemini')]
  },
  {
    name: 'switch_agent_refuses_the_agent_that_owns_the_chat',
    initial: armed(),
    steps: [switchTo({ pendingText: 'go' }, 'claude')]
  },
  {
    name: 'switch_agent_continues_a_stopped_chat_on_the_same_config',
    context: { projects: PROJECTS, agentReplies: [OPENED, null] },
    initial: armed({}, { status: 'closed' }),
    steps: [switchTo({ pendingText: 'go' }, 'claude')]
  },
  {
    name: 'switch_agent_on_a_missing_chat_is_rejected',
    initial: armed(),
    steps: [switchTo({ pendingText: 'go' }, 'codex', 's9')]
  },
  {
    name: 'switch_agent_runs_once_for_two_rapid_sends',
    context: {
      projects: PROJECTS,
      agentReplies: [{ hold: true, release: SPAWNED }, OPENED, null]
    },
    initial: armed(),
    steps: [
      switchTo({ pendingText: 'go' }),
      switchTo({ pendingText: 'again' }),
      ev('setSwitchPendingOption', S, { modeId: 'code' }),
      ev('releaseHeld')
    ]
  },
  {
    name: 'switch_agent_applies_armed_picks_before_the_handoff',
    context: {
      ...happy,
      agentReplies: [
        SPAWNED,
        {
          sessionId: 's2',
          models: null,
          modes: { currentModeId: 'chat', availableModes: [{ id: 'chat' }, { id: 'code' }] },
          configOptions: [
            {
              id: 'thought_level',
              name: 'thought_level',
              category: null,
              type: 'select',
              currentValue: 'low',
              options: [
                { value: 'low', name: 'low' },
                { value: 'max', name: 'max' }
              ]
            }
          ]
        },
        null
      ]
    },
    initial: armed(
      {},
      {
        switching: {
          toConfigId: 'codex',
          status: 'pending',
          pendingOptions: { modeId: 'code', configValues: { thought_level: 'max' } }
        }
      }
    ),
    steps: [switchTo({ pendingText: 'go' })]
  },
  {
    name: 'switch_agent_staged_send_carries_the_draft',
    context: happy,
    initial: armed(),
    steps: [ev('sendPrompt', S, 'carry this over')]
  },
  {
    name: 'switch_agent_staged_block_send_carries_the_composer_blocks',
    context: happy,
    initial: armed(),
    steps: [
      ev('sendPromptBlocks', S, [{ type: 'text', text: 'wire text' }], {
        displayBlocks: [{ type: 'text', text: 'shown text' }]
      })
    ]
  },
  {
    name: 'switch_agent_second_hop_appends_to_the_ordered_agents',
    context: { ...happy, openChatTabs: [S] },
    initial: armed({
      sessionIndex: [{ ...ROW, agents: ['gemini', 'claude'] }],
      agentSwitches: {
        [S]: [
          {
            id: 'switch:seq-9',
            fromConfigId: 'gemini',
            toConfigId: 'claude',
            newSessionId: 's0',
            summaryText: 'Earlier handoff',
            timestamp: 1,
            seq: 9
          }
        ]
      }
    }),
    steps: [switchTo({ pendingText: 'go' })]
  },
  {
    name: 'switch_agent_keeps_a_marker_the_host_already_sent',
    context: happy,
    initial: armed(),
    steps: [
      ev('_onAgentSwitch', {
        agentId: AGENT,
        sessionId: S,
        fromConfigId: 'claude',
        toConfigId: 'codex',
        newSessionId: 's2',
        summaryText: 'Early'
      }),
      switchTo({ pendingText: 'go' })
    ]
  },
  {
    name: 'switch_agent_summary_keeps_the_intent_and_the_newest_turns',
    context: happy,
    initial: armed({
      messages: {
        [S]: [
          ...Array.from({ length: 15 }, (_, i) => [
            message(`u${i}`, 'user', `Question number ${i}\nwith a second line`, {
              seq: 10 * i + 1
            }),
            message(`a${i}`, 'agent', `Answer ${i}`, { seq: 10 * i + 3 })
          ]).flat(),
          message('th', 'thought', 'thinking hard', { seq: 200 }),
          message('st', 'agent', 'still typing', { seq: 201, streaming: true }),
          message('img', 'user', '', {
            seq: 202,
            blocks: [{ type: 'image', data: 'AAAA', mimeType: 'image/png' }]
          }),
          message('plan', 'agent', 'Plan below\n```termul-plan\n{"entries":[]}\n```\nDone.', {
            seq: 205
          })
        ]
      },
      toolCalls: {
        [S]: [
          {
            toolCallId: 'k1',
            kind: 'execute',
            status: 'completed',
            rawInput: { command: 'npm test' },
            seq: 12
          },
          {
            toolCallId: 'k2',
            kind: 'edit',
            status: 'completed',
            title: 'Edit',
            content: [{ type: 'diff', path: '/work/a.ts', oldText: 'a\nb', newText: 'a\nc\nd' }],
            seq: 203
          },
          {
            toolCallId: 'k3',
            kind: 'search',
            status: 'in_progress',
            rawInput: { pattern: 'TODO' },
            seq: 204
          },
          {
            toolCallId: 'k4',
            kind: 'think',
            status: 'completed',
            rawInput: { thought: 'hmm' },
            seq: 206
          },
          {
            toolCallId: 'k5',
            kind: 'fetch',
            status: 'completed',
            rawInput: { url: 'https://example.com' },
            seq: 207
          },
          { toolCallId: 'k6', status: 'completed', title: '  Custom tool  ', seq: 208 },
          {
            toolCallId: 'k7',
            kind: 'read',
            status: 'completed',
            rawInput: { file_path: '/work/dir/' },
            seq: 209
          },
          {
            toolCallId: 'k8',
            kind: 'other',
            status: 'completed',
            rawInput: { todos: [] },
            title: 'TodoWrite',
            seq: 210
          },
          {
            toolCallId: 'k9',
            kind: 'edit',
            status: 'completed',
            rawInput: { path: '/work/new.ts' },
            diffStat: { added: 4, removed: 0 },
            seq: 211
          },
          { toolCallId: 'k0', kind: 'execute', status: 'completed', rawInput: { command: 'ls' } }
        ]
      }
    }),
    steps: [switchTo({ pendingText: 'go' })]
  }
]
