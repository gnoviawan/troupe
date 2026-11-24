// The user's own control picks for `acp.ts`: `setMode`, `setModel` and
// `setConfigOption`. Each restates a case of `session-controls.test.ts` or
// `composer.test.ts`.
import type { Scenario } from '../record'
import { AGENT, ev, idle, S, seeded } from './acp-fixtures'

const REUSE_KEY = 'cfg-1\u0000/work\u0000p1'

const select = (id: string, category: string, current: unknown, ...values: string[]) => ({
  id,
  name: id,
  category,
  type: 'select',
  currentValue: current,
  options: values.map((value) => ({ value, name: value.toUpperCase() }))
})

const modelOption = (current = 'm1') => select('model', 'model', current, 'm1', 'm2')
const effort = (current = 'low') => select('effort', 'thought_level', current, 'low', 'high')
const speed = (current = 'slow') => select('speed', 'other', current, 'slow', 'fast')
const permission = (current = 'ask') => select('perm', 'mode', current, 'ask', 'auto')

const modes = { currentModeId: 'ask', availableModes: [{ id: 'ask' }, { id: 'code' }] }
const models = {
  currentModelId: 'm1',
  availableModels: [
    { modelId: 'm1', name: 'M1' },
    { modelId: 'm2', name: 'M2' }
  ]
}

function picks(
  session: Record<string, unknown> = {},
  extra: Record<string, unknown> = {}
): Record<string, unknown> {
  return seeded(
    { ...idle, modes, models, configOptions: [modelOption(), effort(), speed()], ...session },
    {
      configToLiveAgent: { [REUSE_KEY]: AGENT },
      sessionIndex: [{ id: S, title: 'Chat', modelId: 'm1', modelName: 'M1' }],
      ...extra
    }
  )
}

export const pickScenarios: Scenario[] = [
  {
    name: 'set_mode_moves_the_mode_and_saves_the_pick',
    initial: picks(),
    steps: [ev('setMode', S, 'code')]
  },
  {
    name: 'set_mode_without_modes_only_saves_the_pick',
    initial: picks({ modes: null }),
    steps: [ev('setMode', S, 'code')]
  },
  {
    name: 'a_pick_on_an_unknown_session_is_rejected',
    steps: [
      ev('setMode', S, 'code'),
      ev('setModel', S, 'm2'),
      ev('setConfigOption', S, 'effort', 'high')
    ]
  },
  {
    name: 'set_model_moves_both_projections_and_the_index',
    initial: picks(),
    steps: [ev('setModel', S, 'm2'), ev('setModel', S, 'm1', { persist: false })]
  },
  {
    name: 'set_model_with_native_models_only',
    initial: picks({ configOptions: [effort()] }),
    steps: [ev('setModel', S, 'm2')]
  },
  {
    name: 'set_model_without_a_live_config_saves_nothing',
    initial: picks({}, { configToLiveAgent: {} }),
    steps: [ev('setModel', S, 'm2')]
  },
  {
    name: 'set_config_option_without_a_snapshot_sets_the_value',
    initial: picks(),
    steps: [
      ev('setConfigOption', S, 'effort', 'high'),
      ev('setConfigOption', S, 'speed', 'fast', { persist: false })
    ]
  },
  {
    name: 'set_config_option_keeps_a_moved_value_over_a_creation_echo',
    context: {
      configReplies: [
        [modelOption(), effort('high'), speed('fast')],
        [modelOption(), effort('high'), speed('fast')]
      ]
    },
    initial: picks({ creationOptionDefaults: { configValues: { speed: 'fast', effort: 'low' } } }),
    steps: [ev('setConfigOption', S, 'effort', 'high'), ev('setConfigOption', S, 'effort', 'high')]
  },
  {
    name: 'set_config_option_on_the_model_saves_the_model',
    context: { configReplies: [[modelOption('m2'), effort('high'), speed()]] },
    initial: picks(),
    steps: [ev('setConfigOption', S, 'model', 'm2')]
  },
  {
    name: 'set_config_option_on_a_mode_option_is_agent_wide',
    initial: picks({ configOptions: [modelOption(), permission()] }),
    steps: [ev('setConfigOption', S, 'perm', 'auto')]
  },
  {
    name: 'set_config_option_saves_a_boolean_as_text',
    initial: picks({
      configOptions: [
        modelOption(),
        { id: 'fast', name: 'Fast', category: 'other', type: 'boolean', currentValue: false }
      ]
    }),
    steps: [ev('setConfigOption', S, 'fast', true)]
  },
  {
    name: 'set_config_option_without_a_live_config_only_sets_the_value',
    initial: picks({}, { configToLiveAgent: {} }),
    steps: [ev('setConfigOption', S, 'effort', 'high')]
  }
]
