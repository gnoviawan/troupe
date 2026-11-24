// The ACP store's authenticate-on-demand scenarios for `acp.ts`:
// `withAuthRetry` and `authenticateBeforeSession` around `setMode` and
// `setModel`. Each restates a case of `auth-recovery.test.ts`.
import type { Scenario } from '../record'
import { AGENT, ev, idle, S, seeded } from './acp-fixtures'

const modes = { currentModeId: 'ask', availableModes: [{ id: 'ask' }, { id: 'code' }] }
const models = {
  currentModelId: 'm1',
  availableModels: [
    { modelId: 'm1', name: 'M1' },
    { modelId: 'm2', name: 'M2' }
  ]
}

const method = (id: string, extra: Record<string, unknown> = {}) => ({ id, name: id, ...extra })

/** A chat on `AGENT`, which runs `config` and advertises `methods`. */
function chat(methods: unknown[], config = 'claude', agent: Record<string, unknown> = {}) {
  return seeded(
    { ...idle, modes, models },
    {
      agents: { [AGENT]: { id: AGENT, authMethods: methods, ...agent } },
      agentStatus: { [AGENT]: 'connected' },
      configToLiveAgent: { [`${config}\u0000/work`]: AGENT }
    }
  )
}

/** `agent-2` also runs `claude`, in another folder, and advertises `methods`. */
function withSibling(state: Record<string, unknown>, methods: unknown[]) {
  const agents = state.agents as Record<string, unknown>
  const live = state.configToLiveAgent as Record<string, string>
  return {
    ...state,
    agents: { ...agents, 'agent-2': { id: 'agent-2', authMethods: methods } },
    configToLiveAgent: { ...live, 'claude\u0000/other': 'agent-2' }
  }
}

const REQUIRED = { reject: 'ACP_AUTH_REQUIRED: sign in first' }
const setMode = ev('setMode', S, 'code')
const setModel = ev('setModel', S, 'm2')
const FACTORY = 'acp-registry:factory-droid'
const factoryMethods = [method('factory-api-key'), method('login')]

export const authRetryScenarios: Scenario[] = [
  {
    name: 'set_mode_signs_in_with_the_only_method_and_sends_again',
    context: { setReplies: [REQUIRED], agentReplies: [null] },
    initial: chat([method('oauth', { type: 'agent' }), method('  ')]),
    steps: [setMode]
  },
  {
    name: 'set_model_failing_again_after_the_sign_in_forgets_the_method',
    context: {
      setReplies: [{ reject: '401 Unauthorized' }, { reject: 'Not logged in' }],
      agentReplies: [null]
    },
    initial: chat([method('oauth')]),
    steps: [setModel]
  },
  {
    name: 'set_mode_with_several_methods_asks_for_a_new_chat_then_uses_the_remembered_one',
    context: {
      setReplies: [REQUIRED, REQUIRED, REQUIRED],
      agentReplies: [null, null, { reject: 'unauthorized: token expired' }]
    },
    initial: chat([method('m1', { type: 'agent' }), method('m2', { type: 'agent' })]),
    steps: [setMode, ev('authenticateAgent', AGENT, 'm2'), setMode, setMode]
  },
  {
    name: 'set_mode_whose_remembered_method_is_refused_asks_for_a_new_chat',
    context: {
      setReplies: [REQUIRED],
      agentReplies: [null, { reject: 'unauthorized: token expired' }]
    },
    initial: chat([method('m1'), method('m2')]),
    steps: [ev('authenticateAgent', AGENT, 'm2'), setMode]
  },
  {
    // Another agent of the same config signed in with `old`.
    name: 'set_mode_forgets_a_remembered_method_the_agent_no_longer_lists',
    context: { setReplies: [REQUIRED], agentReplies: [null, null] },
    initial: withSibling(chat([method('oauth')]), [method('old')]),
    steps: [ev('authenticateAgent', 'agent-2', 'old'), setMode]
  },
  {
    name: 'set_mode_forgets_a_remembered_method_that_needs_the_user',
    context: { setReplies: [REQUIRED], agentReplies: [null] },
    initial: withSibling(chat([method('m1'), method('m2', { type: 'terminal' })]), [method('m2')]),
    steps: [ev('authenticateAgent', 'agent-2', 'm2'), setMode]
  },
  {
    name: 'set_mode_without_a_usable_method_keeps_the_first_error',
    context: { setReplies: [REQUIRED, { reject: 'connection reset by peer' }] },
    initial: chat([]),
    steps: [setMode, setMode]
  },
  {
    name: 'set_mode_with_only_a_terminal_method_keeps_the_first_error',
    context: { setReplies: [REQUIRED] },
    initial: chat([method('t', { type: 'terminal' })]),
    steps: [setMode]
  },
  {
    name: 'set_mode_with_only_the_gateway_keeps_the_first_error',
    context: { setReplies: [REQUIRED] },
    initial: chat([method(' gateway ')]),
    steps: [setMode]
  },
  {
    name: 'set_mode_on_a_host_signed_in_agent_keeps_the_first_error',
    context: { setReplies: [REQUIRED] },
    initial: chat([method('oauth')], 'claude', { hostAuthReady: true }),
    steps: [setMode]
  },
  {
    name: 'set_mode_on_factory_droid_signs_in_with_the_stored_key',
    context: {
      setReplies: [REQUIRED],
      agentReplies: [{ reject: 'bad key' }],
      factoryKey: true
    },
    initial: chat(factoryMethods, FACTORY),
    steps: [setMode]
  },
  {
    name: 'set_mode_on_factory_droid_without_a_key_leaves_the_sign_in_to_the_banner',
    context: { setReplies: [REQUIRED], factoryKey: { reject: 'offline' } },
    initial: chat(factoryMethods, FACTORY),
    steps: [setMode]
  },
  {
    name: 'set_mode_joins_the_sign_in_a_click_started',
    context: { setReplies: [REQUIRED], agentReplies: [{ hold: true, release: null }] },
    initial: chat([method('oauth')]),
    steps: [ev('authenticateAgent', AGENT, 'oauth'), setMode, ev('releaseHeld')]
  },
  {
    name: 'a_click_joining_a_sign_in_on_demand_fails_with_it',
    context: {
      setReplies: [REQUIRED],
      agentReplies: [{ hold: true, release: { reject: 'boom' } }]
    },
    initial: chat([method('oauth')]),
    steps: [setMode, ev('authenticateAgent', AGENT, 'oauth'), ev('releaseHeld')]
  }
]
