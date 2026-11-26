import { existsSync, readdirSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { describe, expect, it, vi } from 'vitest'
import {
  drawId,
  emit,
  idleWindowReplies,
  type Manifest,
  manifestEntry,
  OUT_DIR,
  readFailures,
  reads,
  recordVector,
  type Scenario,
  type StoreRecorder,
  settingsWatch,
  sourcesOutsideWorkflow,
  storeText,
  type Vector,
  writeOut
} from './record'

const lib = '../../../src/renderer/lib'

vi.mock(`${lib}/log-api`, async (importOriginal) => ({
  ...(await importOriginal<object>()),
  logFrontendError: (payload: { level: string; source: string; message: string }) => {
    emit({ kind: 'log', level: payload.level, source: payload.source, message: payload.message })
    return Promise.resolve()
  }
}))

// `isChatRouteCurrent` answers yes, so a navigation the store asks to replace
// names the chat route it replaces.
let routeQuery: string | null = null
vi.mock(`${lib}/router-navigate`, async (importOriginal) => ({
  ...(await importOriginal<object>()),
  isChatRouteCurrent: (sessionId: string) => {
    routeQuery = sessionId
    return true
  },
  navigateToChatSession: (sessionId: string, options?: { replace?: boolean }) => {
    emit({
      kind: 'navigateToChat',
      sessionId,
      ...(options?.replace && routeQuery ? { replaceIfOn: routeQuery } : {})
    })
    routeQuery = null
  },
  clearChatRoute: () => emit({ kind: 'clearChatRoute' })
}))

vi.mock(`${lib}/api`, async (importOriginal) => {
  const actual = await importOriginal<{ acpApi: object }>()
  const push = (op: string) => async (secs: number | null) => {
    emit({ kind: 'acp', op, secs })
    const next = idleWindowReplies[0] as { failPush?: string } | undefined
    if (op === 'setAgentIdleWindow' && next?.failPush) {
      idleWindowReplies.shift()
      throw new Error(next.failPush)
    }
  }
  return {
    ...actual,
    // The settings vectors record the ACP pushes; no other store here calls them.
    acpApi: {
      ...actual.acpApi,
      setTurnTimeout: push('setTurnTimeout'),
      setTurnIdleTimeout: push('setTurnIdleTimeout'),
      setSessionNewTimeout: push('setSessionNewTimeout'),
      setSessionReopenTimeout: push('setSessionReopenTimeout'),
      setAgentIdleWindow: push('setAgentIdleWindow'),
      getAgentIdleWindow: async () => {
        emit({ kind: 'acp', op: 'getAgentIdleWindow' })
        const reply = idleWindowReplies.shift() as { reject?: string } | undefined
        if (!reply) throw new Error('no idle-window reply')
        if (reply.reject) throw new Error(reply.reject)
        return reply
      }
    },
    persistenceApi: {
      watchShared: (key: string, apply: (change: unknown) => void) => {
        if (key !== 'settings/app') return () => undefined
        settingsWatch.apply = apply
        return () => {
          settingsWatch.apply = null
        }
      },
      read: async (key: string) => {
        const failed = readFailures.get(key)
        if (failed) return { success: false, code: failed, error: key }
        const data = reads.get(key)
        return data
          ? { success: true, data }
          : { success: false, code: 'KEY_NOT_FOUND', error: key }
      },
      write: async (key: string, record: Record<string, unknown>) => writeRecord(key, record, true),
      writeDebounced: async (key: string, record: Record<string, unknown>) =>
        writeRecord(key, record, false),
      delete: async (key: string) => {
        emit({ kind: 'deleteStoreKey', key })
        return { success: true, data: undefined }
      },
      flushPendingWrites: async () => undefined
    }
  }
})

function writeRecord(key: string, record: Record<string, unknown>, immediate: boolean) {
  if (key === 'settings/app') {
    emit({ kind: 'writeSettings', record, immediate })
    return { success: true, data: undefined }
  }
  if (key !== 'shell-layout') return writePrefs(record)
  emit({ kind: 'writeShellLayout', record, immediate })
  return { success: true, data: undefined }
}

/** The device record, without the wall-clock `updatedAt` the app stamps. */
function writePrefs({ updatedAt: _stamp, ...record }: Record<string, unknown>) {
  emit({ kind: 'writePrefs', record })
  return { success: true, data: undefined }
}

vi.mock(`${lib}/tauri-runtime`, async (importOriginal) => ({
  ...(await importOriginal<object>()),
  isTauriContext: () => false
}))
vi.mock(`${lib}/active-project`, async (importOriginal) => ({
  ...(await importOriginal<object>()),
  writeActiveProject: (projectId: string) => emit({ kind: 'saveActiveProject', projectId })
}))
vi.mock(`${lib}/project-registry-api`, async (importOriginal) => ({
  ...(await importOriginal<object>()),
  projectRegistryApi: {
    update: async (projectId: string, patch: object) => {
      emit({ kind: 'updateProjectOnHost', projectId, patch })
    }
  }
}))
// The delete flow starts by unmounting the project's canvases; stop it there.
vi.mock(`${lib}/canvas-unmount-flow`, async (importOriginal) => ({
  ...(await importOriginal<object>()),
  unmountCanvasesFor: async (projectIds: string[]) => {
    emit({ kind: 'deleteProject', projectId: projectIds[0] })
    return null
  }
}))
// The vectors record the quick chat as closed.
vi.mock('../../../src/renderer/stores/agent-chat-lifetime-store', async (importOriginal) => ({
  ...(await importOriginal<object>()),
  useAgentChatLifetimeStore: {
    getState: () => ({
      forgetProject: (projectId: string) => emit({ kind: 'forgetProjectChats', projectId })
    })
  }
}))
vi.mock('../../../src/renderer/stores/quick-chat-store', async (importOriginal) => ({
  ...(await importOriginal<object>()),
  closeQuickChatView: () => null
}))

// The git store's host calls answer the vector's `context.git` queue.
vi.mock(`${lib}/git-api`, async (importOriginal) => {
  const { gitCall } = await import('./stores/git-mock')
  const ops = [
    'getStatus',
    'getDiff',
    'getDiffStat',
    'stage',
    'unstage',
    'discard',
    'stageHunk',
    'unstageHunk',
    'getLog',
    'commit',
    'push',
    'getCommitContext',
    'stashSave',
    'stashList',
    'stashApply',
    'stashPop',
    'stashDrop',
    'branchList',
    'branchSwitch',
    'createBranch'
  ]
  // `realGitApi` is the web client's own facade, for the route vectors.
  const actual = await importOriginal<{ gitApi: object }>()
  return {
    gitApi: Object.fromEntries(ops.map((op) => [op, gitCall(op)])),
    realGitApi: actual.gitApi
  }
})
vi.mock(`${lib}/worktree-reconciler`, async (importOriginal) => ({
  ...(await importOriginal<object>()),
  reconcileWorktreeOwnerOf: (cwd: string) => {
    emit({ kind: 'reconcileWorktree', cwd })
    return null
  }
}))
// Only the git store's toasts are recorded; the other stores keep sonner's own.
vi.mock('sonner', async (importOriginal) => {
  const actual = await importOriginal<typeof import('sonner')>()
  const { gitMock } = await import('./stores/git-mock')
  const error = (message: unknown) =>
    gitMock.active
      ? emit({ kind: 'toast', level: 'error', message })
      : actual.toast.error(String(message))
  return { ...actual, toast: Object.assign(actual.toast, { error }) }
})

const { workspaceRecorder, workspaceScenarios } = await import('./stores/workspace')
const { shortcutsRecorder, shortcutScenarios } = await import('./stores/shortcuts')
const { overlayRecorder, overlayScenarios } = await import('./stores/overlay')
const { projectsRecorder, projectScenarios } = await import('./stores/projects')
const { shellLayoutRecorder, shellLayoutScenarios } = await import('./stores/shell-layout')
const { paneTreeRecorder, paneTreeScenarios } = await import('./stores/pane-tree')
const { gitStatusRecorder, gitStatusScenarios } = await import('./stores/git-status')
const { gitMock } = await import('./stores/git-mock')
const { gitRoutesRecorder, gitRoutesScenarios } = await import('./stores/git-routes')
const { appSettingsRecorder, appSettingsScenarios } = await import('./stores/app-settings')
const { settingsRoutesRecorder, settingsRoutesScenarios } = await import('./stores/settings-routes')
const { paletteRecorder, paletteScenarios } = await import('./stores/palette')
const { terminalProfilesRecorder, terminalProfilesScenarios } = await import(
  './stores/terminal-profiles'
)
const { projectSettingsRecorder, projectSettingsScenarios } = await import(
  './stores/project-settings'
)
const { mcpServersRecorder, mcpServersScenarios } = await import('./stores/mcp-servers')
const { realGitApi } = (await import(`${lib}/git-api`)) as unknown as {
  realGitApi: Record<string, (...args: unknown[]) => unknown>
}

const STORES: Array<[StoreRecorder, Scenario[]]> = [
  [workspaceRecorder, workspaceScenarios],
  [shortcutsRecorder, shortcutScenarios],
  [overlayRecorder, overlayScenarios],
  [projectsRecorder, projectScenarios],
  [shellLayoutRecorder, shellLayoutScenarios],
  [paneTreeRecorder, paneTreeScenarios],
  [gitStatusRecorder, gitStatusScenarios],
  [gitRoutesRecorder(realGitApi), gitRoutesScenarios],
  [appSettingsRecorder, appSettingsScenarios],
  [settingsRoutesRecorder(), settingsRoutesScenarios],
  [paletteRecorder, paletteScenarios],
  [terminalProfilesRecorder, terminalProfilesScenarios],
  [projectSettingsRecorder, projectSettingsScenarios],
  [mcpServersRecorder, mcpServersScenarios]
]

const write = process.env.RECORD_VECTORS === '1'

describe('test vectors', () => {
  it('match what the TypeScript stores do today', async () => {
    vi.spyOn(globalThis.crypto, 'randomUUID').mockImplementation(drawId)
    const manifest: Manifest = { stores: {} }
    const byStore = new Map<string, Vector[]>()
    for (const [recorder, scenarios] of STORES) {
      const recorded: Vector[] = []
      for (const scenario of scenarios) recorded.push(await recordVector(recorder, scenario))
      gitMock.active = false
      for (const file of ['parity-vectors.yml', 'rune.yml'] as const) {
        expect(sourcesOutsideWorkflow(recorder.sources, file), `${file} paths`).toEqual([])
      }
      manifest.stores[recorder.store] = manifestEntry(recorder, recorded)
      byStore.set(recorder.store, recorded)
    }
    if (write) {
      writeOut(manifest, byStore)
      return
    }
    const onDisk = JSON.parse(readFileSync(join(OUT_DIR, 'manifest.json'), 'utf8')) as Manifest
    expect(onDisk, 'out/manifest.json is stale: rerun with RECORD_VECTORS=1').toEqual(manifest)
    for (const [store, vectors] of byStore) {
      const path = join(OUT_DIR, `${store}.json`)
      expect(existsSync(path), `${path} is missing`).toBe(true)
      expect(readFileSync(path, 'utf8'), `${path} is stale`).toBe(storeText(vectors))
    }
    const files = readdirSync(OUT_DIR).filter((file) => file.endsWith('.json'))
    expect(files.length, 'out/ holds files no store records').toBe(byStore.size + 1)
  })
})
