// The ACP store's live window scenarios for `acp.ts`. Each restates a case
// of `transcript-window.test.ts`. They keep the transcripts just over the
// bounds (300 messages, 500 tool calls), with small records, because every
// step snapshots the whole state.
import type { Scenario } from '../record'
import { chunk, ev, S, seeded, toolCall, toolUpdate } from './acp-fixtures'

const DURABLE = { metadata: { id: S }, messages: [] }

/** `count` finished messages, oldest first, then `streaming` streaming ones. */
function history(count: number, streaming = 0): Record<string, unknown>[] {
  return Array.from({ length: count + streaming }, (_, index) => ({
    id: `m${index}`,
    role: 'user',
    blocks: [],
    streaming: index >= count,
    timestamp: 1
  }))
}

/** An agent chunk that starts a message of its own. */
const fresh = (id: string) => chunk(id, { messageId: id })

/** `count` small tool calls, oldest first, with the status `status` gives. */
function calls(count: number, status: (index: number) => string): Record<string, unknown>[] {
  return Array.from({ length: count }, (_, index) => ({
    toolCallId: `c${index}`,
    status: status(index)
  }))
}

const finished = () => 'completed'

export const windowScenarios: Scenario[] = [
  {
    name: 'live_window_trims_the_oldest_once_the_history_is_durable',
    context: { payloads: { [S]: DURABLE } },
    initial: seeded({}, { messages: { [S]: history(300) } }),
    // The reader back at the live edge trims at once, without a flush.
    steps: [fresh('a'), ev('clearSessionBackfill', S), fresh('b')]
  },
  {
    name: 'live_window_keeps_a_streaming_tail_longer_than_the_bound',
    context: { payloads: { [S]: DURABLE } },
    initial: seeded({}, { messages: { [S]: history(0, 301) } }),
    steps: [fresh('a'), fresh('b')]
  },
  {
    name: 'live_window_never_trims_a_session_without_durable_history',
    initial: seeded({}, { messages: { [S]: history(300) } }),
    steps: [fresh('a'), fresh('b')]
  },
  {
    name: 'live_window_probes_again_after_a_failed_probe',
    context: { payloads: { [S]: { reject: 'disk gone' } } },
    initial: seeded({}, { messages: { [S]: history(301) } }),
    steps: [fresh('a'), fresh('b')]
  },
  {
    name: 'live_window_is_not_trimmed_by_a_replay_or_a_covered_event',
    context: { payloads: { [S]: DURABLE } },
    initial: seeded({ replaying: 'streaming' }, { messages: { [S]: history(301) } }),
    steps: [fresh('a')]
  },
  {
    name: 'live_window_trims_after_a_tool_event',
    context: { payloads: { [S]: DURABLE } },
    initial: seeded({}, { messages: { [S]: history(301) } }),
    steps: [
      toolCall({ toolCallId: 't1', title: 'Read', status: 'pending' }),
      toolUpdate({ toolCallId: 'missing', status: 'completed' })
    ]
  },
  {
    name: 'live_tool_calls_plateau_at_the_bound_keeping_in_flight_calls',
    initial: seeded(
      {},
      { toolCalls: { [S]: calls(500, (i) => (i < 3 ? 'pending' : 'completed')) } }
    ),
    steps: [
      toolCall({ toolCallId: 'n1', title: 'Run', status: 'in_progress' }),
      toolUpdate({ toolCallId: 'c3', status: 'completed', title: 'gone' })
    ]
  },
  {
    name: 'live_tool_calls_keep_a_finished_subagent_dispatch',
    initial: seeded(
      {},
      {
        toolCalls: {
          [S]: [
            {
              toolCallId: 'd1',
              status: 'completed',
              seq: 0,
              subagent: { id: 'sub-1', title: 'Explore' }
            },
            ...calls(500, finished)
          ]
        }
      }
    ),
    steps: [toolCall({ toolCallId: 'n1', title: 'Run', status: 'completed' })]
  },
  {
    name: 'live_tool_calls_leave_a_summary_of_a_dropped_file_change',
    initial: seeded(
      {},
      {
        toolCalls: {
          [S]: [
            {
              toolCallId: 'e1',
              kind: 'edit',
              status: 'completed',
              seq: 0,
              timestamp: 1,
              locations: [{ path: ' /w/a.ts ' }],
              rawInput: { path: '/w/a.ts' },
              content: [
                { type: 'diff', path: '/w/a.ts', oldText: 'a\nb\nc\n', newText: 'a\nB\nc\nd\n' },
                { type: 'diff', path: '/w/b.ts', oldText: null, newText: 'x\r\ny\n' }
              ]
            },
            {
              toolCallId: 'm1',
              kind: 'move',
              status: 'failed',
              locations: [{ path: '/w/old.ts' }, { path: '/w/new.ts' }]
            },
            {
              toolCallId: 'e2',
              kind: 'edit',
              status: 'completed',
              rawInput: { file_path: '  /w/c.ts' },
              diffStat: { added: 4, removed: 2 }
            },
            {
              toolCallId: 'e3',
              kind: 'edit',
              status: 'completed',
              content: [{ type: 'diff', path: '/w/d.ts', oldText: 'gone\n', newText: '' }]
            },
            { toolCallId: 'r1', kind: 'read', status: 'completed', locations: [{ path: '/w/r' }] },
            {
              toolCallId: 'e4',
              kind: 'edit',
              status: 'completed',
              locations: [{ path: '/w/e.ts' }],
              diffStat: { added: 1.5, removed: 0 }
            },
            ...calls(499, finished)
          ]
        }
      }
    ),
    // 506 calls: the six above drop, and the file changes leave summaries.
    steps: [toolCall({ toolCallId: 'n1', title: 'Run', status: 'completed' })]
  },
  {
    name: 'live_tool_calls_over_the_bound_in_flight_keep_the_newest',
    initial: seeded({}, { toolCalls: { [S]: calls(500, () => 'pending') } }),
    steps: [toolCall({ toolCallId: 'n1', title: 'Run', status: 'pending' })]
  }
]
