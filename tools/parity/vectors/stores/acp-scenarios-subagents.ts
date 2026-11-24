// The ACP store's subagent and approval scenarios for `acp.ts`. Each restates
// a case of `subagent-lifecycle.test.ts`, `subagent-transcript.test.ts`,
// `child-approvals.test.ts`, `browser-consent.test.ts` or `events.test.ts`.
import type { Scenario } from '../record'
import {
  AGENT,
  big,
  ev,
  idle,
  S,
  seeded,
  text,
  toolCall,
  toolUpdate,
  watermark
} from './acp-fixtures'

const CHILD = 'child-1'

function record(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    id: 'sub-1',
    rootSessionId: S,
    dispatchToolCallId: 'd1',
    source: 'claude-meta',
    title: 'Explore the repo',
    description: 'look around',
    background: false,
    startedAt: 10,
    caps: { stop: 'session-cancel', transcript: 'live', liveProgress: true, usage: true },
    activity: 'running',
    ...overrides
  }
}

const meta = (value: Record<string, unknown>) => ({ _meta: { 'termul/subagent': value } })

const childChunk = (value: string, extra: Record<string, unknown> = {}, seq?: number) =>
  ev(
    '_onMessageChunk',
    {
      agentId: AGENT,
      sessionId: S,
      role: 'agent',
      content: text(value),
      parentToolCallId: 'd1',
      ...extra
    },
    ...(seq === undefined ? [] : [seq])
  )

const permission = (requestId: string, agentId: string, extra: Record<string, unknown> = {}) =>
  ev('_onPermissionRequest', {
    agentId,
    sessionId: S,
    requestId,
    toolCall: { toolCallId: `call-${requestId}`, title: 'Write file', status: 'pending' },
    options: [{ optionId: 'allow', name: 'Allow', kind: 'allow_once' }],
    ...extra
  })

const question = (questionId: string, agentId: string, extra: Record<string, unknown> = {}) =>
  ev('_onQuestionRequest', {
    agentId,
    sessionId: S,
    questionId,
    question: 'Which one?',
    options: [{ value: 'a', label: 'A' }],
    ...extra
  })

const elicitation = (requestId: string, agentId: string, extra: Record<string, unknown> = {}) =>
  ev('_onElicitationRequest', {
    agentId,
    sessionId: S,
    requestId,
    mode: 'form',
    message: 'Fill this in',
    fields: [{ name: 'pick', title: 'Pick', options: ['one', { value: 'two', label: 'Two' }] }],
    ...extra
  })

const browserTicket = (requestId: string, browser: Record<string, unknown>, options?: unknown) =>
  ev('_onPermissionRequest', {
    agentId: AGENT,
    sessionId: S,
    requestId,
    toolCall: { toolCallId: `call-${requestId}` },
    options: options ?? [],
    _meta: { termul: { origin_kind: 'browser', browser } }
  })

const recordScenarios: Scenario[] = [
  {
    name: 'subagent_record_is_lifted_onto_its_dispatch_call',
    initial: seeded(),
    steps: [
      toolCall({ toolCallId: 'd1', title: 'Task', status: 'in_progress', ...meta(record()) }),
      toolUpdate({
        toolCallId: 'd1',
        status: 'completed',
        ...meta(
          record({
            activity: 'idle',
            end: { reason: 'completed' },
            endedAt: 20,
            usage: { inputTokens: 5, outputTokens: 7, bogus: 1 },
            workspace: { mode: 'worktree', path: '/wt', branch: 'b' },
            model: 'opus',
            parentId: null
          })
        )
      }),
      toolCall({
        toolCallId: 'stop-1',
        title: 'KillShell',
        status: 'pending',
        _meta: { 'termul/subagentControl': { subagentIds: ['sub-1'] } }
      }),
      toolUpdate({ toolCallId: 'd1', title: 'Task done' })
    ]
  },
  {
    name: 'malformed_subagent_records_log_once_per_call',
    initial: seeded(),
    steps: [
      toolCall({
        toolCallId: 'bad-1',
        status: 'pending',
        ...meta(
          record({ caps: { stop: 'kill', transcript: 'live', liveProgress: true, usage: true } })
        )
      }),
      toolUpdate({ toolCallId: 'bad-1', ...meta(record({ caps: null })) }),
      toolCall({
        toolCallId: 'bad-2',
        status: 'pending',
        ...meta(record({ agent: { name: 'x' } }))
      }),
      toolCall({
        toolCallId: 'bad-3',
        status: 'pending',
        ...meta(record({ activity: 'idle', end: { reason: 'nope', error: 3 } }))
      }),
      toolCall({
        toolCallId: 'bad-4',
        status: 'pending',
        ...meta(record({ activity: 'running', end: {} }))
      }),
      toolCall({
        toolCallId: 'bad-5',
        status: 'pending',
        ...meta(record({ usage: { toolUses: -1 } }))
      }),
      toolCall({
        toolCallId: 'bad-6',
        status: 'pending',
        _meta: { 'termul/subagentControl': { subagentIds: ['ok', ''] } }
      }),
      toolCall({ toolCallId: 'bad-7', status: 'pending', ...meta(record({ background: 'no' })) })
    ]
  },
  {
    name: 'subagent_calls_link_to_their_dispatch',
    initial: seeded(),
    steps: [
      toolCall({ toolCallId: 'd1', title: 'Task', status: 'in_progress' }),
      toolCall({
        toolCallId: 'c1',
        title: 'Read',
        status: 'pending',
        _meta: { claudeCode: { parentToolUseId: 'd1' } }
      }),
      toolCall({
        toolCallId: 'self',
        status: 'pending',
        _meta: { claudeCode: { parentToolUseId: 'self' } }
      }),
      toolCall({
        toolCallId: 'long',
        status: 'pending',
        _meta: { claudeCode: { parentToolUseId: big(600, 'p') } }
      })
    ]
  },
  {
    name: 'wire_subagent_messages_are_parsed_and_bounded',
    initial: seeded(),
    steps: [
      toolCall({
        toolCallId: 'd1',
        status: 'completed',
        subagentMessages: [
          { seq: 1, timestamp: 2, text: 'first', messageId: 'm1' },
          { seq: 'x', timestamp: 2, text: 'bad seq' },
          { seq: 3, timestamp: 4, text: 'long id', messageId: big(300, 'i') },
          null
        ]
      }),
      toolCall({
        toolCallId: 'd2',
        status: 'completed',
        subagentMessages: [{ text: 'no stamps' }]
      }),
      toolCall({
        toolCallId: 'd3',
        status: 'completed',
        subagentMessages: [
          { seq: 1, timestamp: 1, text: big(3_000, 'a') },
          { seq: 2, timestamp: 1, text: big(3_000, 'b') },
          { seq: 3, timestamp: 1, text: big(3_000, 'c') }
        ]
      }),
      toolCall({
        toolCallId: 'd4',
        status: 'completed',
        subagentMessages: [
          { seq: 1, timestamp: 1, text: big(100, 'o') },
          { seq: 2, timestamp: 1, text: big(3_000, 'm') },
          { seq: 3, timestamp: 1, text: big(5_000, 'n') }
        ]
      })
    ]
  }
]

const chunkScenarios: Scenario[] = [
  {
    name: 'subagent_text_joins_its_dispatch_call',
    initial: seeded(),
    steps: [
      toolCall({ toolCallId: 'd1', title: 'Task', status: 'in_progress' }),
      childChunk('Looking'),
      childChunk(' around', { messageId: undefined }),
      childChunk('Second', { messageId: 'm2' }),
      childChunk(' message', { messageId: 'm2' }),
      toolCall({
        toolCallId: 'c1',
        status: 'pending',
        _meta: { claudeCode: { parentToolUseId: 'd1' } }
      }),
      childChunk('After the tool', { messageId: 'm2' }),
      childChunk('thinking', { role: 'thought' }),
      childChunk(''),
      ev('_onMessageChunk', {
        agentId: AGENT,
        sessionId: S,
        role: 'agent',
        content: { type: 'image', data: 'AA' },
        parentToolCallId: 'd1'
      })
    ]
  },
  {
    name: 'subagent_text_for_an_unknown_dispatch_is_dropped_and_logged_once',
    initial: seeded(),
    steps: [
      childChunk('orphan', { parentToolCallId: 'ghost' }),
      childChunk('orphan again', { parentToolCallId: 'ghost' }),
      childChunk('closed', { sessionId: 's9' })
    ]
  },
  {
    name: 'subagent_text_follows_the_tool_replay_floor',
    initial: seeded(
      {},
      { toolCalls: { [S]: [{ toolCallId: 'd1', status: 'in_progress', seq: 2 }] } }
    ),
    steps: [
      watermark(10),
      ev('noteLiveTurnReplayFloor', S, 6),
      childChunk('covered', {}, 5),
      childChunk('replayed', {}, 8),
      childChunk('live', {}, 12)
    ]
  },
  {
    name: 'subagent_text_skips_a_closed_session',
    initial: seeded(
      { status: 'closed', ...idle },
      { toolCalls: { [S]: [{ toolCallId: 'd1', status: 'in_progress', seq: 2 }] } }
    ),
    steps: [childChunk('late')]
  }
]

const promptScenarios: Scenario[] = [
  {
    name: 'permission_requests_keep_the_first_and_clamp_the_call',
    initial: seeded(),
    steps: [
      permission('r1', AGENT),
      permission('r1', AGENT, { options: [] }),
      permission('r2', AGENT, {
        toolCall: { toolCallId: 'big', status: 'pending', rawOutput: big(40_000) }
      }),
      permission('r3', AGENT, { toolCall: { toolCallId: big(70_000, 'j') } }),
      watermark(10),
      ev(
        '_onPermissionRequest',
        {
          agentId: AGENT,
          sessionId: S,
          requestId: 'old',
          toolCall: { toolCallId: 'o' },
          options: []
        },
        9
      )
    ]
  },
  {
    name: 'questions_keep_the_first_and_cancel_by_id',
    initial: seeded(),
    steps: [
      question('q1', AGENT, { allowFreeText: true, multiSelect: false }),
      question('q1', AGENT, { question: 'Replaced?' }),
      question('q2', AGENT),
      ev('_onQuestionCancelled', { agentId: AGENT, sessionId: S, questionId: 'q1' }),
      ev('_onQuestionCancelled', { agentId: AGENT, sessionId: S, questionId: 'nope' })
    ]
  },
  {
    name: 'elicitations_normalize_string_options',
    initial: seeded(),
    steps: [
      elicitation('e1', AGENT, { allowOther: true, url: 'https://example.com/device' }),
      elicitation('e1', AGENT, { message: 'second' }),
      elicitation('e2', AGENT, { fields: [{ name: 'free' }] }),
      elicitation('e3', AGENT, { fields: undefined })
    ]
  },
  {
    name: 'browser_consent_tickets_ride_the_permission_contract',
    initial: seeded(),
    steps: [
      browserTicket('host:1', {
        origin: 'https://example.com',
        targetUrl: 'https://example.com/pay',
        action: 'payment_submit',
        element: 'Pay now',
        profile: 'ephemeral'
      }),
      browserTicket(
        'host:2',
        {
          origin: '/tmp/a.pdf',
          targetKind: 'file',
          fileName: 'a.pdf',
          targetUrl: 'file:///tmp/a.pdf',
          targetUrlBytes: 99,
          action: 'mystery',
          file: { name: 'a.pdf', bytes: 12 }
        },
        [{ optionId: 'deny' }, { optionId: 'other' }]
      ),
      browserTicket('host:3', { origin: 'https://x.dev' }),
      ev('_onBrowserTicketClosed', { requestId: 'host:1' }),
      ev('_onBrowserTicketClosed', { requestId: 7 })
    ]
  },
  {
    name: 'a_settled_orchestrated_run_dismisses_its_prompts',
    initial: seeded(),
    steps: [
      permission('r-child', CHILD),
      permission('r-main', AGENT),
      question('q-child', CHILD),
      elicitation('e-child', CHILD),
      toolUpdate({
        toolCallId: 'run',
        ...meta(
          record({
            source: 'orchestrated',
            agent: { name: 'Codex', agentId: CHILD },
            activity: 'running'
          })
        )
      }),
      toolCall({
        toolCallId: 'run',
        status: 'completed',
        ...meta(
          record({
            source: 'orchestrated',
            agent: { name: 'Codex', configId: 'codex', agentId: CHILD },
            activity: 'idle',
            end: { reason: 'completed' }
          })
        )
      }),
      toolCall({
        toolCallId: 'run-2',
        status: 'completed',
        ...meta(
          record({
            source: 'orchestrated',
            agent: { name: 'Codex', agentId: CHILD },
            activity: 'idle',
            end: { reason: 'failed', error: 'boom' }
          })
        )
      })
    ]
  }
]

export const subagentScenarios: Scenario[] = [
  ...recordScenarios,
  ...chunkScenarios,
  ...promptScenarios
]
