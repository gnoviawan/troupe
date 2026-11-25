// The ACP store's one-shot scenarios for `acp.ts`: `generateCommitMessage`,
// `assistTerminal` and `draftBugReport` (`slices/misc.ts`) and the
// collectors that keep their session's events out of every transcript.
// Each restates a case of `commit-message.test.ts`, `assist-terminal.test.ts`
// or `bug-report-draft.test.ts`.
import type { Scenario } from '../record'
import { AGENT, ev } from './acp-fixtures'

const NUL = '\u0000'
const GEMINI = { id: 'gemini', name: 'Gemini', command: 'gemini-acp' }
const T = 't1'

/** `AGENT` connected for the selected `gemini` config in `/work`. */
function live(extra: Record<string, unknown> = {}) {
  return {
    agentConfigs: [GEMINI],
    selectedAgentConfigId: 'gemini',
    agents: { [AGENT]: { id: AGENT, capabilities: {}, authMethods: [] } },
    agentStatus: { [AGENT]: 'connected' },
    configToLiveAgent: { [`gemini${NUL}/work`]: AGENT },
    ...extra
  }
}

const said = (text: string, role = 'agent') =>
  ev('_onMessageChunk', { agentId: AGENT, sessionId: T, role, content: { type: 'text', text } })
const done = (stopReason = 'end_turn') =>
  ev('_onPromptComplete', { agentId: AGENT, sessionId: T, stopReason })
const commit = (diff = 'diff --git a/x b/x') => ev('generateCommitMessage', ' /work ', diff)
const assist = (kind: string, exitCode: unknown = 1, cwd = '/work') =>
  ev('assistTerminal', kind, cwd, ' npm test\nFAIL ', exitCode)
const bug = (input: Record<string, unknown> = {}) =>
  ev('draftBugReport', {
    cwd: '/work',
    userDescription: ' it broke ',
    diagnostics: { os: 'linux', version: '0.5.0' },
    ...input
  })
const OPENED = [{ sessionId: T }, null]

export const oneShotScenarios: Scenario[] = [
  {
    name: 'commit_message_is_generated_and_its_session_disposed',
    context: { agentReplies: OPENED },
    initial: live(),
    steps: [commit(), said('{"summary":" Fix the build ",'), said('"description":" why "}'), done()]
  },
  {
    name: 'commit_message_reads_a_fenced_reply',
    context: { agentReplies: OPENED },
    initial: live(),
    steps: [commit(), said('```json\n{"summary":"Add x"}\n```'), done()]
  },
  {
    name: 'commit_message_refuses_an_empty_diff',
    initial: live(),
    steps: [commit('   ')]
  },
  {
    name: 'commit_message_refuses_a_diff_over_the_cap',
    initial: live(),
    steps: [commit('x'.repeat(120_001))]
  },
  {
    name: 'commit_message_needs_a_selected_agent',
    initial: live({ selectedAgentConfigId: 'missing' }),
    steps: [commit()]
  },
  {
    name: 'commit_message_fails_when_the_agent_uses_a_tool',
    context: { agentReplies: OPENED },
    initial: live(),
    steps: [
      commit(),
      ev('_onToolCall', {
        agentId: AGENT,
        sessionId: T,
        toolCall: { toolCallId: 'c1', title: 'Read', status: 'pending' }
      })
    ]
  },
  {
    name: 'commit_message_fails_on_a_cancelled_turn',
    context: { agentReplies: OPENED },
    initial: live(),
    steps: [commit(), said('{"summary":"x"}'), done('cancelled')]
  },
  {
    name: 'commit_message_rejects_an_invalid_reply',
    context: { agentReplies: OPENED },
    initial: live(),
    steps: [commit(), said('not json'), done()]
  },
  {
    name: 'commit_message_rejects_a_blank_summary',
    context: { agentReplies: OPENED },
    initial: live(),
    steps: [commit(), said('{"summary":"  "}'), done()]
  },
  {
    name: 'commit_message_rejects_a_long_summary',
    context: { agentReplies: OPENED },
    initial: live(),
    steps: [commit(), said(`{"summary":"${'s'.repeat(73)}"}`), done()]
  },
  {
    name: 'commit_message_rejects_a_bad_description',
    context: { agentReplies: OPENED },
    initial: live(),
    steps: [commit(), said('{"summary":"ok","description":3}'), done()]
  },
  {
    name: 'commit_message_fails_on_a_reply_over_the_cap',
    context: { agentReplies: OPENED },
    initial: live(),
    steps: [commit(), said('x'.repeat(20_001)), done()]
  },
  {
    name: 'commit_message_ignores_user_and_thought_chunks',
    context: { agentReplies: OPENED },
    initial: live(),
    steps: [commit(), said('echo', 'user'), said('{"summary":"Only this"}'), done()]
  },
  {
    name: 'commit_message_fails_when_the_dispatch_fails',
    context: { agentReplies: OPENED, dispatch: [{ error: 'send refused' }] },
    initial: live(),
    steps: [commit()]
  },
  {
    name: 'commit_message_fails_when_the_agent_is_unavailable',
    context: { agentReplies: [{ reject: 'spawn failed: no binary' }] },
    initial: live({ configToLiveAgent: {} }),
    steps: [commit()]
  },
  {
    name: 'commit_message_fails_when_the_session_cannot_open',
    context: { agentReplies: [{ reject: 'session refused' }] },
    initial: live(),
    steps: [commit()]
  },
  {
    name: 'commit_message_logs_a_failed_dispose',
    context: { agentReplies: [{ sessionId: T }, { reject: 'dispose broke' }] },
    initial: live(),
    steps: [commit(), said('{"summary":"x"}'), done()]
  },
  {
    name: 'commit_message_fails_on_a_permission_request',
    context: { agentReplies: OPENED },
    initial: live(),
    steps: [
      commit(),
      ev('_onPermissionRequest', {
        agentId: AGENT,
        sessionId: T,
        requestId: 'r1',
        options: [],
        toolCall: { toolCallId: 'c1' }
      })
    ]
  },
  {
    name: 'commit_message_is_unavailable_when_its_config_goes_during_the_spawn',
    context: {
      agentReplies: [
        { hold: true, release: { agentId: 'agent-9', capabilities: {}, authMethods: [] } },
        null
      ]
    },
    initial: live({ configToLiveAgent: {} }),
    steps: [commit(), ev('agentConfigsChanged', []), ev('releaseHeld')]
  },
  {
    name: 'one_shot_clears_the_active_mark_of_its_session',
    context: { agentReplies: OPENED },
    initial: live({ activeSessionId: T }),
    steps: [commit(), said('{"summary":"x"}'), done()]
  },
  {
    name: 'terminal_assist_refuses_a_selection_over_the_cap',
    initial: live(),
    steps: [ev('assistTerminal', 'fix', '/work', 'y'.repeat(20_001), 1)]
  },
  {
    name: 'bug_report_draft_skips_diagnostics_over_the_cap',
    initial: live(),
    steps: [bug({ diagnostics: { log: 'z'.repeat(192_000) } })]
  },
  {
    name: 'terminal_fix_answers_the_text',
    context: { agentReplies: OPENED },
    initial: live(),
    steps: [assist('fix'), said('  Use npm ci.  '), done()]
  },
  {
    name: 'terminal_explain_fails_on_an_empty_reply',
    context: { agentReplies: OPENED },
    initial: live(),
    steps: [assist('explain', null), said('   '), done()]
  },
  {
    name: 'terminal_assist_needs_a_working_directory',
    initial: live(),
    steps: [assist('fix', 1, '  ')]
  },
  {
    name: 'terminal_assist_fails_on_a_question',
    context: { agentReplies: OPENED },
    initial: live(),
    steps: [
      assist('explain'),
      ev('_onQuestionRequest', {
        agentId: AGENT,
        sessionId: T,
        questionId: 'q1',
        question: 'Which?',
        options: []
      })
    ]
  },
  {
    name: 'terminal_assist_fails_when_the_agent_errors',
    context: { agentReplies: OPENED },
    initial: live(),
    steps: [assist('fix'), ev('_onAgentError', { agentId: AGENT, sessionId: T, message: '' })]
  },
  {
    name: 'bug_report_draft_is_parsed',
    context: { agentReplies: OPENED },
    initial: live(),
    steps: [bug(), said('{"title":" Crash on open ","body":" Steps "}'), done()]
  },
  {
    name: 'bug_report_draft_answers_null_on_a_bad_reply',
    context: { agentReplies: OPENED },
    initial: live(),
    steps: [bug(), said('{"title":"ok"}'), done()]
  },
  {
    name: 'bug_report_draft_skips_without_a_working_directory',
    initial: live(),
    steps: [bug({ cwd: ' ' })]
  },
  {
    name: 'bug_report_draft_skips_a_long_description',
    initial: live(),
    steps: [bug({ userDescription: 'd'.repeat(10_001) })]
  },
  {
    name: 'bug_report_draft_is_quiet_without_an_agent',
    initial: live({ selectedAgentConfigId: null }),
    steps: [bug()]
  },
  {
    name: 'bug_report_draft_fails_when_its_session_closes',
    context: { agentReplies: OPENED },
    initial: live(),
    steps: [bug(), ev('_onSessionClosed', { agentId: AGENT, sessionId: T })]
  },
  {
    name: 'one_shot_session_goes_with_a_disconnected_agent',
    context: { agentReplies: OPENED },
    initial: live(),
    steps: [
      assist('fix'),
      said('partial'),
      ev('_onAgentDisconnected', { agentId: AGENT, reason: 'killed' })
    ]
  },
  {
    name: 'one_shot_fails_when_the_agent_crashes',
    context: { agentReplies: OPENED },
    initial: live(),
    steps: [commit(), ev('_onAgentCrashed', { agentId: AGENT, sessionId: T, message: 'segfault' })]
  }
]
