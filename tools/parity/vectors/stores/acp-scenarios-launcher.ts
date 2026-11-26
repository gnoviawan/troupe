// The ACP store's `applyPendingLauncherOptions` scenarios for `acp.ts`
// (`slices/launch.ts`): the launcher's or an armed switch's picks set on a
// new session one command at a time. Each restates a case of
// `composer.test.ts` (`applyPendingLauncherOptions`) or `agent-switch.test.ts`.
import type { Scenario } from '../record'
import { AGENT, ev, idle, S, seeded } from './acp-fixtures'

const option = (id: string, current: string, values: string[], category?: string) => ({
  id,
  name: id,
  category: category ?? null,
  type: 'select',
  currentValue: current,
  options: values.map((value) => ({ value, name: value }))
})
const MODEL = option('model', 'm1', ['m1', 'm2'], 'model')
const EFFORT = option('thought_level', 'low', ['low', 'max'])
const PERMISSION = option('permission', 'ask', ['ask', 'auto'], 'mode')
const FAST = { id: 'fast', name: 'fast', category: null, type: 'boolean', currentValue: false }
const MODES = { currentModeId: 'chat', availableModes: [{ id: 'chat' }, { id: 'code' }] }
const NUL = '\u0000'

/** `s1`, idle, with these controls; `AGENT` runs `claude` when `mapped`. */
const live = (controls: Record<string, unknown>, mapped = false) =>
  seeded(
    { ...idle, modes: MODES, ...controls },
    mapped ? { configToLiveAgent: { [`claude${NUL}/work${NUL}project${NUL}p1`]: AGENT } } : {}
  )
const apply = (pending: unknown) => ev('applyPendingLauncherOptions', S, pending)

export const launcherOptionScenarios: Scenario[] = [
  {
    name: 'apply_launcher_options_sets_mode_model_and_values_in_order',
    initial: live({ configOptions: [MODEL, EFFORT, PERMISSION] }, true),
    steps: [
      apply({
        modeId: 'code',
        modelId: 'm2',
        configValues: { model: 'm2', thought_level: 'max', permission: 'ask', gone: 'x' }
      })
    ]
  },
  {
    name: 'apply_launcher_options_without_picks_does_nothing',
    initial: live({ configOptions: [MODEL] }),
    steps: [apply(null), apply({ modeId: 'chat', modelId: 'm1', configValues: {} })]
  },
  {
    name: 'apply_launcher_options_on_a_closed_chat_does_nothing',
    initial: live({ status: 'closed', configOptions: [MODEL] }),
    steps: [apply({ modeId: 'code', modelId: 'm2', configValues: { thought_level: 'max' } })]
  },
  {
    name: 'apply_launcher_options_on_a_missing_chat_does_nothing',
    initial: live({ configOptions: [MODEL] }),
    steps: [ev('applyPendingLauncherOptions', 's9', { modeId: 'code', configValues: {} })]
  },
  {
    name: 'apply_launcher_options_sets_a_native_model',
    initial: live({
      configOptions: [],
      models: {
        currentModelId: 'gpt-4',
        availableModels: [
          { modelId: 'gpt-4', name: 'GPT-4' },
          { modelId: 'gpt-5', name: 'GPT-5' }
        ]
      }
    }),
    steps: [apply({ modelId: 'gpt-5', configValues: {} })]
  },
  {
    name: 'apply_launcher_options_falls_back_to_the_model_option',
    context: { setReplies: [{ reject: 'set_model is not supported' }] },
    initial: live({
      configOptions: [MODEL],
      models: { currentModelId: 'm1', availableModels: [{ modelId: 'm1', name: 'M1' }] }
    }),
    steps: [apply({ modelId: 'm2', configValues: {} })]
  },
  {
    name: 'apply_launcher_options_with_a_missing_model_skips_its_scoped_values',
    initial: live({ configOptions: [EFFORT, PERMISSION] }),
    steps: [apply({ modelId: 'm9', configValues: { thought_level: 'max', permission: 'auto' } })]
  },
  {
    name: 'apply_launcher_options_logs_a_rejected_model_option',
    context: { configReplies: [{ reject: 'model m2 is gated' }] },
    initial: live({ configOptions: [MODEL, EFFORT, PERMISSION] }),
    steps: [apply({ modelId: 'm2', configValues: { thought_level: 'max', permission: 'auto' } })]
  },
  {
    name: 'apply_launcher_options_logs_a_rejected_mode_and_goes_on',
    context: { setReplies: [{ reject: 'mode code is gated' }] },
    initial: live({ configOptions: [MODEL] }),
    steps: [apply({ modeId: 'code', modelId: 'm2', configValues: {} })]
  },
  {
    name: 'apply_launcher_options_logs_a_rejected_value_and_goes_on',
    context: { configReplies: [{ reject: 'effort max is gated' }] },
    initial: live({ configOptions: [EFFORT, PERMISSION, FAST] }),
    steps: [
      apply({
        configValues: { thought_level: 'max', permission: 'auto', fast: 'true', missing: 'x' }
      })
    ]
  },
  {
    name: 'apply_launcher_options_skips_the_model_option_the_model_pick_set',
    context: { configReplies: [[MODEL, EFFORT]] },
    initial: live({ configOptions: [MODEL, EFFORT] }),
    steps: [apply({ modelId: 'm2', configValues: { model: 'm2', thought_level: 'max' } })]
  },
  {
    name: 'apply_launcher_options_skips_values_already_current_and_unknown_values',
    initial: live({ configOptions: [EFFORT, PERMISSION, FAST] }),
    steps: [apply({ configValues: { thought_level: 'low', permission: 'never', fast: 'maybe' } })]
  }
]
