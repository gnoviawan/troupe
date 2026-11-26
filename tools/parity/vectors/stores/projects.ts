import {
  _resetProjectPrefsForTests,
  forgetProjectPrefs,
  getProjectLayout,
  loadProjectPrefs,
  persistProjectPrefs,
  rememberProjectPrefs
} from '../../../../src/renderer/lib/project-prefs'
import { applyRegistryList } from '../../../../src/renderer/stores/project-registry-feed'
import { useProjectStore } from '../../../../src/renderer/stores/project-store'
import type { ProjectPrefsRecord } from '../../../../src/shared/types/persistence.types'
import { reads, type Scenario, type StoreRecorder, type VectorEvent } from '../record'

const ev = (action: string, ...args: unknown[]): VectorEvent => ({ action, args })

const summary = (id: string, extra: Record<string, unknown> = {}) => ({
  id,
  name: `Project ${id}`,
  color: 'blue',
  path: `/work/${id}`,
  isArchived: false,
  isDefault: false,
  ...extra
})
const list = (
  ids: string[],
  defaultProjectId: string | null = null,
  extra: Record<string, Record<string, unknown>> = {}
) => ({
  projects: ids.map((id) => summary(id, extra[id])),
  defaultProjectId
})
const project = (id: string, extra: Record<string, unknown> = {}) => ({
  id,
  name: `Project ${id}`,
  color: 'blue',
  path: `/work/${id}`,
  ...extra
})
const worktree = (id: string) => ({
  id,
  name: id,
  branch: `feat/${id}`,
  path: `/work/a/.termul/worktrees/${id}`,
  createdAt: '2026-11-01T00:00:00.000Z'
})

const savedRecord = {
  projects: {
    b: { defaultShell: 'zsh', gitBranch: 'dev', isGitRepo: true, worktrees: [worktree('w1')] },
    gone: { symlinkDirs: ['node_modules'] }
  },
  groups: [{ id: 'g1', name: 'Work', projectIds: ['b', 'c'], isCollapsed: true, color: 'red' }],
  order: ['c', 'gone', 'b'],
  updatedAt: '2026-11-01T00:00:00.000Z'
}

export const projectScenarios: Scenario[] = [
  {
    name: 'first_list_uses_saved_layout_and_selection',
    steps: [
      ev('loadProjectPrefs', savedRecord),
      ev('addGroup', 'Early'),
      ev('applyRegistryList', list(['a', 'b', 'c', 'd'], 'a'), 'b'),
      ev('applyRegistryList', list(['a', 'c', 'e'], 'a', { a: { name: 'Renamed' } }), 'b'),
      ev('applyRegistryList', list(['c', 'e'], 'e'), null)
    ]
  },
  {
    name: 'saved_selection_archived_falls_back',
    steps: [
      ev('loadProjectPrefs', null),
      ev('applyRegistryList', list(['a', 'b'], 'zz', { a: { isArchived: true } }), 'a'),
      ev('applyRegistryList', list([], null), null)
    ]
  },
  {
    name: 'host_default_when_nothing_saved',
    steps: [
      ev('loadProjectPrefs', null),
      ev('applyRegistryList', list(['a', 'b', 'c'], 'c', { c: { path: null } }), null)
    ]
  },
  {
    name: 'select_project',
    steps: [
      ev('setProjects', [project('a'), project('b'), project('c', { isArchived: true })]),
      ev('selectProject', 'b'),
      ev('selectProject', 'b'),
      ev('selectProject', 'zz')
    ]
  },
  {
    name: 'update_project_splits_host_and_device_fields',
    steps: [
      ev('setProjects', [project('a'), project('b')], 'a'),
      ev('updateProject', 'a', { defaultShell: '/bin/zsh', gitBranch: 'main' }),
      ev('updateProject', 'a', { name: 'Alpha', color: 'blue', path: '  /work/a ' }),
      // JavaScript's trim drops U+FEFF and keeps U+0085, unlike Rust's.
      ev('updateProject', 'a', { path: '\uFEFF/work/a' }),
      ev('updateProject', 'a', { path: '/work/a\u0085' }),
      ev('updateProject', 'b', { color: 'green', path: '  /elsewhere  ', symlinkDirs: ['x'] }),
      ev('updateProject', 'b', { path: '   ' }),
      ev('updateProject', 'missing', { name: 'Ghost' })
    ]
  },
  {
    name: 'reorder_projects_keeps_unlisted_and_archived',
    steps: [
      ev('setProjects', [
        project('a'),
        project('x', { isArchived: true }),
        project('b'),
        project('c')
      ]),
      ev('reorderProjects', ['c', 'x', 'ghost', 'a']),
      ev('reorderProjects', [])
    ]
  },
  {
    name: 'worktrees_and_lock',
    steps: [
      ev('setProjects', [project('a'), project('b', { worktrees: [worktree('w0')] })]),
      ev('addWorktree', 'a', worktree('w1')),
      ev('addWorktree', 'b', worktree('w2')),
      ev('removeWorktree', 'b', 'w0'),
      ev('removeWorktree', 'a', 'nope'),
      ev('setWorktreeOperationLock', true),
      ev('setWorktreeOperationLock', false)
    ]
  },
  {
    name: 'groups',
    steps: [
      ev('setProjects', [project('a'), project('b'), project('c')], 'a', [
        { id: 'g0', name: 'Zero', projectIds: ['c'] }
      ]),
      ev('addGroup', 'One'),
      ev('addGroup', 'Two'),
      ev('moveProjectToGroup', 'a', 'id-1'),
      ev('moveProjectToGroup', 'b', 'id-1', 0),
      ev('moveProjectToGroup', 'c', 'id-1', 9),
      ev('toggleGroupCollapse', 'id-1'),
      ev('moveProjectToGroup', 'a', 'id-2'),
      ev('moveProjectToGroup', 'b', null),
      ev('renameGroup', 'id-2', 'Second'),
      ev('updateGroup', 'id-2', { color: 'purple', isCollapsed: true }),
      ev('reorderGroups', ['id-2', 'nope', 'g0']),
      ev('reorderProjectInGroup', 'id-1', ['c', 'zz']),
      ev('removeGroup', 'nope', true),
      ev('removeGroup', 'g0', false),
      ev('removeGroup', 'id-1', true)
    ]
  },
  {
    name: 'persist_prefs_skips_unchanged_record',
    steps: [
      ev('loadProjectPrefs', savedRecord),
      ev('applyRegistryList', list(['a', 'b']), 'a'),
      ev('persistProjectPrefs'),
      ev('persistProjectPrefs'),
      ev('updateProject', 'a', {
        defaultShell: 'fish',
        envVars: [{ key: 'TOKEN', value: 'x', isSecret: true }],
        worktrees: [{ ...worktree('w9'), outsideProjectRoot: true }],
        icon: { dataUri: 'data:image/png;base64,AA==', fetchedAt: 1 }
      }),
      ev('persistProjectPrefs'),
      ev('forgetProjectPrefs', 'b'),
      ev('persistProjectPrefs')
    ]
  },
  {
    name: 'remembered_prefs_reach_a_new_project',
    steps: [
      ev('loadProjectPrefs', null),
      ev('applyRegistryList', list(['a']), null),
      ev('rememberProjectPrefs', 'n', {
        defaultShell: 'bash',
        envVars: [{ key: 'A', value: '1' }]
      }),
      ev('applyRegistryList', list(['a', 'n']), null),
      ev('selectProject', 'n'),
      ev('persistProjectPrefs')
    ]
  }
]

export const projectsRecorder: StoreRecorder = {
  store: 'projects',
  sources: [
    'src/renderer/stores/project-store.ts',
    'src/renderer/stores/project-registry-feed.ts',
    'src/renderer/lib/project-prefs.ts'
  ],
  reset() {
    _resetProjectPrefsForTests()
    reads.delete('project-prefs')
    useProjectStore.setState({
      projects: [],
      groups: [],
      activeProjectId: '',
      isLoaded: false,
      isWorktreeOperationLocked: false
    })
  },
  seed(initial) {
    // The layout lives in the prefs module, not the store.
    if ('layout' in initial) throw new Error('seed the layout with a loadProjectPrefs step')
    useProjectStore.setState(structuredClone(initial))
  },
  snapshot() {
    const { projects, groups, activeProjectId, isLoaded, isWorktreeOperationLocked } =
      useProjectStore.getState()
    return {
      projects,
      groups,
      activeProjectId,
      isLoaded,
      isWorktreeOperationLocked,
      layout: getProjectLayout()
    }
  },
  dispatch({ action, args }) {
    switch (action) {
      case 'loadProjectPrefs':
        reads.set('project-prefs', args[0] as ProjectPrefsRecord | null)
        return loadProjectPrefs()
      case 'rememberProjectPrefs':
        return rememberProjectPrefs(args[0] as string, args[1] as never)
      case 'persistProjectPrefs': {
        const { projects, groups } = useProjectStore.getState()
        return persistProjectPrefs(projects, groups, [])
      }
      case 'forgetProjectPrefs':
        return forgetProjectPrefs(args[0] as string)
      case 'applyRegistryList':
        return applyRegistryList(args[0] as never, args[1] as string | null)
    }
    const fn = (useProjectStore.getState() as unknown as Record<string, unknown>)[action]
    if (typeof fn !== 'function') throw new Error(`project store has no action ${action}`)
    return fn(...args)
  }
}
