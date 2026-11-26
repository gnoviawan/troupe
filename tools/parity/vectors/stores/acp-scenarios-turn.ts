// The ACP store's turn-end scenarios for `acp.ts`. Each restates a case of
// `events.test.ts`, `prompting.test.ts`, `plans-commands.test.ts` or
// `chat-title-turn-end.test.ts`.
import type { Scenario } from '../record'
import { AGENT, ev, idle, message, S, seeded, text, watermark } from './acp-fixtures'

const open = { activeTurn: true, openTurnId: 'turn:t1' }

const complete = (stopReason: string, extra: Record<string, unknown> = {}, seq?: number) =>
  ev(
    '_onPromptComplete',
    { agentId: AGENT, sessionId: S, stopReason, turnId: 't1', ...extra },
    ...(seq === undefined ? [] : [seq])
  )

const streamingReply = (value = 'done') =>
  message('a1', 'agent', value, { seq: 2, streaming: true, messageId: 'm1' })

const working = (value: boolean, agentId = AGENT) =>
  ev('_onAgentWorking', { agentId, sessionId: S, working: value })

const background = (revision: number, commands: unknown[], agentId = AGENT) =>
  ev('_onBackgroundCommands', { agentId, sessionId: S, revision, commands })

const prompt = (kind: string, id: string, agentId: string) => ({
  [id]: {
    ...(kind === 'permission' ? { requestId: id, options: [], toolCall: null } : {}),
    ...(kind === 'question' ? { questionId: id, question: 'q', options: [] } : {}),
    ...(kind === 'elicitation' ? { requestId: id, mode: 'form', message: 'm', fields: [] } : {}),
    agentId,
    sessionId: S
  }
})

const endScenarios: Scenario[] = [
  {
    name: 'prompt_complete_closes_the_turn_and_finalizes',
    initial: seeded(open, {
      messages: {
        [S]: [
          message('turn:t1', 'user', 'go', { seq: 1 }),
          message('th', 'thought', 'hmm', { seq: 2, streaming: true }),
          streamingReply()
        ]
      }
    }),
    steps: [complete('end_turn')]
  },
  {
    name: 'stop_reasons_set_their_note',
    initial: seeded(open, { messages: { [S]: [streamingReply()] } }),
    steps: [
      complete('refusal'),
      complete('max_tokens', { turnId: 't2' }),
      complete('cancelled', { turnId: 't3' }),
      complete('weird', { turnId: 't4' }),
      complete('interrupted', { turnId: undefined })
    ]
  },
  {
    name: 'a_failed_turn_keeps_the_agent_error',
    initial: seeded(
      { ...open, lastError: 'Rate limited', lastErrorCode: 429, lastErrorData: { retry: 3 } },
      { agentStatus: { [AGENT]: 'connected' } }
    ),
    steps: [
      complete('error'),
      complete('end_turn', { turnId: 't2' }),
      complete('crashed', { turnId: 't3' }),
      complete('timeout', { turnId: 't4' })
    ]
  },
  {
    name: 'a_turn_ends_once_per_turn_id',
    initial: seeded(open, { messages: { [S]: [streamingReply()] } }),
    steps: [complete('end_turn'), complete('refusal')]
  },
  {
    name: 'end_turn_clears_a_rejected_rate_limit',
    initial: seeded(open, {
      sessionUsage: {
        [S]: {
          used: 5,
          size: 10,
          baselineUsed: 5,
          source: 'reported',
          updatedAt: 1,
          rateLimit: { status: 'rejected', resetsAt: 9 }
        }
      }
    }),
    steps: [complete('error'), complete('end_turn', { turnId: 't2' })]
  },
  {
    name: 'the_plan_settles_and_snapshots_onto_the_reply',
    initial: seeded(open, {
      messages: {
        [S]: [
          message('old', 'agent', 'earlier', { seq: 1 }),
          {
            id: 'a1',
            role: 'agent',
            blocks: [
              { type: 'text', text: 'working on it' },
              { type: 'text', text: '```termul-plan\n[{"content":"stale"}]\n```' },
              { type: 'image', data: 'AA' }
            ],
            streaming: true,
            timestamp: 1,
            seq: 3
          },
          message('switch-splice:x', 'agent', 'spliced', { seq: 4 })
        ]
      },
      plans: {
        [S]: [
          { content: 'one', status: 'completed', id: '1' },
          { content: 'two', status: 'in_progress', priority: 'high' },
          { content: 'three', status: 'in_progress' }
        ]
      }
    }),
    steps: [complete('end_turn')]
  },
  {
    name: 'a_settled_plan_still_snapshots',
    initial: seeded(open, {
      messages: { [S]: [message('a1', 'agent', 'done\n', { seq: 2 })] },
      plans: { [S]: [{ content: 'one', status: 'completed' }] }
    }),
    steps: [complete('end_turn')]
  },
  {
    name: 'turn_end_drops_only_its_agents_prompts',
    initial: seeded(open, {
      pendingPermissions: {
        ...prompt('permission', 'p1', AGENT),
        ...prompt('permission', 'p2', 'child')
      },
      pendingQuestions: prompt('question', 'q1', AGENT),
      pendingElicitations: prompt('elicitation', 'e1', 'child')
    }),
    steps: [complete('end_turn')]
  },
  {
    name: 'prompt_complete_for_an_unknown_session',
    initial: seeded(open, { pendingQuestions: prompt('question', 'q1', AGENT) }),
    steps: [ev('_onPromptComplete', { agentId: AGENT, sessionId: 's9', stopReason: 'end_turn' })]
  },
  {
    name: 'a_covered_prompt_complete_is_history',
    initial: seeded(open),
    steps: [watermark(10), complete('refusal', {}, 7), complete('end_turn', { turnId: 't2' }, 11)]
  },
  {
    name: 'an_active_turn_without_an_id_closes',
    initial: seeded({ activeTurn: true, openTurnId: null }),
    steps: [complete('end_turn', { turnId: undefined })]
  },
  {
    name: 'an_idle_session_schedules_no_close',
    initial: seeded(idle, { messages: { [S]: [streamingReply()] } }),
    steps: [complete('end_turn')]
  },
  {
    name: 'prompt_complete_clears_the_opencode_line_and_persists',
    initial: seeded(
      { ...open, opencodeNotice: { compaction: 'started', retryAttempt: null } },
      { sessionIndex: [{ id: S, title: 'Chat' }] }
    ),
    steps: [complete('end_turn')]
  },
  {
    name: 'a_streaming_user_bubble_is_normalized_at_the_end',
    initial: seeded(open, {
      messages: {
        [S]: [
          message(
            'u1',
            'user',
            '<<<TERMUL_READ_ONLY_GUARD>>>\nguard\n<<<END_TERMUL_READ_ONLY_GUARD>>>\n\nasked',
            { seq: 1, streaming: true }
          ),
          message('u2', 'user', '# Conversation handoff\n\nsummary\n\n---\n\ndraft', {
            seq: 2,
            streaming: true
          }),
          message('u3', 'user', '# Conversation handoff\n\nsummary only', {
            seq: 3,
            streaming: true
          }),
          {
            id: 'u4',
            role: 'user',
            blocks: [text('# Conversation handoff\n\nsummary'), { type: 'image', data: 'AA' }],
            streaming: true,
            timestamp: 1,
            seq: 4
          },
          message('u5', 'user', '# Conversation handoff\n\nagain', {
            seq: 5,
            streaming: true,
            handoffBoundary: true
          })
        ]
      }
    }),
    steps: [complete('end_turn')]
  },
  {
    name: 'a_suppressed_queue_does_not_flush',
    initial: seeded(open, {
      promptQueues: { [S]: [{ id: 'q1', blocks: [text('next')], createdAt: 1 }] },
      suppressQueueFlush: { [S]: true }
    }),
    steps: [complete('end_turn')]
  }
]

const workScenarios: Scenario[] = [
  {
    name: 'agent_working_marks_the_session_busy',
    initial: seeded(idle),
    steps: [working(true), working(true), working(false, 'old-agent'), working(false)]
  },
  {
    name: 'background_commands_keep_the_newest_list',
    initial: seeded(idle),
    steps: [
      background(2, [{ id: 'c1', command: 'npm run dev', state: 'running' }]),
      background(1, [{ id: 'old', command: 'old', state: 'running' }]),
      background(3, [{ id: 'c1', command: 'npm run dev', state: 'exited' }]),
      background(1, [{ id: 'x', command: 'other agent', state: 'running' }], 'old-agent'),
      ev('_onBackgroundCommands', {
        agentId: AGENT,
        sessionId: 's9',
        revision: 1,
        commands: []
      })
    ]
  },
  {
    name: 'background_commands_of_another_agent_are_replaced',
    initial: seeded({
      ...idle,
      backgroundCommands: { agentId: 'old-agent', revision: 9, commands: [] }
    }),
    steps: [background(5, [{ id: 'c2', command: 'cargo watch', state: 'running' }])]
  }
]

export const turnScenarios: Scenario[] = [...endScenarios, ...workScenarios]
