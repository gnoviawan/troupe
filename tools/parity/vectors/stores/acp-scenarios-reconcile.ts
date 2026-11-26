// The ACP store's prompt reconcile scenarios for `acp.ts` (`prompt-reconcile.ts`
// over `lib/command-engine.ts`): a prompt whose reply was lost waits to be
// confirmed. Each restates a case of `prompt-reconcile.test.ts`. A
// `context.dispatch` entry `{ uncertain: code }` loses the reply with that
// transport code. The engine's resend ladder runs on timers the recorder does
// not reach; `reconcile_tests.rs` covers it.
import type { Scenario } from '../record'
import { AGENT, ev, idle, S, seeded, text } from './acp-fixtures'

const lost = { uncertain: 'closed' }
const unknown = { uncertain: 'outcome_unknown' }
const send = ev('sendPrompt', S, 'hello')
const recheck = ev('recheckPromptCommand', S)
const echo = (turnId = 'id-1') =>
  ev('_onUserPrompt', { sessionId: S, turnId, content: [text('hello')] }, 3)
const complete = (stopReason: string, turnId = 'id-1') =>
  ev('_onPromptComplete', { agentId: AGENT, sessionId: S, stopReason, turnId })

export const reconcileScenarios: Scenario[] = [
  {
    name: 'a_prompt_whose_reply_is_lost_waits_to_confirm',
    context: { dispatch: [lost] },
    initial: seeded(idle),
    steps: [send]
  },
  {
    name: 'a_prompt_whose_outcome_is_unknown_holds',
    context: { dispatch: [unknown] },
    initial: seeded(idle),
    steps: [send]
  },
  {
    name: 'the_echo_confirms_a_waiting_prompt',
    context: { dispatch: [lost] },
    initial: seeded(idle),
    steps: [send, echo()]
  },
  {
    name: 'an_echo_of_another_turn_leaves_the_prompt_waiting',
    context: { dispatch: [lost] },
    initial: seeded(idle),
    steps: [send, echo('t9')]
  },
  {
    name: 'prompt_complete_ends_a_waiting_prompt',
    context: { dispatch: [lost] },
    initial: seeded(idle),
    steps: [send, complete('end_turn')]
  },
  {
    name: 'recheck_resends_the_same_turn_and_settles',
    context: { dispatch: [unknown, { receipt: 'accepted' }] },
    initial: seeded(idle),
    steps: [send, recheck]
  },
  {
    name: 'recheck_answered_by_a_stop_reason_settles',
    context: { dispatch: [lost, 'end_turn'] },
    initial: seeded(idle),
    steps: [send, recheck]
  },
  {
    name: 'recheck_that_loses_its_reply_again_keeps_waiting',
    context: { dispatch: [unknown, lost] },
    initial: seeded(idle),
    steps: [send, recheck]
  },
  {
    name: 'recheck_with_an_unknown_outcome_keeps_the_hold',
    context: { dispatch: [lost, unknown] },
    initial: seeded(idle),
    steps: [send, recheck]
  },
  {
    name: 'recheck_rejected_fails_the_turn',
    context: { dispatch: [unknown, { error: 'boom' }] },
    initial: seeded(idle),
    steps: [send, recheck]
  },
  {
    name: 'recheck_on_a_busy_host_fails_the_turn',
    context: { dispatch: [unknown, { busy: true }] },
    initial: seeded(idle),
    steps: [send, recheck]
  },
  {
    name: 'recheck_with_nothing_waiting_does_nothing',
    initial: seeded(idle),
    steps: [recheck]
  },
  {
    name: 'stop_abandons_a_waiting_prompt',
    context: { dispatch: [lost] },
    initial: seeded(idle),
    steps: [send, ev('cancelPrompt', S)]
  },
  {
    name: 'a_late_terminator_after_stop_is_stale',
    context: { dispatch: [lost] },
    initial: seeded(idle),
    steps: [send, ev('cancelPrompt', S), complete('end_turn')]
  },
  {
    name: 'recheck_after_stop_sends_nothing',
    context: { dispatch: [lost] },
    initial: seeded(idle),
    steps: [send, ev('cancelPrompt', S), recheck]
  },
  {
    name: 'the_agent_going_away_drops_a_waiting_prompt',
    context: { dispatch: [lost] },
    initial: seeded(idle),
    steps: [send, ev('_onAgentDisconnected', { agentId: AGENT, reason: 'killed' })]
  },
  {
    name: 'closing_the_chat_drops_a_waiting_prompt',
    context: { dispatch: [lost] },
    initial: seeded(idle),
    steps: [send, ev('_onSessionClosed', { agentId: AGENT, sessionId: S })]
  },
  {
    name: 'a_new_prompt_after_stop_replaces_the_waiting_one',
    context: { dispatch: [lost, lost] },
    initial: seeded(idle),
    steps: [send, ev('cancelPrompt', S), ev('sendPrompt', S, 'again')]
  },
  {
    name: 'a_terminator_after_a_failed_send_is_not_taken_for_a_stale_one',
    context: { dispatch: [{ error: 'boom' }] },
    initial: seeded(idle),
    steps: [send, complete('end_turn', 'id-7')]
  }
]
