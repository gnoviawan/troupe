import { existsSync, readdirSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { describe, expect, it, vi } from 'vitest'
import {
  dispatchOutcomes,
  drawId,
  emit,
  type Manifest,
  manifestEntry,
  OUT_DIR,
  openChatTabs,
  recordVector,
  sourcesOutsideWorkflow,
  storeText,
  type Vector,
  writeOut
} from './record'

// The ACP chat store records apart from the other stores: it needs the module
// mocks its own unit suites use (`src/renderer/stores/acp-store/*.test.ts`),
// and `bun run test` runs each file in a fresh global. Rewrite `out/acp/` with
//   RECORD_VECTORS=1 bun run test tools/parity/vectors

const renderer = '../../../src/renderer'

vi.mock('sonner', () => ({ toast: { error: () => {}, warning: () => {}, success: () => {} } }))
vi.mock('@tauri-apps/api/core', () => ({ invoke: async () => null }))
vi.mock('@tauri-apps/api/event', () => ({ listen: async () => () => {} }))
vi.mock(`${renderer}/lib/tauri-runtime`, async (importOriginal) => ({
  ...(await importOriginal<object>()),
  isTauriContext: () => false,
  cleanupTauriListener: () => {}
}))
vi.mock(`${renderer}/lib/log-api`, async (importOriginal) => ({
  ...(await importOriginal<object>()),
  logFrontendError: (payload: { level: string; source: string; message: string }) => {
    emit({ kind: 'log', level: payload.level, source: payload.source, message: payload.message })
    return Promise.resolve()
  }
}))
vi.mock(`${renderer}/lib/acp-history-persistence`, async (importOriginal) => {
  const actual =
    await importOriginal<typeof import('../../../src/renderer/lib/acp-history-persistence')>()
  return {
    ...actual,
    // The index reload is the host's; its answer arrives as its own event.
    loadSessionIndex: () => {
      emit({ kind: 'refreshSessionIndex' })
      return new Promise(() => {})
    },
    saveSessionIndex: async () => {},
    saveSessionPayload: async () => {},
    queueSessionPayloadSave: async () => {},
    queueSessionPayloadDelete: async () => {},
    loadSessionPayload: async (id: string) => actual.getCachedSessionPayload(id) ?? null,
    loadSessionPayloadTail: async () => null
  }
})
vi.mock(`${renderer}/lib/acp-agents-persistence`, async (importOriginal) => ({
  ...(await importOriginal<object>()),
  loadAgentConfigs: async () => [],
  saveAgentConfigs: async () => {}
}))
vi.mock(`${renderer}/lib/acp-mcp-persistence`, async (importOriginal) => ({
  ...(await importOriginal<object>()),
  loadMcpServers: async () => [],
  saveMcpServers: async () => {}
}))
vi.mock(`${renderer}/lib/api`, async (importOriginal) => ({
  ...(await importOriginal<object>()),
  persistenceApi: {
    read: async () => ({ success: false }),
    write: async () => ({ success: true }),
    writeDebounced: async () => ({ success: true }),
    delete: async () => ({ success: true }),
    flushPendingWrites: async () => undefined
  }
}))
vi.mock(`${renderer}/stores/workspace-store`, () => ({
  getAllLeafPanes: () => [],
  getAllTreeRoots: () => [],
  // A consent ticket asks for its chat tab; the vector keeps the ask, as the
  // model's `revealChatTab` effect, and finds no pane.
  findPaneContainingTab: (_root: unknown, tabId: string) => {
    emit({ kind: 'revealChatTab', sessionId: tabId.replace(/^chat-/, '') })
    return null
  },
  agentChatTabId: (sessionId: string) => `chat-${sessionId}`,
  getAgentChatSessionIdsInAllTrees: () => new Set(openChatTabs),
  useWorkspaceStore: {
    subscribe: () => () => {},
    getState: () => ({
      addAgentChatTab: () => {},
      removeTab: () => {},
      remapAgentChatSession: () => {},
      root: { type: 'leaf', id: 'pane-1', tabs: [], activeTabId: null }
    })
  }
}))
vi.mock(`${renderer}/lib/web-tab-session`, () => ({
  setTabFocusedSessionId: () => {},
  getTabFocusedSessionId: () => null
}))
// The session index and payload writes are the host's; the vector keeps the
// call, as the model's `persistSession` effect.
// `peekSeqCounter` reads the live `seqCounter` binding, which a spread copies.
vi.mock(`${renderer}/stores/acp-store/shared-state`, async (importOriginal) => {
  const actual = await importOriginal<{ seqCounter: number }>()
  return {
    ...actual,
    persistSession: (_state: unknown, sessionId: string) =>
      emit({ kind: 'persistSession', sessionId }),
    peekSeqCounter: () => actual.seqCounter
  }
})

// The prompt command boundary: each dispatch answers with the next outcome
// the vector lists (`context.dispatch`), else an accepted receipt.
function dispatchPrompt(
  agentId: string,
  sessionId: string,
  blocks: unknown[],
  turnId: string,
  displayBlocks?: unknown[]
): Promise<unknown> {
  emit({
    kind: 'dispatchPrompt',
    agentId,
    sessionId,
    turnId,
    blocks,
    ...(displayBlocks ? { displayBlocks } : {})
  })
  const outcome = dispatchOutcomes.shift() ?? { receipt: 'accepted' }
  const fields = (typeof outcome === 'object' && outcome !== null ? outcome : {}) as Record<
    string,
    unknown
  >
  if ('busy' in fields) {
    return Promise.reject(new Error('ACP_TURN_IN_PROGRESS: a prompt turn is already in progress'))
  }
  if ('error' in fields) return Promise.reject(new Error(String(fields.error)))
  if ('agentDead' in fields) {
    return Promise.reject(new Error('agent thread is no longer running'))
  }
  return Promise.resolve(outcome)
}
vi.mock(`${renderer}/lib/acp-api`, async (importOriginal) => {
  const actual = await importOriginal<{ acpApi: object }>()
  return {
    ...actual,
    acpApi: {
      ...actual.acpApi,
      sendPrompt: (agentId: string, sessionId: string, text: string, turnId: string) =>
        dispatchPrompt(agentId, sessionId, [{ type: 'text', text }], turnId),
      sendPromptBlocks: dispatchPrompt,
      cancelPrompt: async (agentId: string, sessionId: string) =>
        emit({ kind: 'cancelPrompt', agentId, sessionId })
    }
  }
})

// The model list save and the read of the user's saved composer picks are
// the boundary of the options cache; the vector keeps both calls.
vi.mock(`${renderer}/stores/acp-store/model-catalog`, async (importOriginal) => ({
  ...(await importOriginal<object>()),
  persistModelCatalog: (configId: string, catalog: unknown) =>
    emit({ kind: 'persistModelCatalog', configId, catalog })
}))
vi.mock(`${renderer}/stores/acp-store/composer-options`, async (importOriginal) => ({
  ...(await importOriginal<object>()),
  readComposerOptions: async (configId: string) => {
    emit({ kind: 'reapplyComposerOptions', configId })
    return null
  }
}))

const { acpRecorder, acpScenarios } = await import('./stores/acp')

const ACP_OUT = join(OUT_DIR, 'acp')
const write = process.env.RECORD_VECTORS === '1'

describe('acp test vectors', () => {
  it('match what the ACP chat store does today', async () => {
    vi.spyOn(globalThis.crypto, 'randomUUID').mockImplementation(drawId)
    const recorded: Vector[] = []
    for (const scenario of acpScenarios) recorded.push(await recordVector(acpRecorder, scenario))
    expect(sourcesOutsideWorkflow(acpRecorder.sources), 'parity-vectors.yml paths').toEqual([])
    const manifest: Manifest = { stores: { acp: manifestEntry(acpRecorder, recorded) } }
    const byStore = new Map([['acp', recorded]])
    if (write) {
      writeOut(manifest, byStore, ACP_OUT)
      return
    }
    const onDisk = JSON.parse(readFileSync(join(ACP_OUT, 'manifest.json'), 'utf8')) as Manifest
    expect(onDisk, 'out/acp/manifest.json is stale: rerun with RECORD_VECTORS=1').toEqual(manifest)
    const path = join(ACP_OUT, 'acp.json')
    expect(existsSync(path), `${path} is missing`).toBe(true)
    expect(readFileSync(path, 'utf8'), `${path} is stale`).toBe(storeText(recorded))
    const files = readdirSync(ACP_OUT).filter((file) => file.endsWith('.json'))
    expect(files.length, 'out/acp/ holds files no store records').toBe(2)
  })
})
