import type { IpcResult } from '@shared/types/ipc.types'
import type { ProjectPrefsRecord } from '@shared/types/persistence.types'
import type { ProjectListPayload, ProjectSummary } from '@shared/types/web-projects.types'
import { act, renderHook, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const h = vi.hoisted(() => {
  const store = new Map<string, unknown>()
  const calls: string[] = []
  return {
    store,
    calls,
    heldWrites: null as Array<(result: IpcResult<void>) => void> | null,
    tauri: { value: false },
    subscribe: vi.fn(),
    unwatch: vi.fn(),
    importLegacyProjects: vi.fn(),
    hydrateShellLayout: vi.fn(async () => {}),
    grantFsScope: vi.fn(),
    reconcile: vi.fn()
  }
})

vi.mock('@/lib/acp-transport', () => ({
  getAcpTransport: () => ({ watches: { subscribe: h.subscribe } })
}))
vi.mock('@/lib/api', () => ({
  persistenceApi: {
    read: vi.fn(async (key: string) =>
      h.store.has(key)
        ? { success: true, data: h.store.get(key) }
        : { success: false, code: 'KEY_NOT_FOUND', error: 'none' }
    ),
    write: vi.fn(async (key: string, data: unknown) => {
      h.store.set(key, data)
      return { success: true, data: undefined }
    }),
    writeDebounced: vi.fn(async (key: string, data: unknown) => {
      if (h.heldWrites) return new Promise((resolve) => h.heldWrites?.push(resolve))
      h.store.set(key, data)
      return { success: true, data: undefined }
    }),
    flushPendingWrites: vi.fn(async () => ({ success: true, data: undefined }))
  },
  secureStorageApi: { getSecret: vi.fn(), setSecret: vi.fn(), deleteSecret: vi.fn() },
  filesystemApi: { grantFsScope: h.grantFsScope }
}))
vi.mock('@/lib/tauri-runtime', () => ({ isTauriContext: () => h.tauri.value }))
vi.mock('@/lib/web-auth-gate', () => ({ useWebAuthGateOk: () => true }))
vi.mock('@/lib/legacy-project-import', () => ({
  importLegacyProjects: h.importLegacyProjects,
  LEGACY_PROJECTS_KEY: 'projects'
}))
vi.mock('@/lib/worktree-reconciler', () => ({ reconcileProjectWorktrees: h.reconcile }))
vi.mock('@/lib/log-api', () => ({ logFrontendError: vi.fn(() => Promise.resolve()) }))
vi.mock('@/lib/project-registry-api', () => ({
  ProjectBusyError: class extends Error {},
  projectRegistryApi: {}
}))
vi.mock('@/lib/router-navigate', () => ({ clearChatRoute: vi.fn() }))
vi.mock('@/stores/shell-layout-store', () => ({
  useShellLayoutStore: { getState: () => ({ hydrate: h.hydrateShellLayout }) }
}))

import { persistenceApi, secureStorageApi } from '@/lib/api'
import { CLOSE_WRITE_TIMEOUT_MS, runCloseWriters } from '@/lib/close-flush'
import { _resetProjectPrefsForTests } from '@/lib/project-prefs'
import { useProjectStore } from '@/stores/project-store'
import { PREFS_SAVE_RETRY_MS, useProjectRegistry } from './use-project-registry'

function summary(id: string, extra: Partial<ProjectSummary> = {}): ProjectSummary {
  return {
    id,
    name: `Project ${id}`,
    color: 'blue',
    path: `/work/${id}`,
    isArchived: false,
    isDefault: false,
    ...extra
  }
}

function list(
  projects: ProjectSummary[],
  defaultProjectId: string | null = null
): ProjectListPayload {
  return { projects, defaultProjectId }
}

/** The handlers the hook registered on the `projects` watch. */
function watchHandlers(): {
  onState: (list: ProjectListPayload) => void
  onDelta: (list: ProjectListPayload) => void
} {
  return h.subscribe.mock.calls[0][1]
}

function mount(): ReturnType<typeof renderHook> {
  return renderHook(() => useProjectRegistry())
}

async function mountAndDeliver(state: ProjectListPayload): Promise<ReturnType<typeof renderHook>> {
  const rendered = mount()
  await waitFor(() => expect(h.subscribe).toHaveBeenCalled())
  act(() => watchHandlers().onState(state))
  await waitFor(() => expect(useProjectStore.getState().isLoaded).toBe(true))
  return rendered
}

beforeEach(() => {
  vi.clearAllMocks()
  h.store.clear()
  h.calls.length = 0
  h.heldWrites = null
  h.tauri.value = false
  h.subscribe.mockReturnValue(h.unwatch)
  h.importLegacyProjects.mockResolvedValue('skipped')
  h.grantFsScope.mockResolvedValue({ success: true, data: undefined })
  _resetProjectPrefsForTests()
  useProjectStore.setState({ projects: [], groups: [], activeProjectId: '', isLoaded: false })
})

describe('useProjectRegistry', () => {
  describe('prefs read failure', () => {
    it('saves a group created after the first prefs read failed', async () => {
      vi.mocked(persistenceApi.read).mockImplementationOnce(async () => ({
        success: false,
        code: 'TIMEOUT',
        error: 'store socket did not answer'
      }))

      await mountAndDeliver(list([summary('a')]))
      act(() => useProjectStore.getState().addGroup('Survives restart'))

      await waitFor(() => {
        const saved = h.store.get('project-prefs') as ProjectPrefsRecord | undefined
        expect(saved?.groups.map((g) => g.name)).toEqual(['Survives restart'])
      })
    })

    it('keeps the saved groups and saves again once the prefs read recovers', async () => {
      h.store.set('project-prefs', {
        projects: {},
        groups: [{ id: 'g-old', name: 'Old group', projectIds: [] }],
        order: [],
        updatedAt: 't'
      } satisfies ProjectPrefsRecord)
      // Every boot attempt fails: the store socket is still not answering.
      const boot = 1 + 3
      for (let i = 0; i < boot; i++) {
        vi.mocked(persistenceApi.read).mockImplementationOnce(async () => ({
          success: false,
          code: 'TIMEOUT',
          error: 'store socket did not answer'
        }))
      }

      // The boot retries take a few seconds, so wait for the watch past the default.
      mount()
      await waitFor(() => expect(h.subscribe).toHaveBeenCalled(), { timeout: 8000 })
      act(() => watchHandlers().onState(list([summary('a')])))
      await waitFor(() => expect(useProjectStore.getState().isLoaded).toBe(true))
      act(() => useProjectStore.getState().addGroup('Made while unreadable'))

      await waitFor(
        () => {
          const saved = h.store.get('project-prefs') as ProjectPrefsRecord | undefined
          expect(saved?.groups.map((g) => g.name)).toEqual(['Made while unreadable', 'Old group'])
        },
        { timeout: 9000 }
      )
      expect(useProjectStore.getState().groups.map((g) => g.name)).toEqual([
        'Made while unreadable',
        'Old group'
      ])
    }, 15000)

    it('keeps a listed project saved prefs after the read recovers', async () => {
      h.store.set('project-prefs', {
        projects: { a: { defaultShell: '/bin/zsh', gitBranch: 'feature' } },
        groups: [],
        order: [],
        updatedAt: 't'
      } satisfies ProjectPrefsRecord)
      for (let i = 0; i < 1 + 3; i++) {
        vi.mocked(persistenceApi.read).mockImplementationOnce(async () => ({
          success: false,
          code: 'TIMEOUT',
          error: 'store socket did not answer'
        }))
      }

      mount()
      await waitFor(() => expect(h.subscribe).toHaveBeenCalled(), { timeout: 8000 })
      act(() => watchHandlers().onState(list([summary('a')])))
      await waitFor(() => expect(useProjectStore.getState().isLoaded).toBe(true))
      act(() => useProjectStore.getState().addGroup('Triggers a save'))

      // The group only reaches the record once the read has recovered and saving is on.
      await waitFor(
        () => {
          const saved = h.store.get('project-prefs') as ProjectPrefsRecord | undefined
          expect(saved?.groups.map((g) => g.name)).toEqual(['Triggers a save'])
        },
        { timeout: 9000 }
      )
      const saved = h.store.get('project-prefs') as ProjectPrefsRecord
      expect(saved.projects.a).toMatchObject({ defaultShell: '/bin/zsh', gitBranch: 'feature' })
      expect(useProjectStore.getState().projects[0]).toMatchObject({
        defaultShell: '/bin/zsh',
        gitBranch: 'feature'
      })
    }, 15000)

    it('stops retrying the prefs read once the hook is unmounted', async () => {
      vi.mocked(persistenceApi.read).mockImplementationOnce(async () => ({
        success: false,
        code: 'TIMEOUT',
        error: 'store socket did not answer'
      }))
      const prefsReads = () =>
        vi.mocked(persistenceApi.read).mock.calls.filter(([key]) => key === 'project-prefs').length

      const { unmount } = mount()
      await waitFor(() => expect(prefsReads()).toBe(1))
      unmount()

      // Longer than the first retry delay: a live hook would read the prefs again by now.
      await new Promise((resolve) => setTimeout(resolve, 800))
      expect(prefsReads()).toBe(1)
    })
  })
})
