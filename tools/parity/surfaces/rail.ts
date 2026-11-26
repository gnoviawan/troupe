import type { Surface } from '../types'
import { ensureProject } from '../world'
import {
  AGENT_PICKED,
  click,
  css,
  DASHBOARD,
  gone,
  hover,
  id,
  label,
  rclick,
  see,
  step,
  txt
} from './dsl'

const betaMenu = rclick(id('project-item-matrix-beta'))
const collapse = click(label('Expand or collapse projects'))
const addMenu = click(label('Add project or folder group'))
const switchWorktree = [
  rclick(id('project-item-matrix-alpha')),
  click(txt('Switch worktree', false))
]
const NO_GROUP =
  'needs a project group, which the seeded world does not hold (seeding groups is a follow-up)'

export const RAIL: Surface[] = [
  {
    id: 'rail.project-list',
    area: 'rail',
    title: 'Project list and switching',
    source: { journey: 'starter' },
    roi: '[data-testid="shell-rail"]',
    steps: [
      step(
        'a-dashboard',
        [],
        [...see('alpha', 'beta', 'gamma', 'delta', 'What should we do in alpha?'), AGENT_PICKED]
      ),
      step(
        'b-switch-beta',
        [click(id('project-item-matrix-beta'))],
        [...see('What should we do in beta?'), AGENT_PICKED]
      )
    ]
  },
  {
    id: 'rail.project-menu',
    area: 'rail',
    title: 'Project context menu',
    source: { overlay: ['components/sidebar/project-context-menu.tsx'] },
    roi: '[role="menu"]',
    steps: [DASHBOARD, step('b-menu', [betaMenu], see('Rename', 'Change color', 'Move to group'))]
  },
  {
    id: 'rail.archived-project-menu',
    area: 'rail',
    title: 'Archived project context menu',
    source: { overlay: ['components/sidebar/project-context-menu.tsx'] },
    blocked: 'needs an archived project, which the seeded world does not hold',
    steps: []
  },
  {
    id: 'rail.project-colour',
    area: 'rail',
    title: 'Project colour picker',
    source: { journey: 'starter' },
    prepare: (world) => ensureProject(world, 'beta'),
    steps: [
      DASHBOARD,
      step('b-picker', [betaMenu, click(txt('Change color'))], see('Select Color'))
    ]
  },
  {
    id: 'rail.project-settings',
    area: 'settings',
    title: 'Project settings: General',
    source: { screen: 'settings', name: 'project:general' },
    steps: [
      DASHBOARD,
      step(
        'b-settings',
        [betaMenu, click(label('Settings', 'menuitem'))],
        see('Basic project identification and location.')
      )
    ]
  },
  {
    id: 'rail.delete-project',
    area: 'rail',
    title: 'Delete project confirm',
    source: { overlay: ['components/ProjectSidebar.tsx'] },
    steps: [
      DASHBOARD,
      step(
        'b-confirm',
        [betaMenu, click(label('Delete', 'menuitem'))],
        see('Delete Project', 'Are you sure you want to delete "beta"?')
      )
    ]
  },
  {
    id: 'rail.delete-group',
    area: 'rail',
    title: 'Delete group confirm',
    source: { overlay: ['components/ProjectSidebar.tsx'] },
    blocked: NO_GROUP,
    steps: []
  },
  {
    id: 'rail.group-menu',
    area: 'rail',
    title: 'Group context menu',
    source: { overlay: ['components/sidebar/group-context-menu.tsx'] },
    blocked: NO_GROUP,
    steps: []
  },
  {
    id: 'rail.new-project',
    area: 'rail',
    title: 'Add project menu and new project modal',
    source: { overlay: ['components/sidebar/add-project-menu.tsx'] },
    steps: [
      DASHBOARD,
      step('b-menu', [addMenu], see('New project')),
      step(
        'c-modal',
        [click(txt('New project', false))],
        see('Create New Project', 'Root Directory')
      )
    ]
  },
  {
    id: 'rail.new-project-modal',
    area: 'rail',
    title: 'New project modal',
    source: { overlay: ['components/NewProjectModal.tsx'] },
    steps: [
      DASHBOARD,
      step('b-modal', [addMenu, click(id('header-new-project'))], see('Create New Project'))
    ]
  },
  {
    id: 'rail.directory-picker',
    area: 'rail',
    title: 'Directory picker (web Browse)',
    source: { overlay: ['components/DirectoryPicker.tsx'] },
    steps: [
      DASHBOARD,
      step('b-modal', [addMenu, click(id('header-new-project'))], see('Create New Project')),
      step('c-picker', [click(txt('Browse'))], see('Select'))
    ]
  },
  {
    id: 'rail.new-group',
    area: 'rail',
    title: 'New group modal',
    source: { overlay: ['components/NewGroupModal.tsx'] },
    steps: [
      DASHBOARD,
      step('b-modal', [addMenu, click(id('header-new-group'))], see('Create New Group'))
    ]
  },
  {
    id: 'rail.new-worktree',
    area: 'rail',
    title: 'New worktree modal',
    source: { overlay: ['components/NewWorktreeModal.tsx'] },
    steps: [
      DASHBOARD,
      step(
        'b-modal',
        [rclick(id('project-item-matrix-alpha')), click(txt('New worktree', false))],
        see('New Worktree', 'Path Preview')
      )
    ]
  },
  {
    id: 'rail.project-busy',
    area: 'rail',
    title: 'Project busy confirm',
    source: { overlay: ['components/ProjectBusyDialog.tsx'] },
    blocked: 'opens only when the host refuses a delete or archive because a process is running',
    steps: []
  },
  {
    id: 'rail.chat-menu',
    area: 'rail',
    title: 'Chat row menu',
    source: { overlay: ['components/shell/rail/rail-chat-menu.tsx'] },
    steps: [
      DASHBOARD,
      step(
        'b-menu',
        [click(css('button[aria-label^="More actions for How does"]'))],
        see('Copy chat ID', 'Delete chat')
      )
    ]
  },
  {
    id: 'rail.collapse',
    area: 'rail',
    title: 'Collapse the rail',
    source: { journey: 'starter' },
    steps: [DASHBOARD, step('b-collapsed', [collapse], [gone('Automations')])]
  },
  {
    id: 'rail.project-card',
    area: 'rail',
    title: 'Project card on hover (collapsed rail)',
    source: { overlay: ['components/sidebar/project-icon-item.tsx'] },
    steps: [
      DASHBOARD,
      step('b-collapsed', [collapse], [gone('Automations')]),
      step(
        'c-card',
        [hover(label('Project: beta'))],
        [{ text: 'beta', within: 'rail-project-card' }]
      )
    ]
  },
  {
    id: 'rail.chat-peek',
    area: 'rail',
    title: 'Collapsed-rail chat peek',
    source: { overlay: ['components/shell/rail/RailProjectChats.tsx'] },
    steps: [
      DASHBOARD,
      step('b-collapsed', [collapse], [gone('Automations')]),
      step(
        'c-peek',
        [hover(css('[data-testid="rail-project-chats"] button'))],
        [{ text: 'How does the web client', within: 'rail-chat-peek' }]
      )
    ]
  },
  {
    id: 'rail.quick-chats-peek',
    area: 'rail',
    title: 'Quick chats peek',
    source: { overlay: ['components/shell/rail/rail-unassigned-chats.tsx'] },
    blocked: 'needs a quick chat (a chat with no project), which the seeded world does not hold',
    steps: []
  },
  {
    id: 'rail.quick-chat-delete',
    area: 'rail',
    title: 'Delete quick chat confirm',
    source: { overlay: ['components/shell/rail/rail-unassigned-chats.tsx'] },
    blocked: 'needs a quick chat (a chat with no project), which the seeded world does not hold',
    steps: []
  },
  {
    id: 'rail.worktree-switcher',
    area: 'rail',
    title: 'Worktree switcher popover',
    source: { overlay: ['components/shell/rail/worktree-switcher.tsx'] },
    steps: [DASHBOARD, step('b-popover', switchWorktree, see('Search or create a worktree'))]
  },
  {
    id: 'rail.worktree-row-menu',
    area: 'rail',
    title: 'Worktree row context menu',
    source: { overlay: ['components/shell/rail/worktree-switcher.tsx'] },
    steps: [
      DASHBOARD,
      step('b-popover', switchWorktree, see('Search or create a worktree')),
      step(
        'c-menu',
        [rclick(css('[role="listbox"][aria-label="Working copies"] > *'))],
        see('Open terminal here')
      )
    ]
  }
]
