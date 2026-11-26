// The user's agent commands for `acp.ts`: spawn, kill, sign-in, sign-out and
// the answer to an agent's own sign-in prompt. Each restates a case of
// `auth-recovery.test.ts`, `lifecycle.test.ts`, `agent-browser-pane.test.ts`
// or `events.test.ts`.
import type { Scenario } from '../record'
import { AGENT, ev, idle, S, seeded } from './acp-fixtures'

const REUSE_KEY = 'cfg-1\u0000/work\u0000p1'

const methods = [
  { id: 'oauth', name: 'Browser', type: 'agent' },
  { id: 'term', name: 'Terminal', type: 'terminal' },
  { id: 'legacy', name: 'Legacy' }
]

const agent = (extra: Record<string, unknown> = {}) => ({
  id: AGENT,
  capabilities: { auth: { logout: true } },
  authMethods: methods,
  hostAuthReady: false,
  ...extra
})

/** A live chat on the agent, which the config maps to. */
function live(extra: Record<string, unknown> = {}): Record<string, unknown> {
  return seeded(idle, {
    agents: { [AGENT]: agent() },
    agentStatus: { [AGENT]: 'connected' },
    configToLiveAgent: { [REUSE_KEY]: AGENT },
    ...extra
  })
}

const browserPrompt = { [AGENT]: { kind: 'browser', url: 'https://login.example/a' } }
const elicitation = {
  [AGENT]: {
    kind: 'elicitation',
    requestId: 'req-1',
    url: 'https://login.example/e',
    message: 'Code 1234'
  }
}

const authenticate = (methodId: string, input?: unknown) =>
  ev('authenticateAgent', AGENT, methodId, ...(input === undefined ? [] : [input]))
const browserOpen = (url: string) => ev('_onBrowserOpenRequest', { agentId: AGENT, url })
const config = { id: 'cfg-1', name: 'Claude', command: 'claude-acp', args: [] }

export const agentCommandScenarios: Scenario[] = [
  {
    name: 'spawn_agent_registers_what_the_reply_says',
    context: {
      agentReplies: [
        {
          agentId: 'agent-2',
          capabilities: { loadSession: true },
          authMethods: [{ id: 'oauth', type: 'agent' }],
          hostAuthReady: true
        }
      ]
    },
    steps: [ev('spawnAgent', config, 'p1', 's1')]
  },
  {
    // The reply wins; the `agent_spawned` event only fills what it left out.
    name: 'spawn_agent_falls_back_to_what_the_event_seeded',
    context: { agentReplies: [{ agentId: AGENT, capabilities: { loadSession: true } }] },
    initial: { agents: { [AGENT]: agent({ hostAuthReady: true }) } },
    steps: [ev('spawnAgent', config, 'p1')]
  },
  {
    name: 'spawn_agent_failure_marks_the_config_name',
    context: { agentReplies: [{ reject: 'spawn failed' }] },
    steps: [ev('spawnAgent', config, 'p1')]
  },
  {
    name: 'kill_agent_drops_the_agent_and_closes_its_chats',
    initial: live({
      preparedSessions: { [`${REUSE_KEY}\u0000`]: S },
      pendingSignIn: browserPrompt,
      pendingPermissions: {
        'perm-1': {
          requestId: 'perm-1',
          agentId: AGENT,
          sessionId: S,
          toolCall: { toolCallId: 't1', title: 'Edit' },
          options: []
        }
      }
    }),
    steps: [ev('killAgent', AGENT)]
  },
  {
    name: 'kill_agent_failure_keeps_the_agent',
    context: { agentReplies: [{ reject: 'no such agent' }] },
    initial: live(),
    steps: [ev('killAgent', AGENT)]
  },
  {
    name: 'authenticate_remembers_an_agent_method_and_ends_the_sign_in',
    initial: live({ pendingSignIn: browserPrompt }),
    steps: [authenticate(' oauth '), authenticate('legacy'), authenticate('legacy')]
  },
  {
    // An interactive-only method, or one that carried a key, cannot run
    // again by itself, so it is not remembered.
    name: 'authenticate_does_not_remember_a_terminal_method_or_a_key',
    initial: live(),
    steps: [authenticate('term'), authenticate('oauth', { apiKey: 'secret' })]
  },
  {
    name: 'authenticate_rejects_an_empty_or_unadvertised_method',
    initial: live(),
    steps: [authenticate('  '), authenticate('password')]
  },
  {
    name: 'authenticate_failure_logs_without_the_reason',
    context: { agentReplies: [{ reject: 'bad token abc123' }] },
    initial: live({ pendingSignIn: browserPrompt }),
    steps: [authenticate('oauth')]
  },
  {
    // Only the first capture of a sign-in flight may open by itself; a later
    // capture in the same flight and any capture outside one wait for a click.
    name: 'only_the_first_browser_open_of_a_sign_in_flight_auto_opens',
    context: { agentReplies: [{ hold: true }] },
    initial: live(),
    steps: [
      browserOpen('https://login.example/b'),
      authenticate('oauth'),
      browserOpen('https://login.example/c'),
      browserOpen('https://login.example/c'),
      browserOpen('https://evil.example/d')
    ]
  },
  {
    // The redirect landed without an authenticate reply: the attempted
    // method is remembered.
    name: 'complete_browser_auth_remembers_the_attempted_method',
    context: { agentReplies: [{ hold: true }] },
    initial: live(),
    steps: [
      authenticate('oauth'),
      browserOpen('https://login.example/c'),
      ev('completeBrowserAuth', AGENT)
    ]
  },
  {
    name: 'a_disconnect_forgets_the_sign_in',
    initial: live(),
    steps: [authenticate('oauth'), ev('_onAgentDisconnected', { agentId: AGENT, reason: 'exited' })]
  },
  {
    name: 'logout_signs_the_agent_out',
    initial: live(),
    steps: [authenticate('oauth'), ev('logoutAgent', AGENT)]
  },
  {
    name: 'logout_is_refused_without_the_capability_or_on_failure',
    context: { agentReplies: [null, { reject: 'host gone' }] },
    initial: live({
      agents: { [AGENT]: agent({ capabilities: {} }), 'agent-2': agent({ id: 'agent-2' }) }
    }),
    steps: [ev('logoutAgent', AGENT), authenticate('oauth'), ev('logoutAgent', 'agent-2')]
  },
  {
    name: 'respond_agent_sign_in_accept_keeps_the_prompt',
    initial: live({ pendingSignIn: elicitation }),
    steps: [ev('respondAgentSignIn', AGENT, 'accept')]
  },
  {
    name: 'respond_agent_sign_in_cancel_ends_the_prompt',
    initial: live({ pendingSignIn: elicitation }),
    steps: [ev('respondAgentSignIn', AGENT, 'cancel')]
  },
  {
    name: 'respond_agent_sign_in_ignores_a_browser_prompt',
    initial: live({ pendingSignIn: browserPrompt }),
    steps: [ev('respondAgentSignIn', AGENT, 'accept')]
  },
  {
    name: 'respond_agent_sign_in_failure_is_logged',
    context: { agentReplies: [{ reject: 'request gone' }] },
    initial: live({ pendingSignIn: elicitation }),
    steps: [ev('respondAgentSignIn', AGENT, 'accept')]
  }
]
