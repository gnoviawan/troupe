// The ACP store's scroll-back scenarios for `acp.ts`: `loadOlderMessages`
// pages older messages from the durable payload in front of the live
// window's head. Each restates a case of `transcript-window.test.ts`,
// `replay-dedup.test.ts` or `stale-cache-reopen.test.ts`. `context.payloads`
// is the cached read, `context.hostPayloads` the host's re-read.
import type { Scenario } from '../record'
import { ev, message, S, seeded } from './acp-fixtures'

/** A persisted row: `role` and text, at host seq `seq`. */
const row = (id: string, role: string, text: string, seq: number) =>
  message(id, role, text, { seq })

const ROWS = [
  row('u1', 'user', 'first', 1),
  row('a1', 'agent', 'one', 2),
  row('u2', 'user', 'second', 3),
  row('a2', 'agent', 'two', 4),
  row('u3', 'user', 'third', 5),
  row('a3', 'agent', 'three', 6)
]
const payload = (rows = ROWS, lastSeq = 6) => ({ metadata: { id: S, lastSeq }, messages: rows })

/** The live window: the persisted rows from `from` on, stamped with their host seq. */
const window = (from: number) => ROWS.slice(from).map((m) => ({ ...m, hostSeq: m.seq as number }))

const older = (count = 2) => ev('loadOlderMessages', S, count)

export const olderScenarios: Scenario[] = [
  {
    name: 'load_older_pages_in_the_messages_before_the_head',
    context: { payloads: { [S]: payload() } },
    initial: seeded({}, { messages: { [S]: window(4) } }),
    steps: [older(), older(), older()]
  },
  {
    name: 'load_older_without_durable_history_does_nothing',
    initial: seeded({}, { messages: { [S]: window(4) } }),
    steps: [older(), older()]
  },
  {
    name: 'load_older_after_a_failed_read_keeps_the_view',
    context: { payloads: { [S]: { reject: 'disk gone' } } },
    initial: seeded({}, { messages: { [S]: window(4) } }),
    steps: [older(), older()]
  },
  {
    name: 'load_older_on_an_empty_window_does_nothing',
    context: { payloads: { [S]: payload() } },
    initial: seeded(),
    steps: [older()]
  },
  {
    name: 'load_older_rereads_a_copy_that_stops_short_of_the_head',
    context: {
      payloads: { [S]: payload(ROWS.slice(0, 4), 4) },
      hostPayloads: { [S]: payload() }
    },
    initial: seeded({}, { messages: { [S]: window(4) } }),
    steps: [older()]
  },
  {
    name: 'load_older_waits_when_the_host_reread_fails',
    context: {
      payloads: { [S]: payload(ROWS.slice(0, 4), 4) },
      hostPayloads: { [S]: { reject: 'host gone' } }
    },
    initial: seeded({}, { messages: { [S]: window(4) } }),
    steps: [older(), older()]
  },
  {
    name: 'load_older_pages_the_cached_copy_when_the_host_has_none',
    context: { payloads: { [S]: payload(ROWS.slice(0, 4), 4) } },
    initial: seeded({}, { messages: { [S]: window(4) } }),
    steps: [older()]
  },
  {
    name: 'load_older_anchors_a_live_bubble_by_its_host_seq',
    context: { payloads: { [S]: payload() } },
    initial: seeded(
      {},
      { messages: { [S]: [{ ...message('msg-x', 'agent', 'three'), seq: 40, hostSeq: 6 }] } }
    ),
    steps: [older(3)]
  },
  {
    name: 'load_older_cannot_anchor_a_head_without_a_seq',
    context: { payloads: { [S]: payload() } },
    initial: seeded({}, { messages: { [S]: [message('msg-x', 'agent', 'lost')] } }),
    steps: [older()]
  },
  {
    name: 'load_older_folds_the_persisted_twin_of_an_unstamped_head',
    context: { payloads: { [S]: payload() } },
    initial: seeded(
      {},
      {
        messages: {
          [S]: [
            { ...message('turn:t1', 'user', 'third'), seq: 50 },
            { ...message('msg-b', 'agent', 'thr'), seq: 51, streaming: true }
          ]
        }
      }
    ),
    steps: [older(10)]
  },
  {
    name: 'load_older_skips_rows_the_window_already_shows',
    context: { payloads: { [S]: payload() } },
    initial: seeded(
      {},
      {
        messages: {
          [S]: [
            { ...message('turn:t1', 'user', 'third'), seq: 50 },
            { ...ROWS[3], hostSeq: 4 },
            { ...message('msg-c', 'agent', 'later'), seq: 52, hostSeq: 2 }
          ]
        }
      }
    ),
    steps: [older(10)]
  },
  {
    name: 'a_second_load_while_one_reads_waits_for_it',
    context: { payloads: { [S]: { hold: true, release: payload() } } },
    initial: seeded({}, { messages: { [S]: window(4) } }),
    steps: [older(), older(), ev('releaseHeld'), older()]
  },
  {
    name: 'load_older_skips_hidden_turns',
    context: {
      payloads: {
        [S]: payload([
          row('u0', 'user', 'kept', 1),
          { ...row('h1', 'user', '', 2), blocks: [] },
          row('h2', 'agent', 'hidden answer', 3),
          ...ROWS.slice(2)
        ])
      }
    },
    initial: seeded({}, { messages: { [S]: window(4) } }),
    steps: [older(5)]
  },
  {
    name: 'load_older_drops_a_row_the_window_shows_under_its_id',
    context: { payloads: { [S]: payload() } },
    initial: seeded(
      {},
      {
        messages: {
          [S]: [
            { ...message('turn:t1', 'user', 'fresh'), seq: 50 },
            { ...ROWS[3], seq: 51 }
          ]
        }
      }
    ),
    steps: [older(10)]
  },
  {
    name: 'load_older_folds_one_row_per_unstamped_twin',
    context: {
      payloads: {
        [S]: payload([
          row('u1', 'user', 'again', 1),
          row('a1', 'agent', 'one', 2),
          row('u2', 'user', 'again', 3),
          row('a2', 'agent', 'two', 4)
        ])
      }
    },
    initial: seeded(
      {},
      { messages: { [S]: [{ ...message('turn:t1', 'user', 'again'), seq: 50 }] } }
    ),
    steps: [older(10)]
  },
  {
    name: 'load_older_never_folds_into_a_stamped_or_persisted_bubble',
    context: {
      payloads: {
        [S]: payload([
          row('u1', 'user', 'stamped', 1),
          row('a1', 'agent', 'persisted', 2),
          row('u2', 'user', 'other', 3),
          row('a2', 'agent', 'two', 4)
        ])
      }
    },
    initial: seeded(
      {},
      {
        messages: {
          [S]: [
            { ...message('turn:t1', 'user', 'fresh'), seq: 50 },
            { ...message('msg-s', 'user', 'stamped'), seq: 51, hostSeq: 9 },
            { ...message('a2', 'agent', 'persisted'), seq: 52 }
          ]
        }
      }
    ),
    steps: [older(10)]
  },
  {
    name: 'load_older_folds_a_streaming_prefix_only_at_the_seam',
    context: {
      payloads: {
        [S]: payload([
          row('a0', 'agent', 'far answer', 1),
          row('u1', 'user', 'q1', 2),
          row('u2', 'user', 'q2', 3),
          row('u3', 'user', 'q3', 4),
          row('u4', 'user', 'q4', 5),
          row('u5', 'user', 'q5', 6),
          row('a5', 'agent', 'near answer', 7),
          row('a6', 'agent', 'end', 8)
        ])
      }
    },
    initial: seeded(
      {},
      {
        messages: {
          [S]: [
            { ...message('turn:t1', 'user', 'fresh'), seq: 50 },
            { ...message('msg-n', 'agent', 'near'), seq: 51, streaming: true },
            { ...message('msg-f', 'agent', 'far'), seq: 52, streaming: true }
          ]
        }
      }
    ),
    steps: [older(10)]
  },
  {
    name: 'load_older_keeps_a_streaming_prefix_far_from_the_seam',
    context: {
      payloads: {
        [S]: payload([
          row('u1', 'user', 'q1', 1),
          row('a1', 'agent', 'far answer', 2),
          row('a2', 'agent', 'end', 3)
        ])
      }
    },
    initial: seeded(
      {},
      {
        messages: {
          [S]: [
            { ...message('turn:t1', 'user', 'fresh'), seq: 50 },
            { ...message('msg-f', 'agent', 'far'), seq: 51, streaming: true },
            ...['x1', 'x2', 'x3', 'x4', 'x5'].map((id, index) => ({
              ...message(id, 'agent', id),
              seq: 52 + index,
              hostSeq: 20 + index
            }))
          ]
        }
      }
    ),
    steps: [older(10)]
  }
]
