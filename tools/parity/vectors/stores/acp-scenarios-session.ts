// The ACP store's session lifecycle scenarios for `acp.ts`. Each restates a
// case of `events.test.ts`, `lifecycle.test.ts`, `warm-pool.test.ts` or
// `turn-error-reopen.test.ts`.
import type { Scenario } from '../record'
import { AGENT, ev, idle, message, S, seeded, watermark } from './acp-fixtures'

const created = (extra: Record<string, unknown> = {}, sessionId = 's2') =>
  ev('_onSessionCreated', { agentId: AGENT, sessionId, ...extra })

const closed = (agentId: string | undefined = AGENT, sessionId = S) =>
  ev('_onSessionClosed', { ...(agentId ? { agentId } : {}), sessionId, reason: 'closed' })

const modes = (current: string, ...ids: string[]) => ({
  currentModeId: current,
  availableModes: ids.map((id) => ({ id, name: id.toUpperCase() }))
})

const models = (current: string, ...ids: string[]) => ({
  currentModelId: current,
  availableModels: ids.map((modelId) => ({ modelId, name: modelId }))
})

const select = (id: string, current: string, ...values: string[]) => ({
  id,
  name: id,
  type: 'select',
  currentValue: current,
  options: values.map((value) => ({ value, name: value }))
})

function subagent(id: string, overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    id: `sub-${id}`,
    rootSessionId: S,
    dispatchToolCallId: id,
    source: 'claude-meta',
    title: 'Explore',
    background: false,
    startedAt: 10,
    caps: { stop: 'session-cancel', transcript: 'live', liveProgress: true, usage: true },
    activity: 'running',
    ...overrides
  }
}

const dispatch = (
  id: string,
  seq: number,
  record: Record<string, unknown>,
  status = 'pending'
) => ({
  toolCallId: id,
  title: 'Task',
  kind: 'other',
  status,
  seq,
  timestamp: seq,
  subagent: record
})

const pending = (sessionId: string) => ({
  pendingPermissions: {
    [`perm-${sessionId}`]: {
      requestId: `perm-${sessionId}`,
      agentId: AGENT,
      sessionId,
      options: [],
      toolCall: null
    }
  },
  pendingQuestions: {
    [`q-${sessionId}`]: {
      questionId: `q-${sessionId}`,
      agentId: AGENT,
      sessionId,
      question: 'q',
      options: []
    }
  }
})

const twoSessions = (overrides: Record<string, unknown> = {}) => {
  const base = seeded(overrides)
  const sessions = base.sessions as Record<string, Record<string, unknown>>
  return {
    ...base,
    sessions: { ...sessions, s9: { ...sessions[S], id: 's9', status: 'active' } },
    messages: { [S]: [message('u1', 'user', 'hi', { seq: 1 })], s9: [] },
    toolCalls: { [S]: [{ toolCallId: 'tc-1', title: 'Read', status: 'completed', seq: 2 }] },
    plans: { [S]: [{ content: 'step', status: 'pending', priority: 'medium' }] },
    commands: { [S]: [{ name: 'help', description: 'Help' }] },
    commandsSeq: { [S]: { logEpoch: 'e1', seq: 3 } },
    sessionUsage: { [S]: { used: 10, size: 100, baselineUsed: 0, source: 'acp' } },
    agentSwitches: { [S]: [] },
    preparedSessions: { 'prep-a': S, 'prep-b': 's9' },
    pendingPermissions: { ...pending(S).pendingPermissions, ...pending('s9').pendingPermissions },
    pendingQuestions: { ...pending(S).pendingQuestions, ...pending('s9').pendingQuestions }
  }
}

export const sessionScenarios: Scenario[] = [
  {
    name: 'session_created_installs_a_stub_with_its_creation_defaults',
    steps: [
      created({
        modes: modes('ask', 'ask', 'code'),
        models: models('m1', 'm1', 'm2'),
        configOptions: [select('effort', 'low', 'low', 'high')]
      }),
      created({}, 's3')
    ]
  },
  {
    name: 'session_created_caches_the_options_for_its_config',
    initial: { configToLiveAgent: { 'cfg-1\u0000/work\u0000p1': AGENT } },
    steps: [
      created({
        models: models('m1', 'm1', 'm2'),
        configOptions: [select('effort', 'low', 'low')]
      }),
      created({}, 's3')
    ]
  },
  {
    name: 'session_created_on_the_web_client_does_not_reload_the_index',
    context: { serverHistory: true },
    steps: [created()]
  },
  {
    name: 'session_created_on_a_known_session_adds_the_options_it_lacked',
    initial: seeded({
      ...idle,
      modes: modes('code', 'ask', 'code'),
      models: models('m2', 'm1', 'm2'),
      configOptions: [select('effort', 'high', 'low', 'high')]
    }),
    steps: [
      created(
        {
          modes: modes('ask', 'ask', 'plan'),
          models: models('m1', 'm3', 'm1'),
          configOptions: [select('effort', 'low', 'high', 'max'), select('speed', 'fast', 'fast')]
        },
        S
      ),
      created({ modes: modes('plan', 'ask', 'code', 'plan') }, S)
    ]
  },
  {
    name: 'session_created_keeps_an_existing_creation_baseline',
    initial: seeded({
      ...idle,
      creationOptionDefaults: { modeId: 'ask', configValues: { effort: 'low' } }
    }),
    steps: [created({ modes: modes('code', 'code') }, S)]
  },
  {
    name: 'session_closed_drops_the_transcript_and_its_prompts',
    initial: twoSessions(),
    steps: [watermark(4), closed(), closed(AGENT, 's9')]
  },
  {
    name: 'session_closed_from_an_old_process_is_ignored',
    initial: twoSessions(),
    steps: [closed('agent-old'), closed(undefined)]
  },
  {
    name: 'session_closed_keeps_an_error_status',
    initial: twoSessions({ status: 'error', lastError: 'crashed' }),
    steps: [closed()]
  },
  {
    name: 'session_closed_keeps_the_transcript_an_open_tab_shows',
    context: { openChatTabs: [S, 's9'] },
    initial: twoSessions(),
    steps: [watermark(4), closed()]
  },
  {
    name: 'session_closed_in_error_drops_the_transcript_an_open_tab_shows',
    context: { openChatTabs: [S] },
    initial: twoSessions({ status: 'error', lastError: 'crashed' }),
    steps: [closed()]
  },
  {
    name: 'session_closed_for_an_unknown_session_drops_what_it_left',
    initial: { ...twoSessions(), sessions: {} },
    steps: [closed(AGENT, S), closed(AGENT, 's9')]
  },
  {
    name: 'session_closed_cancels_running_subagents',
    context: { openChatTabs: [S] },
    initial: seeded(idle, {
      messages: { [S]: [message('u1', 'user', 'go', { seq: 5, timestamp: 5 })] },
      toolCalls: {
        [S]: [
          dispatch('live', 6, subagent('live')),
          dispatch(
            'bg',
            7,
            subagent('bg', {
              background: true,
              caps: { stop: null, transcript: 'final', liveProgress: false, usage: false }
            }),
            'completed'
          ),
          dispatch(
            'old-bg',
            3,
            subagent('old-bg', {
              background: true,
              caps: { stop: null, transcript: 'final', liveProgress: false, usage: false }
            }),
            'completed'
          ),
          dispatch(
            'fg',
            8,
            subagent('fg', {
              caps: { stop: null, transcript: 'none', liveProgress: false, usage: false }
            })
          ),
          dispatch('waiting', 9, subagent('waiting', { activity: 'requires_action', endedAt: 4 })),
          dispatch(
            'done',
            10,
            subagent('done', { activity: 'idle', end: { reason: 'completed' }, endedAt: 9 }),
            'completed'
          )
        ]
      }
    }),
    steps: [closed()]
  },
  {
    name: 'session_closed_with_nothing_running_logs_no_cancel',
    initial: seeded(idle, {
      toolCalls: {
        [S]: [
          dispatch(
            'done',
            2,
            subagent('done', { activity: 'idle', end: { reason: 'failed', error: 'x' } }),
            'failed'
          )
        ]
      }
    }),
    steps: [closed()]
  },
  {
    name: 'set_active_session_points_at_the_chat',
    steps: [ev('setActiveSession', S), ev('setActiveSession', null)]
  },
  {
    name: 'mark_resume_failed_writes_only_on_a_closed_chat',
    initial: {
      ...seeded({ ...idle, status: 'closed' }),
      sessions: {
        [S]: { ...(seeded({ ...idle, status: 'closed' }).sessions as Record<string, object>)[S] },
        s9: {
          ...(seeded(idle).sessions as Record<string, object>)[S],
          id: 's9',
          status: 'active'
        }
      }
    },
    steps: [
      ev('markResumeFailed', 's9', 'agent gone'),
      ev('markResumeFailed', S, 'ACP_CLAUDE_ACCOUNT_SIGNED_OUT: Sign in to Claude'),
      ev('markResumeFailed', S, 'later failure'),
      ev('markResumeFailed', 'nobody', 'x')
    ]
  },
  {
    name: 'mark_resume_failed_strips_the_codex_tag_and_keeps_the_sign_in_bar',
    initial: seeded({ ...idle, status: 'closed', lastError: 'Interrupted by server restart.' }),
    steps: [ev('markResumeFailed', S, 'ACP_CODEX_ACCOUNT_SIGNED_OUT: Codex Account is signed out')]
  },
  {
    name: 'mark_resume_failed_leaves_the_codex_signed_out_message',
    initial: seeded({
      ...idle,
      status: 'closed',
      lastError: 'Codex signed out. Sign in again to continue this chat.'
    }),
    steps: [ev('markResumeFailed', S, 'boom')]
  }
]
