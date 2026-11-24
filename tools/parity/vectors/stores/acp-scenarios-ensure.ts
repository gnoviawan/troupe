// The ACP store's `ensureLiveAgent` scenarios for `acp.ts`, through its first
// caller, `prewarmAgent`. Each restates a case of `warm-pool.test.ts`,
// `spawn-prepare.test.ts`, `multi-project.test.ts`,
// `resolve-spawn-project.test.ts` or `host-agent-reuse.test.ts`.
import type { Scenario } from '../record'
import { ev, seeded } from './acp-fixtures'

const CLAUDE = {
  id: 'claude',
  name: 'Claude',
  command: 'claude-acp',
  args: ['--acp'],
  env: { TOKEN: '$API_TOKEN', MODE: 'fast', GONE: '$MISSING', EMPTY: '' },
  allowTerminal: true
}

const OPENCODE = { id: 'acp-registry:opencode', name: 'OpenCode', command: 'opencode' }

const WORK = {
  id: 'p1',
  name: 'Work',
  path: '/work/',
  envVars: [
    { key: 'API_TOKEN', value: 'abc' },
    { key: 'MODE', value: 'slow' },
    // biome-ignore lint/suspicious/noTemplateCurlyInString: a shell reference the spawn env drops
    { key: 'REFS', value: 'a${HOME}b$USER-c%PATH%d$X{e%NOT TAG%' },
    { key: ' ', value: 'skipped' }
  ],
  worktrees: [{ path: '/wt/feature' }]
}

const ROOT = { id: 'p0', name: 'Root', path: '/' }

const NUL = '\u0000'
const workKey = (project = 'p1', config = 'claude') =>
  `${config}${NUL}/work/sub${NUL}project${NUL}${project}`

const prewarm = (cwd = '/work/sub', config = 'claude') => ev('prewarmAgent', config, cwd)
const spawned = (agentId: string) => ({ agentId, capabilities: {}, authMethods: [] })
const held = (agentId: string) => ({ hold: true, release: spawned(agentId) })

export const ensureScenarios: Scenario[] = [
  {
    name: 'prewarm_spawns_once_and_later_callers_join_or_reuse_it',
    context: { projects: [ROOT, WORK], agentReplies: [held('agent-9')] },
    initial: { agentConfigs: [CLAUDE] },
    steps: [prewarm(), prewarm(), ev('releaseHeld'), prewarm(' /work/sub ')]
  },
  {
    name: 'prewarm_without_a_cwd_or_a_known_config_does_nothing',
    context: { projects: [WORK] },
    initial: { agentConfigs: [CLAUDE] },
    steps: [prewarm('   '), prewarm('/work', 'missing')]
  },
  {
    name: 'prewarm_failure_is_silent_and_a_definite_rejection_is_not_retried',
    context: {
      projects: [WORK],
      agentReplies: [{ reject: 'ACP_AGENT_UNAVAILABLE: not installed' }, { reject: 'boom' }]
    },
    initial: { agentConfigs: [CLAUDE] },
    steps: [
      prewarm(),
      prewarm(),
      ev('agentConfigsChanged', [{ ...CLAUDE, args: ['--acp', '--v2'] }]),
      prewarm()
    ]
  },
  {
    // `/ws` moves the verdict into the code and drops the message's tag.
    name: 'prewarm_remembers_a_ws_definite_rejection',
    context: {
      projects: [WORK],
      agentReplies: [{ reject: 'not installed', code: 'agent_unavailable' }]
    },
    initial: { agentConfigs: [CLAUDE] },
    steps: [prewarm(), prewarm()]
  },
  {
    name: 'prewarm_detaches_an_agent_a_chat_holds',
    context: { projects: [WORK], agentReplies: [spawned('agent-9')] },
    initial: seeded(
      { activeTurn: false, openTurnId: null },
      {
        agentConfigs: [CLAUDE],
        configToLiveAgent: { [workKey()]: 'agent-1' },
        agentStatus: { 'agent-1': 'connected' }
      }
    ),
    steps: [prewarm()]
  },
  {
    name: 'prewarm_spawns_again_over_an_agent_that_is_not_connected',
    context: { projects: [WORK], agentReplies: [spawned('agent-9')] },
    initial: {
      agentConfigs: [CLAUDE],
      configToLiveAgent: { [workKey()]: 'agent-1' },
      agentStatus: { 'agent-1': 'error' }
    },
    steps: [prewarm()]
  },
  {
    name: 'prewarm_detaches_an_agent_spawned_before_new_credentials',
    context: { projects: [WORK], agentReplies: [spawned('agent-9'), spawned('agent-10')] },
    initial: { agentConfigs: [CLAUDE] },
    steps: [
      prewarm(),
      ev('renewAgentCredentials', 'claude', 'p1'),
      prewarm(),
      ev('renewAgentCredentials', 'claude', 'p2'),
      prewarm()
    ]
  },
  {
    name: 'prewarm_does_not_join_a_spawn_started_with_an_older_launch',
    context: { projects: [WORK], agentReplies: [held('agent-9'), held('agent-10')] },
    initial: { agentConfigs: [CLAUDE] },
    steps: [
      prewarm(),
      ev('setProjects', [{ ...WORK, envVars: [{ key: 'API_TOKEN', value: 'xyz' }] }]),
      prewarm(),
      ev('releaseHeld'),
      ev('releaseHeld')
    ]
  },
  {
    name: 'prewarm_kills_the_agent_when_its_config_went_meanwhile',
    context: { projects: [WORK], agentReplies: [held('agent-9'), { reject: 'already gone' }] },
    initial: { agentConfigs: [CLAUDE] },
    steps: [prewarm(), ev('agentConfigsChanged', []), ev('releaseHeld')]
  },
  {
    name: 'prewarm_resolves_a_legacy_id_a_worktree_and_another_projects_agent',
    context: { projects: [WORK], agentReplies: [spawned('agent-9'), spawned('agent-10')] },
    initial: {
      agentConfigs: [OPENCODE, CLAUDE],
      configToLiveAgent: { [workKey('p2')]: 'agent-2' },
      agentStatus: { 'agent-2': 'connected' }
    },
    steps: [prewarm('/wt/feature/', 'opencode-v2'), prewarm()]
  }
]
