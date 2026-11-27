import type {
  SubagentProfile,
  SubagentProfileView,
  SubagentSettingsInput,
  SubagentSettingsView
} from '../../../../src/renderer/lib/acp-transport/types'
import {
  agentLockReason,
  agentShortName,
  applySettingsInput,
  canDeleteProfile,
  profileIdError,
  profileSkipsPrompts,
  profileUpserts,
  uniqueProfileId
} from '../../../../src/renderer/pages/app-preferences/subagent-profiles/profile-model'
import type { Scenario, StoreRecorder, VectorEvent } from '../record'

/**
 * The Sub-agents page's pure logic: profile ids, which profile may move or go, the upserts that
 * save one, and a queued change applied to the host's view. `tests/settings` replays each as a
 * read against `model::settings::subagents`.
 */
export const subagentsRecorder: StoreRecorder = {
  store: 'subagents',
  sources: ['src/renderer/pages/app-preferences/subagent-profiles/profile-model.ts'],
  reset() {},
  seed() {},
  snapshot() {
    return null
  },
  dispatch({ action, args }) {
    const all = (i: number) => args[i] as SubagentProfileView[]
    switch (action) {
      case 'idError':
        return profileIdError(args[0] as string, all(1))
      case 'uniqueId':
        return uniqueProfileId(args[0] as string, all(1))
      case 'shortName':
        return agentShortName(args[0] as string)
      case 'canDelete':
        return canDeleteProfile(args[0] as SubagentProfileView, all(1))
      case 'lockReason':
        return agentLockReason(args[0] as SubagentProfileView, all(1))
      case 'skipsPrompts':
        return profileSkipsPrompts(args[0] as SubagentProfile)
      case 'upserts':
        return profileUpserts(
          all(0),
          args[1] as SubagentProfile,
          (args[2] ?? undefined) as string | undefined
        )
      case 'apply':
        return applySettingsInput(args[0] as SubagentSettingsView, args[1] as SubagentSettingsInput)
      default:
        throw new Error(`subagents has no action ${action}`)
    }
  }
}

const read = (action: string, ...args: unknown[]): VectorEvent & { query: true } => ({
  action,
  args,
  query: true
})
const profile = (
  id: string,
  agentId: string,
  over: Partial<SubagentProfileView> = {}
): SubagentProfileView => ({
  id,
  agentId,
  enabled: true,
  model: null,
  mode: null,
  configOptions: {},
  approvals: 'ask',
  description: null,
  agentName: 'Agent',
  implicit: false,
  ...over
})
const stored = ({ agentName: _a, implicit: _i, ...rest }: SubagentProfileView): SubagentProfile =>
  rest
const claude = profile('claude', 'claude-acp', { agentName: 'Claude Agent', implicit: true })
const codex = profile('codex', 'codex-acp', {
  agentName: 'Codex',
  approvals: 'allowAll',
  model: 'gpt-5'
})
const codexFast = profile('codex-fast', 'codex-acp', {
  agentName: 'Codex',
  mode: 'agent-full-access'
})
const gone = profile('gone', 'old-acp', { agentName: null })
const all = [claude, codex, codexFast, gone]
const view: SubagentSettingsView = { enabled: true, maxRunning: 3, maxDepth: 1, profiles: all }

export const subagentsScenarios: Scenario[] = [
  {
    name: 'profile_ids_follow_the_host_rule',
    steps: [
      ...['', 'a b', 'x'.repeat(65), 'CODEX', 'codex-2', 'ok.name:v_1-2'].map((id) =>
        read('idError', id, all)
      ),
      read('uniqueId', 'codex', all),
      read('uniqueId', 'codex-fast-9', all),
      read('uniqueId', '-3', all),
      read('uniqueId', 'y'.repeat(70), all),
      ...['acp-registry:claude-acp', 'custom:My Agent!!x', 'acp-registry:', 'Gemini-CLI'].map(
        (key) => read('shortName', key)
      )
    ]
  },
  {
    name: 'which_profiles_may_move_or_go',
    steps: [
      ...all.map((p) => read('canDelete', p, all)),
      ...all.map((p) => read('lockReason', p, all)),
      read('lockReason', profile('Codex-ACP', 'codex-acp', { agentName: 'Codex' }), [
        ...all,
        profile('Codex-ACP', 'codex-acp', { agentName: 'Codex' })
      ]),
      ...all.map((p) => read('skipsPrompts', stored(p)))
    ]
  },
  {
    name: 'saving_a_profile_keeps_the_implicit_default',
    steps: [
      read('upserts', all, stored(profile('claude-2', 'claude-acp')), null),
      read('upserts', all, stored({ ...codex, id: 'codex-renamed' }), 'codex'),
      read('upserts', all, stored({ ...claude, enabled: false }), null),
      read('apply', view, { enabled: false, maxRunning: 5, deleteProfiles: ['gone'] }),
      read('apply', view, {
        upsertProfiles: [
          { profile: stored(claude) },
          { profile: stored(profile('claude-2', 'claude-acp')) },
          { previousId: 'codex', profile: stored({ ...codex, id: 'codex-x' }) },
          { profile: stored(profile('new', 'new-acp')) }
        ]
      })
    ]
  }
]
