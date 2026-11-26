// Option restores that span several steps for `acp.ts`: the composer's model
// pick (`pickModel`) and the heal from the saved picks
// (`reapplyPersistedComposerOptions`, started by an options event). Each
// restates a case of `composer.test.ts`, `core.test.ts` or
// `session-controls.test.ts`.
import type { Scenario } from '../record'
import { AGENT, ev, S } from './acp-fixtures'
import { effort, modelOption, permission, picks, select, speed } from './acp-scenarios-picks'

const fast = (current = false) => ({
  id: 'fast',
  name: 'Fast',
  category: 'other',
  type: 'boolean',
  currentValue: current
})

const grouped = (current = 'low') => ({
  id: 'effort',
  name: 'effort',
  category: 'thought_level',
  type: 'select',
  currentValue: current,
  options: [
    {
      group: 'standard',
      name: 'Standard',
      options: [
        { value: 'low', name: 'Low' },
        { value: 'high', name: 'High' }
      ]
    }
  ]
})

/** The saved record: `m2` was tuned to high effort, the mode is agent-wide. */
const tuned = {
  modelId: 'm1',
  configValues: { model: 'm1', effort: 'low', perm: 'auto' },
  modelValues: { m2: { effort: 'high' } }
}

const optionsUpdate = (configOptions: unknown[]) =>
  ev('_onConfigOptionsUpdate', { agentId: AGENT, sessionId: S, configOptions })

const base = [modelOption(), effort(), permission()]

export const restoreScenarios: Scenario[] = [
  {
    name: 'pick_model_brings_back_the_effort_saved_for_that_model',
    context: { composerOptions: tuned },
    initial: picks({ models: null, configOptions: base }),
    steps: [ev('pickModel', S, 'm2')]
  },
  {
    name: 'pick_model_never_tuned_keeps_the_agent_default',
    context: { composerOptions: { ...tuned, modelValues: {} } },
    initial: picks({ models: null, configOptions: base }),
    steps: [ev('pickModel', S, 'm2')]
  },
  {
    name: 'pick_model_without_a_live_config_reads_nothing',
    context: { composerOptions: tuned },
    initial: picks({ models: null, configOptions: base }, { configToLiveAgent: {} }),
    steps: [ev('pickModel', S, 'm2')]
  },
  {
    // A favorite applies after its model; a value the model does not offer
    // is skipped with a warn, once.
    name: 'pick_model_favorite_snapshot_skips_an_unoffered_value',
    initial: picks({ models: null, configOptions: [modelOption(), effort(), speed()] }),
    steps: [
      ev('pickModel', S, 'm2', { effort: 'high', speed: 'turbo', model: 'm1' }),
      ev('pickModel', S, 'm1', { speed: 'turbo' })
    ]
  },
  {
    name: 'pick_model_of_the_current_model_applies_only_the_snapshot',
    initial: picks({ models: null, configOptions: [modelOption(), effort(), speed()] }),
    steps: [ev('pickModel', S, 'm1', { effort: 'high', speed: 'slow', gone: 'x' })]
  },
  {
    name: 'pick_model_restores_a_boolean_and_a_grouped_value',
    initial: picks({ models: null, configOptions: [modelOption(), grouped(), fast()] }),
    steps: [ev('pickModel', S, 'm2', { effort: 'high', fast: 'true' })]
  },
  {
    // The native route: no model option. The model's acknowledgement heals
    // nothing while the pick runs.
    name: 'pick_model_on_native_models_skips_the_heal',
    context: { composerOptions: { modelId: 'm1', configValues: { effort: 'high' } } },
    initial: picks({ configOptions: [effort()] }),
    steps: [ev('pickModel', S, 'm2')]
  },
  {
    // A model option with no values cannot take the pick; the native state does.
    name: 'pick_model_passes_over_a_model_option_without_values',
    initial: picks({ configOptions: [select('model', 'model', 'm1'), effort()] }),
    steps: [ev('pickModel', S, 'm2')]
  },
  {
    name: 'pick_model_on_an_unknown_session_is_rejected',
    steps: [ev('pickModel', S, 'm2')]
  },
  {
    name: 'pick_model_whose_model_set_fails_is_rejected',
    context: { configReplies: [{ reject: 'model unavailable' }] },
    initial: picks({ models: null, configOptions: base }),
    steps: [ev('pickModel', S, 'm2', { effort: 'high' })]
  },
  {
    name: 'pick_model_goes_on_after_a_value_the_agent_rejects',
    context: { configReplies: [null, { reject: 'effort locked' }, null] },
    initial: picks({ models: null, configOptions: [modelOption(), effort(), speed()] }),
    steps: [ev('pickModel', S, 'm2', { effort: 'high', speed: 'fast' })]
  },
  {
    name: 'pick_model_on_an_inactive_chat_sets_the_model_only',
    initial: picks({ status: 'error', models: null, configOptions: base }),
    steps: [ev('pickModel', S, 'm2', { effort: 'high' })]
  },
  {
    // The heal: the saved model through the model option, then its effort.
    name: 'heal_brings_back_the_saved_model_and_its_effort',
    context: { composerOptions: { ...tuned, modelId: 'm2' } },
    initial: picks({ models: null, configOptions: base }),
    steps: [optionsUpdate(base)]
  },
  {
    name: 'heal_is_a_no_op_when_the_session_holds_the_picks',
    context: { composerOptions: { modelId: 'm1', configValues: { effort: 'low', perm: 'ask' } } },
    initial: picks({ models: null, configOptions: base }),
    steps: [optionsUpdate(base)]
  },
  {
    name: 'heal_keeps_the_model_when_the_saved_one_is_not_offered',
    context: {
      composerOptions: { modelId: 'm9', configValues: { perm: 'auto' }, modelValues: {} }
    },
    initial: picks({ models: null, configOptions: base }),
    steps: [optionsUpdate(base), optionsUpdate(base)]
  },
  {
    // A native model switch acknowledged while the heal runs marks one more
    // pass, which finds the picks in place.
    name: 'heal_on_native_models_runs_once_more_after_the_switch',
    context: { composerOptions: { modelId: 'm2', configValues: { effort: 'high' } } },
    initial: picks({ configOptions: [effort()] }),
    steps: [optionsUpdate([effort()])]
  },
  {
    // The native model state wins over the model option for the heal.
    name: 'heal_prefers_the_native_model_state',
    context: { composerOptions: { modelId: 'm2', configValues: { perm: 'auto' } } },
    initial: picks({ configOptions: base }),
    steps: [optionsUpdate(base)]
  },
  {
    name: 'heal_guesses_agent_wide_values_before_any_option_is_known',
    context: {
      composerOptions: {
        modelId: 'm1',
        configValues: { model: 'm1', effort: 'high', perm: 'auto' },
        modelValues: { m1: { effort: 'high' } }
      }
    },
    initial: picks({ configOptions: [] }),
    steps: [ev('setMode', S, 'code')]
  },
  {
    name: 'heal_stops_when_the_saved_model_is_rejected',
    context: {
      composerOptions: { ...tuned, modelId: 'm2' },
      configReplies: [{ reject: 'model unavailable' }]
    },
    initial: picks({ models: null, configOptions: base }),
    steps: [optionsUpdate(base)]
  },
  {
    name: 'heal_recognizes_a_grouped_value',
    context: { composerOptions: { modelId: 'm1', configValues: { effort: 'high' } } },
    initial: picks({ models: null, configOptions: [modelOption(), grouped()] }),
    steps: [optionsUpdate([modelOption(), grouped()])]
  },
  {
    name: 'heal_skips_a_chat_that_is_not_active',
    context: { composerOptions: { ...tuned, modelId: 'm2' } },
    initial: picks({ status: 'initializing', models: null, configOptions: base }),
    steps: [optionsUpdate(base)]
  },
  {
    name: 'heal_keeps_a_value_no_longer_offered',
    context: {
      composerOptions: { modelId: 'm1', configValues: { effort: 'max', speed: 'fast' } }
    },
    initial: picks({
      models: null,
      configOptions: [modelOption(), effort(), select('speed', 'other', 'slow', 'slow')]
    }),
    steps: [optionsUpdate([modelOption(), effort(), select('speed', 'other', 'slow', 'slow')])]
  }
]
