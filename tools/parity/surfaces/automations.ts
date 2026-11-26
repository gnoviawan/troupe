import type { Surface } from '../types'
import { AUTOMATION_NAME, seedAutomation } from '../world'
import { click, css, DASHBOARD, label, press, see, step, txt, type } from './dsl'

const LIST = step(
  'b-list',
  [click(label('Open automations'))],
  see(AUTOMATION_NAME, 'Needs approval')
)
const DETAIL = step('c-detail', [click(txt(AUTOMATION_NAME))], see('Approve and arm', 'Secrets'))
const EDITOR = step('d-editor', [click(txt('Edit'))], see('No unsaved changes', 'Discard'))
const INSPECTOR = step(
  'e-inspector',
  [click(css('[data-testid="rf__node-fetch"]'))],
  see('Allowed hosts', 'No issues on Fetch the feed.')
)

const automation = (
  id: string,
  title: string,
  source: Surface['source'],
  steps: Surface['steps']
): Surface => ({
  id: `automations.${id}`,
  area: 'automations',
  title,
  source,
  prepare: seedAutomation,
  steps: [DASHBOARD, LIST, ...steps]
})

export const AUTOMATIONS: Surface[] = [
  automation('list', 'Automations list', { screen: 'pane', name: 'automations' }, []),
  automation('detail', 'Automation detail', { journey: 'starter' }, [DETAIL]),
  automation(
    'delete',
    'Delete automation confirm',
    { overlay: ['components/automations/AutomationDetail.tsx'] },
    [DETAIL, step('d-confirm', [click(txt('Delete'))], see('Delete automation'))]
  ),
  automation(
    'secrets',
    'Automation secrets dialog',
    { overlay: ['components/automations/AutomationSecretsDialog.tsx'] },
    [DETAIL, step('d-dialog', [click(txt('Secrets'))], see('This automation uses no secrets.'))]
  ),
  automation('editor', 'Automation canvas editor with the inspector', { journey: 'starter' }, [
    DETAIL,
    EDITOR,
    INSPECTOR
  ]),
  automation(
    'discard',
    'Discard unsaved automation changes confirm',
    { overlay: ['components/automations/canvas/AutomationEditor.tsx'] },
    [
      DETAIL,
      EDITOR,
      INSPECTOR,
      step(
        'f-edit',
        [click(css('input[value="Fetch the feed"]')), type(' now'), press('enter')],
        see('1 unsaved change')
      ),
      step('g-confirm', [click(label('Close the editor'))], see('Discard unsaved changes'))
    ]
  ),
  automation(
    'template-suggestions',
    'Template field suggestions',
    { overlay: ['components/automations/canvas/inspector/fields.tsx'] },
    [
      DETAIL,
      EDITOR,
      INSPECTOR,
      step(
        'f-suggest',
        [click(css('input[value="https://example.invalid/feed.json"]')), type('{{')],
        see('trigger')
      )
    ]
  ),
  {
    id: 'automations.picker',
    area: 'automations',
    title: 'Inspector picker field',
    source: { overlay: ['components/automations/canvas/inspector/fields.tsx'] },
    blocked:
      'the picker is the tool field of an MCP tool step, enabled only when its MCP server lists tools; the seeded world has no MCP server',
    steps: []
  },
  automation(
    'add-node',
    'Add a step menu',
    { overlay: ['components/automations/canvas/palette/AddNodeMenu.tsx'] },
    [DETAIL, EDITOR, step('e-menu', [click(label('Add a step'))], see('Actions', 'Logic', 'Data'))]
  ),
  {
    id: 'automations.add-node-sheet',
    area: 'automations',
    title: 'Add a step sheet (phone)',
    source: { overlay: ['components/automations/canvas/palette/AddNodeMenu.tsx'] },
    profile: 'phone',
    prepare: seedAutomation,
    steps: [
      DASHBOARD,
      step(
        'b-list',
        [click(label('Open menu')), click(label('Automations'))],
        see(AUTOMATION_NAME, 'Needs approval')
      ),
      step('c-detail', [click(txt(AUTOMATION_NAME))], see('Approve and arm', 'Secrets')),
      step('d-editor', [click(txt('Edit'))], see('No unsaved changes')),
      step('e-sheet', [click(label('Add a step'))], see('Add a step', 'Actions', 'Logic'))
    ]
  }
]
