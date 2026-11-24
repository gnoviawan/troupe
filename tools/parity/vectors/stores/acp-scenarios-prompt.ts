// The ACP store's sending scenarios for `acp.ts`. Each restates a case of
// `prompting.test.ts`, `composer.test.ts`, `core.test.ts` or `events.test.ts`.
// `context.dispatch` lists how each dispatched prompt command answers.
import type { Scenario } from '../record'
import { AGENT, ev, idle, message, S, seeded, text } from './acp-fixtures'

const open = { activeTurn: true, openTurnId: 'turn:t1' }

const queued = (id: string, value: string, extra: Record<string, unknown> = {}) => ({
  id,
  blocks: [text(value)],
  createdAt: 1,
  ...extra
})

const complete = (stopReason: string, turnId = 't1') =>
  ev('_onPromptComplete', { agentId: AGENT, sessionId: S, stopReason, turnId })

const sendScenarios: Scenario[] = [
  {
    name: 'a_send_opens_the_turn_and_dispatches',
    initial: seeded(idle, { sessionIndex: [] }),
    steps: [ev('sendPrompt', S, 'hello')]
  },
  {
    name: 'a_blocks_send_shows_its_display_blocks',
    initial: seeded(idle),
    steps: [
      ev('sendPromptBlocks', S, [text('/skill run it')], {
        displayBlocks: [text('[skill] run it')]
      })
    ]
  },
  {
    name: 'a_send_on_a_busy_session_queues',
    initial: seeded(open),
    steps: [
      ev('sendPrompt', S, 'first'),
      ev('sendPromptBlocks', S, [text('second')], { displayBlocks: [text('2nd')] })
    ]
  },
  {
    name: 'a_send_waits_behind_a_stalled_queue_and_drains_it',
    initial: seeded(idle, { promptQueues: { [S]: [queued('q1', 'older')] } }),
    steps: [ev('sendPrompt', S, 'newer')]
  },
  {
    name: 'answers_log_their_receipt_or_stop_reason',
    context: { dispatch: [{ receipt: 'in_progress' }, 'end_turn'] },
    initial: seeded(idle),
    steps: [ev('sendPrompt', S, 'one'), complete('end_turn', 'id-1'), ev('sendPrompt', S, 'two')]
  },
  {
    name: 'a_busy_answer_puts_the_prompt_back_in_the_queue',
    context: { dispatch: [{ busy: true }] },
    initial: seeded({ ...idle, lastError: 'old', lastErrorCode: 3 }),
    steps: [ev('sendPrompt', S, 'retry me'), ev('sendPrompt', S, 'held behind the retry')]
  },
  {
    name: 'a_failed_answer_fails_the_turn',
    context: { dispatch: [{ error: 'boom' }, { agentDead: true }] },
    initial: seeded({ ...idle, lastError: null }),
    steps: [ev('sendPrompt', S, 'one'), ev('sendPrompt', S, 'two')]
  },
  {
    name: 'a_send_to_a_closed_or_unknown_chat_is_rejected',
    initial: seeded({ ...idle, status: 'closed' }),
    steps: [
      ev('sendPrompt', S, 'late'),
      ev('sendPrompt', 's9', 'nobody'),
      ev('sendPromptBlocks', S, [])
    ]
  },
  {
    name: 'a_read_only_chat_guards_every_dispatch',
    initial: seeded(idle, {
      messages: {
        [S]: [message('u0', 'user', 'what changed?', { seq: 1, readOnlyGuard: 'GUARD' })]
      }
    }),
    steps: [ev('sendPromptBlocks', S, [text('and now?')], { displayBlocks: [text('now?')] })]
  },
  {
    name: 'a_terminator_for_another_turn_is_stale',
    initial: seeded(idle),
    steps: [
      ev('sendPrompt', S, 'mine'),
      complete('end_turn', 'older-turn'),
      complete('end_turn', 'id-1')
    ]
  }
]

const queueScenarios: Scenario[] = [
  {
    name: 'queue_items_move_and_clear',
    initial: seeded(open, {
      promptQueues: { [S]: [queued('a', 'A'), queued('b', 'B'), queued('c', 'C')] }
    }),
    steps: [
      ev('moveQueuedPrompt', S, 'c', 0),
      ev('moveQueuedPrompt', S, 'c', 0),
      ev('moveQueuedPrompt', S, 'a', 9),
      ev('moveQueuedPrompt', S, 'nope', 1),
      ev('removeQueuedPrompt', S, 'b'),
      ev('clearQueuedPrompts', S),
      ev('clearQueuedPrompts', S),
      ev('removeQueuedPrompt', 's9', 'x')
    ]
  },
  {
    name: 'the_turn_end_sends_the_next_queued_prompt',
    initial: seeded(open, {
      promptQueues: {
        [S]: [queued('q1', 'next', { displayBlocks: [text('next!')] }), queued('q2', 'later')]
      }
    }),
    steps: [complete('end_turn')]
  },
  {
    name: 'a_queued_send_that_fails_is_logged',
    context: { dispatch: [{ error: 'boom' }] },
    initial: seeded(open, { promptQueues: { [S]: [queued('q1', 'next')] } }),
    steps: [complete('end_turn')]
  },
  {
    name: 'agent_working_stopping_flushes_the_queue',
    initial: seeded(
      { ...idle, agentWorking: true },
      { promptQueues: { [S]: [queued('q1', 'waited')] } }
    ),
    steps: [ev('_onAgentWorking', { agentId: AGENT, sessionId: S, working: false })]
  }
]

const nowScenarios: Scenario[] = [
  {
    name: 'send_now_on_an_idle_chat_sends_the_item',
    initial: seeded(idle, {
      promptQueues: { [S]: [queued('a', 'A'), queued('b', 'B', { displayBlocks: [text('b!')] })] }
    }),
    steps: [ev('sendQueuedPromptNow', S, 'b'), ev('sendQueuedPromptNow', S, 'zzz')]
  },
  {
    name: 'send_now_on_a_busy_chat_stops_the_turn_first',
    initial: seeded(open, { promptQueues: { [S]: [queued('a', 'A'), queued('b', 'B')] } }),
    steps: [ev('sendQueuedPromptNow', S, 'b'), complete('cancelled')]
  },
  {
    name: 'a_failed_send_now_returns_the_item',
    context: { dispatch: [{ error: 'nope' }] },
    initial: seeded(idle, { promptQueues: { [S]: [queued('a', 'A'), queued('b', 'B')] } }),
    steps: [ev('sendQueuedPromptNow', S, 'b')]
  },
  {
    name: 'stop_cancels_the_turn_or_the_agents_own_work',
    initial: seeded(open),
    steps: [
      ev('cancelPrompt', S),
      complete('cancelled'),
      ev('cancelPrompt', S),
      ev('_onAgentWorking', { agentId: AGENT, sessionId: S, working: true }),
      ev('cancelPrompt', S)
    ]
  }
]

export const promptScenarios: Scenario[] = [...sendScenarios, ...queueScenarios, ...nowScenarios]
