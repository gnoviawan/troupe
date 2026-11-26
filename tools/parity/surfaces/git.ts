import type { Surface } from '../types'
import { resetAlpha } from '../world'
import { click, css, DASHBOARD, label, see, step, txt } from './dsl'

const openChanges = click(label('Changes'))
const CHANGES = step('b-changes', [openChanges], see('README.md', 'notes.txt'))
const branchMenu = click(css('[data-testid="side-panel-header"] button:has-text("main")'))
const HISTORY = step(
  'c-history',
  [click(css('button:has-text("View all")'))],
  see('Any author', 'initial commit')
)

export const GIT: Surface[] = [
  {
    id: 'git.changes',
    area: 'git',
    title: 'Changes panel: staged, unstaged, untracked, history',
    source: { screen: 'side-panel', name: 'changes' },
    prepare: resetAlpha,
    steps: [
      DASHBOARD,
      step('b-changes', [openChanges], see('app.ts', 'README.md', 'notes.txt', 'link the guide'))
    ]
  },
  {
    id: 'git.diff',
    area: 'git',
    title: 'Diff view of an unstaged file',
    source: { screen: 'pane', name: 'diff' },
    prepare: resetAlpha,
    steps: [
      DASHBOARD,
      step('b-changes', [openChanges], see('README.md')),
      step('c-diff', [click(txt('README.md'))], see('An unstaged line.'))
    ]
  },
  {
    id: 'git.branch-menu',
    area: 'git',
    title: 'Branch menu',
    source: { overlay: ['components/git/BranchSection.tsx'] },
    prepare: resetAlpha,
    steps: [DASHBOARD, CHANGES, step('c-menu', [branchMenu], see('Create new branch...'))]
  },
  {
    id: 'git.create-branch',
    area: 'git',
    title: 'Create branch dialog',
    source: { overlay: ['components/git/BranchSection.tsx'] },
    prepare: resetAlpha,
    steps: [
      DASHBOARD,
      CHANGES,
      step('c-menu', [branchMenu], see('Create new branch...')),
      step('d-dialog', [click(txt('Create new branch...'))], see('Create New Branch'))
    ]
  },
  {
    id: 'git.switch-with-changes',
    area: 'git',
    title: 'Uncommitted changes on branch switch',
    source: { overlay: ['components/git/BranchSection.tsx'] },
    prepare: resetAlpha,
    steps: [
      DASHBOARD,
      CHANGES,
      step('c-menu', [branchMenu], see('feature/login')),
      step(
        'd-dialog',
        [click(label('feature/login', 'menuitem'))],
        see('Uncommitted Changes', 'Bring Changes:')
      )
    ]
  },
  {
    id: 'git.amend-pushed',
    area: 'git',
    title: 'Amend a pushed commit confirm',
    source: { overlay: ['components/git/CommitComposer.tsx'] },
    blocked: 'needs a pushed last commit; the seeded repo has no remote',
    steps: []
  },
  {
    id: 'git.discard-confirm',
    area: 'git',
    title: 'Discard changes confirm',
    source: { overlay: ['components/git/GitPanelDialogs.tsx'] },
    prepare: resetAlpha,
    steps: [
      DASHBOARD,
      CHANGES,
      step(
        'c-confirm',
        [click(label('Discard changes'))],
        see('Discard changes to "README.md"? This cannot be undone.')
      )
    ]
  },
  {
    id: 'git.stash-menu',
    area: 'git',
    title: 'Stash menu',
    source: { overlay: ['components/git/StashMenu.tsx'] },
    prepare: resetAlpha,
    steps: [
      DASHBOARD,
      CHANGES,
      step('c-menu', [click(label('Stash changes'))], see('Stash changes'))
    ]
  },
  {
    id: 'git.stash-dialog',
    area: 'git',
    title: 'Stash changes dialog',
    source: { overlay: ['components/git/StashSection.tsx'] },
    prepare: resetAlpha,
    steps: [
      DASHBOARD,
      CHANGES,
      step('c-menu', [click(label('Stash changes'))]),
      step(
        'd-dialog',
        [click(label('Stash changes', 'menuitem'))],
        see('Stash Changes', 'WIP on current branch...')
      )
    ]
  },
  {
    id: 'git.history',
    area: 'git',
    title: 'Git history in the main panel',
    source: { screen: 'pane', name: 'git-history' },
    prepare: resetAlpha,
    steps: [DASHBOARD, CHANGES, HISTORY]
  },
  {
    id: 'git.history-scope',
    area: 'git',
    title: 'History scope menu',
    source: { overlay: ['components/git/history/history-toolbar.tsx'] },
    prepare: resetAlpha,
    steps: [
      DASHBOARD,
      CHANGES,
      HISTORY,
      step('d-menu', [click(label('Scope: All branches'))], see('Current branch'))
    ]
  },
  {
    id: 'git.history-author',
    area: 'git',
    title: 'History author menu',
    source: { overlay: ['components/git/history/history-toolbar.tsx'] },
    prepare: resetAlpha,
    steps: [
      DASHBOARD,
      CHANGES,
      HISTORY,
      step('d-menu', [click(label('Filter by author'))], see('Matrix'))
    ]
  },
  {
    id: 'git.commit-diff',
    area: 'git',
    title: 'Commit diff from history',
    source: { screen: 'pane', name: 'commit-diff' },
    prepare: resetAlpha,
    steps: [
      DASHBOARD,
      CHANGES,
      HISTORY,
      step('d-diff', [click(txt('Open diff'))], see('README.md'))
    ]
  },
  {
    id: 'git.pane',
    area: 'git',
    title: 'Git tab in a pane',
    source: { screen: 'pane', name: 'git' },
    blocked:
      'opens only from the changed-files panel of a chat status chip, and the scripted agent changes no files',
    steps: []
  }
]
