// The ACP store's scenarios for `acp.ts`. Each restates a case of an
// acp-store unit suite (`src/renderer/stores/acp-store/*.test.ts`).
import type { Scenario, VectorEvent } from '../record'

const ev = (action: string, ...args: unknown[]): VectorEvent => ({ action, args })

const S = 's1'
const AGENT = 'agent-1'

function session(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    id: S,
    agentId: AGENT,
    cwd: '/work',
    projectId: 'p1',
    status: 'active',
    title: null,
    activeTurn: true,
    openTurnId: 'seed-turn',
    modes: null,
    models: null,
    configOptions: [],
    lastError: null,
    createdAt: 1,
    ...overrides
  }
}

const idle = { activeTurn: false, openTurnId: null }

function seeded(
  overrides: Record<string, unknown> = {},
  extra: Record<string, unknown> = {}
): Record<string, unknown> {
  return { sessions: { [S]: session(overrides) }, messages: { [S]: [] }, ...extra }
}

function message(
  id: string,
  role: string,
  text: string,
  extra: Record<string, unknown> = {}
): Record<string, unknown> {
  return {
    id,
    role,
    blocks: [{ type: 'text', text }],
    streaming: false,
    timestamp: 1,
    ...extra
  }
}

const text = (value: string) => ({ type: 'text', text: value })

const chunk = (value: string, extra: Record<string, unknown> = {}, seq?: number) =>
  ev(
    '_onMessageChunk',
    { agentId: AGENT, sessionId: S, role: 'agent', content: text(value), ...extra },
    ...(seq === undefined ? [] : [seq])
  )

const prompt = (content: unknown[], extra: Record<string, unknown> = {}, seq?: number) =>
  ev(
    '_onUserPrompt',
    { agentId: AGENT, sessionId: S, content, ...extra },
    ...(seq === undefined ? [] : [seq])
  )

const toolCall = (call: Record<string, unknown>, seq?: number, sessionId = S) =>
  ev(
    '_onToolCall',
    { agentId: AGENT, sessionId, toolCall: call },
    ...(seq === undefined ? [] : [seq])
  )

const toolUpdate = (update: Record<string, unknown>, seq?: number) =>
  ev(
    '_onToolCallUpdate',
    { agentId: AGENT, sessionId: S, update },
    ...(seq === undefined ? [] : [seq])
  )

const watermark = (lastSeq: number, messages: unknown[] = [], toolCalls: unknown[] = []) =>
  ev('noteHistoryWatermark', S, { metadata: { id: S, lastSeq }, messages, toolCalls })

const big = (length: number, char = 'x') => char.repeat(length)

const transcriptScenarios: Scenario[] = [
  {
    name: 'chunks_coalesce_into_one_message',
    initial: seeded(),
    steps: [chunk('Hello '), chunk('world')]
  },
  {
    name: 'message_id_merges_equal_and_splits_new',
    initial: seeded(
      {},
      { messages: { [S]: [message('m-a', 'agent', 'one ', { seq: 1, messageId: 'msg-a' })] } }
    ),
    steps: [chunk('two', { messageId: 'msg-a' }), chunk('next', { messageId: 'msg-b' })]
  },
  {
    name: 'tagged_chunk_never_joins_untagged_bubble',
    initial: seeded(),
    steps: [chunk('plain'), chunk('tagged', { messageId: 'msg-a' })]
  },
  {
    name: 'tool_call_splits_bubbles_with_one_message_id',
    initial: seeded(
      {},
      {
        messages: {
          [S]: [message('m-a', 'agent', 'before', { seq: 1, messageId: 'msg-a', streaming: true })]
        },
        toolCalls: { [S]: [{ toolCallId: 'tc-1', status: 'completed', title: 'Read', seq: 2 }] }
      }
    ),
    steps: [chunk('after', { messageId: 'msg-a' })]
  },
  {
    name: 'roles_do_not_merge',
    initial: seeded(),
    steps: [
      chunk('a'),
      ev('_onMessageChunk', {
        agentId: AGENT,
        sessionId: S,
        role: 'thought',
        content: text('think')
      }),
      chunk('b')
    ]
  },
  {
    name: 'user_chunk_needs_an_open_turn',
    initial: seeded(idle),
    steps: [
      ev('_onMessageChunk', { agentId: AGENT, sessionId: S, role: 'user', content: text('u') }),
      chunk('greeting')
    ]
  },
  {
    name: 'empty_leading_chunk_is_ignored',
    initial: seeded(),
    steps: [chunk(''), chunk('x'), chunk('')]
  },
  {
    name: 'closed_and_unknown_sessions_drop_chunks',
    initial: seeded({ status: 'closed', ...idle }),
    steps: [
      chunk('late'),
      ev('_onMessageChunk', { sessionId: 's9', role: 'agent', content: text('?') })
    ]
  },
  {
    name: 'late_chunk_joins_the_finished_reply',
    initial: seeded(
      {},
      {
        messages: {
          [S]: [
            message('u1', 'user', 'hi', { seq: 1 }),
            message('a1', 'agent', 'hello', { seq: 2 })
          ]
        }
      }
    ),
    steps: [chunk(' there')]
  },
  {
    name: 'replay_replaces_the_transcript_on_desktop',
    initial: seeded(
      { status: 'closed', replaying: 'pending', ...idle },
      {
        messages: {
          [S]: [
            message('switch-splice:old', 'agent', 'old agent', { seq: 1 }),
            message('u1', 'user', 'stale', { seq: 2 })
          ]
        },
        toolCalls: {
          [S]: [
            { toolCallId: 'switch-splice:tc', status: 'completed', seq: 1 },
            { toolCallId: 'stale', status: 'completed', seq: 3 }
          ]
        }
      }
    ),
    steps: [chunk('  '), chunk('Hello'), chunk(' again')]
  },
  {
    name: 'server_replay_window_never_joins_a_restored_bubble',
    context: { serverHistory: true },
    initial: seeded(
      { replaying: 'streaming', ...idle },
      {
        messages: {
          [S]: [
            message('u1', 'user', 'q', { seq: 1 }),
            message('a1', 'agent', 'restored', { seq: 2 })
          ]
        }
      }
    ),
    steps: [chunk('new'), chunk(' text')]
  },
  {
    name: 'history_watermark_drops_covered_events',
    initial: seeded(),
    steps: [
      watermark(
        10,
        [{ id: 'h', role: 'user', blocks: [], seq: 4 }],
        [{ toolCallId: 'h-tc', seq: 7 }]
      ),
      chunk('covered', {}, 9),
      prompt([text('covered prompt')], { turnId: 'old' }, 5),
      toolCall({ toolCallId: 'covered', status: 'pending' }, 8),
      chunk('new', {}, 11),
      toolCall({ toolCallId: 'fresh', status: 'pending' }, 12)
    ]
  },
  {
    name: 'payload_seq_floor_counts_subagent_messages',
    initial: seeded(),
    steps: [
      watermark(
        0,
        [],
        [{ toolCallId: 'd', seq: 50, subagentMessages: [{ seq: 400, timestamp: 1, text: 'x' }] }]
      ),
      chunk('after')
    ]
  },
  {
    name: 'live_turn_replay_floor_restores_covered_tool_cards',
    initial: seeded(),
    steps: [
      watermark(10),
      ev('noteLiveTurnReplayFloor', S, 6),
      toolCall({ toolCallId: 'below', status: 'completed' }, 4),
      toolCall({ toolCallId: 'replayed', status: 'pending' }, 8),
      toolUpdate({ toolCallId: 'replayed', status: 'completed' }, 9),
      chunk('covered text', {}, 8)
    ]
  }
]

const promptScenarios: Scenario[] = [
  {
    name: 'prompt_echo_appends_and_opens_the_turn',
    initial: seeded(idle),
    steps: [prompt([text('do it')], { turnId: 't1' }, 3)]
  },
  {
    name: 'prompt_echo_dedups_by_turn_id_and_still_counts',
    initial: seeded(
      { openTurnId: 'turn:t1' },
      { messages: { [S]: [message('turn:t1', 'user', 'do it', { seq: 1 })] } }
    ),
    steps: [prompt([text('do it')], { turnId: 't1' }), chunk('ok')]
  },
  {
    name: 'prompt_echo_dedups_against_the_trailing_user_blocks',
    initial: seeded(idle, {
      messages: {
        [S]: [
          message('u1', 'user', 'same', { seq: 1 }),
          message('switch-splice:u', 'user', 'other', { seq: 2 })
        ]
      }
    }),
    steps: [prompt([text('same')]), prompt([text('other')])]
  },
  {
    name: 'prompt_echo_on_a_busy_session_keeps_its_turn',
    initial: seeded(),
    steps: [prompt([text('second device')], { turnId: 't2' })]
  },
  {
    name: 'prompt_echo_of_a_replay_or_closed_session_does_not_open_a_turn',
    initial: seeded({ replaying: 'pending', status: 'closed', ...idle }),
    steps: [prompt([text('history')])]
  },
  {
    name: 'prompt_echo_splits_off_the_read_only_guard',
    initial: seeded(idle),
    steps: [
      prompt([
        text(
          '<<<TERMUL_READ_ONLY_GUARD>>>\nread only\n<<<END_TERMUL_READ_ONLY_GUARD>>>\n\n  what changed?'
        ),
        { type: 'image', mimeType: 'image/png', data: 'AAAA' }
      ]),
      prompt([text('<<<TERMUL_READ_ONLY_GUARD>>>\nonly the guard<<<END_TERMUL_READ_ONLY_GUARD>>>')])
    ]
  },
  {
    name: 'prompt_echo_strips_the_handoff_preamble',
    initial: seeded(idle),
    steps: [
      prompt([text('# Conversation handoff\n\nsummary\n\n---\n\n  my draft  ')]),
      prompt([text('# Conversation handoff\n\nonly a summary'), { type: 'image', data: 'BB' }]),
      prompt([text('# Conversation handoff\n\nsummary alone')]),
      prompt([text('# Conversation handoff is my heading')])
    ]
  },
  {
    name: 'prompt_echo_for_an_unknown_session_is_dropped',
    initial: seeded(idle),
    steps: [ev('_onUserPrompt', { sessionId: 's9', content: [text('?')] })]
  }
]

const toolScenarios: Scenario[] = [
  {
    name: 'tool_call_upserts_by_id',
    initial: seeded(),
    steps: [
      toolCall({ toolCallId: 'tc-1', title: 'Read', status: 'pending', kind: 'read' }),
      toolCall({ toolCallId: 'tc-1', title: 'Read file', status: 'in_progress' }),
      toolCall({ toolCallId: 'tc-2', title: 'Edit', status: 'pending', seq: 77, timestamp: 5 })
    ]
  },
  {
    name: 'tool_update_merges_and_ignores_unknown_ids',
    initial: seeded(),
    steps: [
      toolCall({
        toolCallId: 'tc-1',
        title: 'Edit',
        status: 'pending',
        locations: [{ path: '/a.ts' }]
      }),
      toolUpdate({
        toolCallId: 'tc-1',
        status: 'completed',
        content: [{ type: 'diff', path: '/a.ts', oldText: 'a', newText: 'b' }]
      }),
      toolUpdate({ toolCallId: 'tc-1', locations: [{ path: '/b.ts' }] }),
      toolUpdate({ toolCallId: 'nope', status: 'completed' })
    ]
  },
  {
    name: 'tool_events_skip_closed_sessions',
    initial: seeded({ status: 'closed', ...idle }),
    steps: [
      toolCall({ toolCallId: 'tc-1', status: 'pending' }),
      toolCall({ toolCallId: 'x' }, undefined, 's9')
    ]
  },
  {
    name: 'legacy_record_adopts_live_stamps',
    initial: seeded(
      {},
      {
        toolCalls: {
          [S]: [
            { toolCallId: 'legacy-update', status: 'pending', title: 'Old' },
            { toolCallId: 'legacy-call', status: 'pending', title: 'Old' },
            { toolCallId: 'stamped', status: 'pending', seq: 3, timestamp: 9 }
          ]
        }
      }
    ),
    steps: [
      toolUpdate({ toolCallId: 'legacy-update', status: 'completed' }),
      toolCall({ toolCallId: 'legacy-call', status: 'in_progress' }),
      toolUpdate({ toolCallId: 'stamped', status: 'completed' })
    ]
  },
  {
    name: 'live_event_clears_a_restored_summary',
    initial: seeded(
      {},
      {
        toolCalls: {
          [S]: [
            {
              toolCallId: 'r1',
              kind: 'edit',
              status: 'completed',
              restoredSummary: true,
              seq: 2,
              timestamp: 3
            },
            { toolCallId: 'r2', kind: 'edit', status: 'pending', restoredSummary: true, seq: 4 }
          ]
        }
      }
    ),
    steps: [
      toolUpdate({ toolCallId: 'r1', title: 'Edit' }),
      toolCall({ toolCallId: 'r2', title: 'Edit' })
    ]
  },
  {
    name: 'codex_terminal_stream_accumulates',
    initial: seeded(),
    steps: [
      toolCall({
        toolCallId: 'sh',
        kind: 'execute',
        status: 'in_progress',
        _meta: { terminal_output_delta: { data: 'line 1\n' } }
      }),
      toolUpdate({ toolCallId: 'sh', _meta: { terminal_output_delta: { data: 'line 2\n' } } }),
      toolUpdate({
        toolCallId: 'sh',
        status: 'completed',
        _meta: { terminal_exit: { exit_code: 2 } }
      }),
      toolUpdate({ toolCallId: 'sh', _meta: { terminal_output_delta: { data: big(70_000, 'y') } } })
    ]
  }
]

const clampScenarios: Scenario[] = [
  {
    name: 'clamp_raw_output_logs_once',
    initial: seeded(),
    steps: [
      toolCall({ toolCallId: 'clamp-raw', status: 'in_progress', rawOutput: big(40_000) }),
      toolUpdate({ toolCallId: 'clamp-raw', rawOutput: big(50_000, 'z') }),
      toolUpdate({ toolCallId: 'clamp-raw', rawOutput: { small: 'ok', blob: big(40_000) } })
    ]
  },
  {
    name: 'clamp_title_kind_and_locations',
    initial: seeded(),
    steps: [
      toolCall({
        toolCallId: 'clamp-title',
        title: big(2_000, 't'),
        kind: big(1_500, 'k'),
        status: 'pending',
        locations: Array.from({ length: 40 }, (_, i) => ({ path: `/f${i}.ts` }))
      })
    ]
  },
  {
    name: 'clamp_raw_input_keeps_small_fields',
    initial: seeded(),
    steps: [
      toolCall({
        toolCallId: 'clamp-input',
        status: 'pending',
        rawInput: { path: '/a.ts', content: big(70_000) }
      }),
      toolUpdate({ toolCallId: 'clamp-input', rawInput: [big(70_000)] })
    ]
  },
  {
    name: 'clamp_content_items',
    initial: seeded(),
    steps: [
      toolCall({
        toolCallId: 'clamp-content',
        status: 'pending',
        content: [
          { type: 'diff', path: '/a.ts', oldText: big(70_000, 'o'), newText: 'n' },
          { type: 'content', content: { type: 'text', text: big(66_000, 'c') } },
          { type: 'content', content: { type: 'image', data: big(140_000, 'i') } },
          { type: 'terminal', terminalId: 't1' }
        ]
      }),
      toolUpdate({
        toolCallId: 'clamp-content',
        content: Array.from({ length: 5 }, () => ({
          type: 'content',
          content: { type: 'text', text: big(60_000, 'w') }
        }))
      })
    ]
  },
  {
    name: 'clamp_content_block_of_a_chunk_and_echo',
    initial: seeded(),
    steps: [
      ev('_onMessageChunk', {
        agentId: AGENT,
        sessionId: S,
        role: 'agent',
        content: { type: 'image', mimeType: 'image/png', data: big(70_000, 'a') }
      }),
      prompt([text('see'), { type: 'resource', data: big(70_000, 'r') }])
    ]
  },
  {
    name: 'degrade_an_oversized_call_to_its_structure',
    initial: seeded(),
    steps: [
      toolCall({
        toolCallId: 'degrade',
        title: 'Big',
        status: 'pending',
        rawOutput: 'small',
        parentToolCallId: 'p',
        terminalExitCode: 1,
        extraA: big(60_000, 'a'),
        extraB: big(60_000, 'b'),
        extraC: big(60_000, 'c'),
        extraD: big(60_000, 'd'),
        extraE: big(60_000, 'e'),
        extraF: big(60_000, 'f'),
        extraG: big(60_000, 'g')
      })
    ]
  },
  {
    name: 'drop_a_call_whose_id_is_the_payload',
    initial: seeded(),
    steps: [toolCall({ toolCallId: big(70_000, 'i'), status: 'pending' })]
  }
]

const planScenarios: Scenario[] = [
  {
    name: 'todo_write_new_merge_and_clear',
    initial: seeded(),
    steps: [
      toolCall({
        toolCallId: 'todo-1',
        title: 'TodoWrite',
        status: 'pending',
        rawInput: {
          todos: [
            { content: 'a', status: 'pending', id: '1' },
            { content: 'b', status: 'In Progress', priority: 'HIGH' },
            { nope: true }
          ]
        }
      }),
      toolCall({
        toolCallId: 'todo-2',
        title: 'mcp__claude__TodoWrite',
        status: 'pending',
        rawInput: { merge: true, todos: [{ content: 'a2', id: '1', status: 'done' }] }
      }),
      toolCall({
        toolCallId: 'todo-3',
        title: 'TodoWrite',
        status: 'pending',
        rawInput: { merge: true, todos: [] }
      }),
      toolCall({
        toolCallId: 'plan-x',
        title: 'plan',
        status: 'in_progress',
        rawInput: { entries: [] }
      }),
      toolUpdate({ toolCallId: 'plan-x', status: 'completed', rawInput: { entries: [] } })
    ]
  },
  {
    name: 'codex_update_plan_replaces_a_disjoint_list',
    initial: seeded(),
    steps: [
      toolCall({
        toolCallId: 'up-1',
        title: 'update_plan',
        status: 'completed',
        rawInput: { plan: [{ step: 'one', status: 'completed', description: 'first' }] }
      }),
      toolCall({
        toolCallId: 'up-2',
        title: 'update_plan',
        status: 'completed',
        rawInput: { plan: [{ step: 'one', status: 'completed', description: 'first' }] }
      }),
      toolCall({
        toolCallId: 'up-3',
        title: 'Update-Plan',
        status: 'completed',
        rawInput: { plan: [{ text: 'two', status: 'todo' }] }
      }),
      toolCall({
        toolCallId: 'named-in-meta',
        title: 'Tool',
        status: 'completed',
        _meta: { claudeCode: { toolName: 'TodoWrite' } },
        rawInput: { todos: [{ content: 'three', status: 'running' }] }
      })
    ]
  },
  {
    name: 'plan_update_event_sets_and_clears',
    initial: seeded(),
    steps: [
      ev('_onPlanUpdate', {
        agentId: AGENT,
        sessionId: S,
        plan: { entries: [{ content: 'x', status: 'pending', priority: 'low' }] }
      }),
      ev('_onPlanUpdate', { agentId: AGENT, sessionId: S, plan: { entries: [] } }),
      ev('_onPlanUpdate', {
        agentId: AGENT,
        sessionId: 's9',
        plan: { entries: [{ content: 'y' }] }
      })
    ]
  }
]

const commandList = (name: string) => [{ name, description: `${name} command` }]

const updateScenarios: Scenario[] = [
  {
    name: 'commands_update_keeps_the_newest_list',
    initial: seeded(),
    steps: [
      ev(
        '_onCommandsUpdate',
        { sessionId: S, availableCommands: commandList('a') },
        { logEpoch: 'e1', seq: 5 }
      ),
      ev(
        '_onCommandsUpdate',
        { sessionId: S, availableCommands: commandList('old') },
        { logEpoch: 'e1', seq: 3 }
      ),
      ev(
        '_onCommandsUpdate',
        { sessionId: S, availableCommands: commandList('same') },
        { logEpoch: 'e1', seq: 5 }
      ),
      ev(
        '_onCommandsUpdate',
        { sessionId: S, availableCommands: commandList('epoch') },
        { logEpoch: 'e2', seq: 1 }
      ),
      ev('_onCommandsUpdate', { sessionId: S, availableCommands: commandList('bare') }),
      ev('_onCommandsUpdate', { sessionId: 's9', availableCommands: commandList('x') })
    ]
  },
  {
    name: 'session_info_title_and_notices',
    initial: seeded({}, { sessionIndex: [{ id: S, title: 'Untitled Chat 1', projectId: 'p1' }] }),
    steps: [
      ev('_onSessionInfoUpdate', { sessionId: S, title: 'Fix the build' }),
      ev('_onSessionInfoUpdate', { sessionId: S, compaction: { status: 'started' } }),
      ev('_onSessionInfoUpdate', { sessionId: S, retry: { attempt: 2 } }),
      ev('_onSessionInfoUpdate', { sessionId: S }),
      ev('_onSessionInfoUpdate', { sessionId: S, compaction: { status: 'completed' } }),
      ev('_onSessionInfoUpdate', { sessionId: S, retry: null }),
      ev('_onSessionInfoUpdate', { sessionId: S, title: null }),
      ev('_onSessionInfoUpdate', { sessionId: 's9', title: 'nobody' })
    ]
  },
  {
    name: 'session_info_title_of_an_unindexed_session_stays_in_memory',
    initial: seeded(),
    steps: [ev('_onSessionInfoUpdate', { sessionId: S, title: 'Warm' })]
  },
  {
    name: 'usage_update_tracks_tokens_cost_and_limits',
    initial: seeded(),
    steps: [
      ev('_onUsageUpdate', { sessionId: S, used: 1000, size: 200000 }),
      ev('_onUsageUpdate', {
        sessionId: S,
        used: 5000,
        size: 200000,
        cost: { amount: 0.25, currency: 'USD' },
        rateLimit: { status: 'allowed', resetsAt: 10 }
      }),
      ev('_onUsageUpdate', {
        sessionId: S,
        used: 6000,
        size: 200000,
        cost: { amount: 0, currency: 'USD' }
      }),
      ev('_onUsageUpdate', {
        sessionId: S,
        used: 0,
        size: 0,
        limitReached: { status: 'rejected', resetsAt: 20 }
      }),
      ev('_onUsageUpdate', { sessionId: S, used: 0, size: 0 }),
      ev('_onUsageUpdate', { sessionId: 's9', used: 1, size: 2 })
    ]
  },
  {
    name: 'usage_rate_limit_without_tokens_starts_a_record',
    initial: seeded(),
    steps: [
      ev('_onUsageUpdate', {
        sessionId: S,
        used: 0,
        size: 0,
        rateLimit: { status: 'allowed_warning' }
      })
    ]
  }
]

export const acpScenarios: Scenario[] = [
  ...transcriptScenarios,
  ...promptScenarios,
  ...toolScenarios,
  ...clampScenarios,
  ...planScenarios,
  ...updateScenarios
]
