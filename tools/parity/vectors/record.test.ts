import { existsSync, readdirSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { describe, expect, it, vi } from 'vitest'
import {
  drawId,
  emit,
  type Manifest,
  manifestEntry,
  OUT_DIR,
  reads,
  recordVector,
  type Scenario,
  type StoreRecorder,
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

vi.mock(`${lib}/api`, async (importOriginal) => ({
  ...(await importOriginal<object>()),
  persistenceApi: {
    read: async (key: string) => {
      const data = reads.get(key)
      return data ? { success: true, data } : { success: false, code: 'KEY_NOT_FOUND', error: key }
    },
    write: async (_key: string, record: Record<string, unknown>) => writePrefs(record),
    writeDebounced: async (_key: string, record: Record<string, unknown>) => writePrefs(record),
    flushPendingWrites: async () => undefined
  }
}))

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
vi.mock('../../../src/renderer/stores/quick-chat-store', async (importOriginal) => ({
  ...(await importOriginal<object>()),
  closeQuickChatView: () => null
}))

const { workspaceRecorder, workspaceScenarios } = await import('./stores/workspace')
const { shortcutsRecorder, shortcutScenarios } = await import('./stores/shortcuts')
const { overlayRecorder, overlayScenarios } = await import('./stores/overlay')
const { projectsRecorder, projectScenarios } = await import('./stores/projects')

const STORES: Array<[StoreRecorder, Scenario[]]> = [
  [workspaceRecorder, workspaceScenarios],
  [shortcutsRecorder, shortcutScenarios],
  [overlayRecorder, overlayScenarios],
  [projectsRecorder, projectScenarios]
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
