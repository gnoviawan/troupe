import type { Surface } from '../types'
import { click, DASHBOARD, label, see, step } from './dsl'

const NO_GITHUB =
  'needs a project with a GitHub remote and a signed-in gh; the seeded repo has no remote and the host has no gh sign-in'

const github = (id: string, title: string, file: string, profile?: 'phone'): Surface => ({
  id: `github.${id}`,
  area: 'github',
  title,
  source: { overlay: [`components/github/${file}`] },
  ...(profile ? { profile } : {}),
  blocked: NO_GITHUB,
  steps: []
})

export const GITHUB: Surface[] = [
  {
    id: 'github.panel',
    area: 'github',
    title: 'GitHub side panel on a project without a GitHub remote',
    source: { screen: 'side-panel', name: 'github' },
    steps: [
      DASHBOARD,
      step(
        'b-panel',
        [click(label('GitHub'))],
        see("This project isn't a GitHub repository", 'Retry')
      )
    ]
  },
  {
    id: 'github.item',
    area: 'github',
    title: 'GitHub issue or pull request tab',
    source: { screen: 'pane', name: 'github-item' },
    blocked: NO_GITHUB,
    steps: []
  },
  github('choice-sheet', 'GitHub choice sheet', 'GithubChoiceMenu.tsx', 'phone'),
  github('choice-menu', 'GitHub choice menu', 'GithubChoiceMenu.tsx'),
  github('handoff', 'GitHub hand-off dialog', 'GithubHandoffBar.tsx'),
  github('item-sheet', 'GitHub item actions sheet', 'GithubItemMenu.tsx', 'phone'),
  github('item-menu', 'GitHub item actions menu', 'GithubItemMenu.tsx'),
  github('item-state-menu', 'GitHub item state menu', 'GithubItemMenu.tsx'),
  github('merge-confirm', 'Merge pull request confirm', 'GithubMergeControls.tsx'),
  github('picker-sheet', 'GitHub picker sheet', 'GithubPicker.tsx', 'phone'),
  github('picker', 'GitHub picker popover', 'GithubPicker.tsx')
]
