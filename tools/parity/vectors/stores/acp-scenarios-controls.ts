// The ACP store's session-control scenarios for `acp.ts`. Each restates a
// case of `session-controls.test.ts`, `options-cache-hydration.test.ts`,
// `model-catalog.test.ts` or `composer.test.ts`.
import type { Scenario } from '../record'
import { AGENT, ev, S, seeded } from './acp-fixtures'

const mode = (id: string) => ({ id, name: id })
const values = (...names: string[]) => names.map((value) => ({ value, name: value }))

const select = (
  id: string,
  currentValue: unknown,
  options: unknown[],
  category: string | null = null
) => ({ id, name: id, category, type: 'select', currentValue, options })

const modeUpdate = (currentModeId: string, availableModes?: unknown[], sessionId = S) =>
  ev('_onModeUpdate', {
    agentId: AGENT,
    sessionId,
    currentModeId,
    ...(availableModes ? { availableModes } : {})
  })

const configUpdate = (configOptions: unknown[], sessionId = S) =>
  ev('_onConfigOptionsUpdate', { agentId: AGENT, sessionId, configOptions })

const REUSE_KEY = 'cfg-1\u0000/work\u0000p1'

const modeScenarios: Scenario[] = [
  {
    name: 'mode_update_sets_and_keeps_the_mode_list',
    initial: seeded(),
    steps: [
      modeUpdate('default', [mode('default'), mode('plan')]),
      modeUpdate('plan'),
      modeUpdate('x', [], 's9')
    ]
  },
  {
    name: 'a_creation_default_echo_keeps_the_picked_mode',
    initial: seeded({
      modes: { currentModeId: 'plan', availableModes: [mode('default'), mode('plan')] },
      creationOptionDefaults: { modeId: 'default', configValues: {} }
    }),
    steps: [
      modeUpdate('default'),
      modeUpdate('default'),
      modeUpdate('acceptEdits', [mode('default'), mode('acceptEdits')]),
      modeUpdate('default', [mode('default')])
    ]
  }
]

const configScenarios: Scenario[] = [
  {
    name: 'the_first_options_snapshot_is_taken_as_is',
    initial: seeded(),
    steps: [configUpdate([select('effort', 'high', values('low', 'high'))])]
  },
  {
    name: 'the_model_option_keeps_its_value_while_it_is_offered',
    initial: seeded({
      configOptions: [select('model', 'opus', values('opus', 'sonnet'), 'model')]
    }),
    steps: [
      configUpdate([select('model', 'sonnet', values('opus', 'sonnet'), 'model')]),
      configUpdate([select('model', 'haiku', values('sonnet', 'haiku'), 'model')]),
      configUpdate([
        select('effort', 'low', values('low')),
        select('model', 'haiku', values('haiku'), 'model')
      ])
    ]
  },
  {
    name: 'a_creation_default_echo_keeps_the_picked_option',
    initial: seeded({
      configOptions: [
        select('effort', 'high', values('medium', 'high')),
        { id: 'fast', name: 'Fast', category: null, type: 'boolean', currentValue: true }
      ],
      creationOptionDefaults: { configValues: { effort: 'medium', fast: 'false' } }
    }),
    steps: [
      configUpdate([
        select('effort', 'medium', values('medium', 'high')),
        { id: 'fast', name: 'Fast', category: null, type: 'boolean', currentValue: false }
      ]),
      configUpdate([
        select('effort', 'medium', values('medium', 'high')),
        { id: 'fast', name: 'Fast', category: null, type: 'boolean', currentValue: false }
      ]),
      configUpdate([select('effort', 'low', values('low', 'medium', 'high'))])
    ]
  },
  {
    name: 'the_shown_model_moves_onto_models_and_the_index',
    initial: seeded(
      {
        models: {
          currentModelId: 'a',
          availableModels: [
            { modelId: 'a', name: 'Model A' },
            { modelId: 'b', name: 'Model B' }
          ]
        }
      },
      { sessionIndex: [{ id: S, title: 'Chat', modelId: 'a', modelName: 'Model A' }] }
    ),
    steps: [
      configUpdate([
        select(
          'model',
          'b',
          [
            { value: 'a', name: 'Model A' },
            { value: 'b', name: '  Model B  ' }
          ],
          'model'
        )
      ]),
      configUpdate([
        select(
          'model',
          'c',
          [
            { value: 'c', name: '   ' },
            { value: 'b', name: 'B' }
          ],
          'model'
        )
      ])
    ]
  },
  {
    name: 'grouped_values_flatten_for_the_model_check',
    initial: seeded({
      configOptions: [
        select(
          'model',
          'opus',
          [{ group: 'anthropic', name: 'Anthropic', options: values('opus', 'sonnet') }],
          'model'
        )
      ]
    }),
    steps: [
      configUpdate([
        select(
          'model',
          'sonnet',
          [
            { group: 'anthropic', name: 'Anthropic', options: values('opus', 'sonnet') },
            { group: 'openai', options: values('gpt') }
          ],
          'model'
        )
      ])
    ]
  }
]

const cacheScenarios: Scenario[] = [
  {
    name: 'options_land_in_the_agent_cache_and_heal_saved_picks',
    initial: seeded(
      {
        modes: { currentModeId: 'default', availableModes: [mode('default')] },
        models: {
          currentModelId: 'a',
          availableModels: [{ modelId: 'a', name: 'A', description: 'fast' }]
        }
      },
      { configToLiveAgent: { 'other\u0000/x\u0000p2': 'agent-2', [REUSE_KEY]: AGENT } }
    ),
    steps: [
      configUpdate([
        select(
          'model',
          'a',
          [
            { value: 'a', name: 'A', description: 'fast' },
            { value: 'b', name: 'B' }
          ],
          'model'
        )
      ]),
      modeUpdate('default')
    ]
  },
  {
    name: 'an_initializing_session_caches_without_healing',
    initial: seeded({ status: 'initializing' }, { configToLiveAgent: { [REUSE_KEY]: AGENT } }),
    steps: [configUpdate([select('effort', 'high', values('high'))])]
  },
  {
    name: 'an_empty_snapshot_drops_the_cache_entry',
    initial: seeded(
      { modes: null, models: null },
      {
        configToLiveAgent: { [REUSE_KEY]: AGENT },
        agentOptionsCache: {
          'cfg-1': {
            models: null,
            modes: null,
            configOptions: [select('x', 'y', values('y'))],
            updatedAt: 1
          }
        }
      }
    ),
    steps: [configUpdate([])]
  }
]

export const controlScenarios: Scenario[] = [
  ...modeScenarios,
  ...configScenarios,
  ...cacheScenarios
]
