import { existsSync, readdirSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { describe, expect, it, vi } from 'vitest'
import {
  agentReplies,
  composerRecord,
  configReplies,
  dispatchOutcomes,
  drawId,
  emit,
  heldReplies,
  type Manifest,
  manifestEntry,
  mcpReplies,
  mcpSharedSync,
  OUT_DIR,
  openChatTabs,
  platform,
  recordVector,
  sessionPayloads,
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

vi.mock('sonner', () => ({
  toast: {
    error: (message: unknown) => emit({ kind: 'toast', level: 'error', message }),
    warning: (message: unknown) => emit({ kind: 'toast', level: 'warning', message }),
    success: (message: unknown) => emit({ kind: 'toast', level: 'success', message })
  }
}))
vi.mock('@tauri-apps/api/core', () => ({ invoke: async () => null }))
vi.mock('@tauri-apps/api/event', () => ({ listen: async () => () => {} }))
vi.mock(`${renderer}/lib/tauri-runtime`, async (importOriginal) => ({
  ...(await importOriginal<object>()),
  isTauriContext: () => platform.desktop,
  cleanupTauriListener: () => {}
}))
vi.mock(`${renderer}/lib/log-api`, async (importOriginal) => ({
  ...(await importOriginal<object>()),
  logFrontendError: (payload: { level: string; source: string; message: string }) => {
    emit({
      kind: 'log',
      level: payload.level ?? 'error',
      source: payload.source,
      message: payload.message
    })
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
    // The history delete is the model's `deleteSessionPayload` command.
    queueSessionPayloadDelete: (sessionId: string) =>
      agentCommand({ kind: 'deleteSessionPayload', sessionId }),
    // A payload read is the model's `readSessionPayload` effect; it answers
    // `context.payloads` the way an agent command answers its reply.
    loadSessionPayload: (id: string) => {
      emit({ kind: 'readSessionPayload', sessionId: id })
      return answer(sessionPayloads.get(id) ?? null)
    },
    loadSessionPayloadTail: async () => null
  }
})
vi.mock(`${renderer}/lib/acp-agents-persistence`, async (importOriginal) => ({
  ...(await importOriginal<object>()),
  loadAgentConfigs: async () => [],
  saveAgentConfigs: async () => {}
}))
// The MCP registry's disk is the model's `loadMcpServers` and `saveMcpServers`
// effects; both answer `context.mcpReplies`.
vi.mock(`${renderer}/lib/acp-mcp-persistence`, async (importOriginal) => ({
  ...(await importOriginal<object>()),
  loadMcpServers: () => mcpCommand({ kind: 'loadMcpServers' }),
  saveMcpServers: (servers: unknown) => mcpCommand({ kind: 'saveMcpServers', servers })
}))
vi.mock(`${renderer}/lib/shared-key-sync`, () => ({
  syncSharedKey: (_key: string, apply: (change: unknown) => void) => {
    emit({ kind: 'watchSharedMcpServers' })
    mcpSharedSync.apply = apply
    return () => {}
  }
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
// One tree holds a chat tab per `context.openChatTabs` session; the index
// prune's tab removal is the model's `closeChatTab` effect.
vi.mock(`${renderer}/stores/workspace-store`, () => ({
  getAllLeafPanes: () => [
    {
      tabs: [...openChatTabs].map((sessionId) => ({
        type: 'agent-chat',
        id: `chat-${sessionId}`,
        sessionId
      }))
    }
  ],
  getAllTreeRoots: () => [{}],
  // A consent ticket asks for its chat tab; the vector keeps the ask, as the
  // model's `revealChatTab` effect, and finds no pane.
  findPaneContainingTab: (_root: unknown, tabId: string) => {
    emit({ kind: 'revealChatTab', sessionId: tabId.replace(/^chat-/, '') })
    return null
  },
  agentChatTabId: (sessionId: string) => `chat-${sessionId}`,
  browserTabId: (tabId: string) => `browser-${tabId}`,
  getAgentChatSessionIdsInAllTrees: () => new Set(openChatTabs),
  useWorkspaceStore: {
    subscribe: () => () => {},
    getState: () => ({
      addAgentChatTab: () => {},
      // An agent's browser tab opens and closes as one workspace effect; the
      // browser session store beside it follows the tab.
      openAgentBrowserTab: (tabId: string, url?: string) =>
        emit({ kind: 'openAgentBrowserTab', tabId, ...(url ? { url } : {}) }),
      removeTab: (tabId: string) =>
        tabId.startsWith('browser-')
          ? emit({ kind: 'closeAgentBrowserTab', tabId: tabId.replace(/^browser-/, '') })
          : emit({ kind: 'closeChatTab', sessionId: tabId.replace(/^chat-/, '') }),
      remapAgentChatSession: () => {},
      root: { type: 'leaf', id: 'pane-1', tabs: [], activeTabId: null }
    })
  }
}))
vi.mock(`${renderer}/stores/browser-session-store`, () => ({
  useBrowserSessionStore: { getState: () => ({ createTab: () => {}, removeTab: () => {} }) }
}))
// Which session owns an agent browser tab, for the Stop indicator.
vi.mock(`${renderer}/stores/browser-control-store`, () => ({
  useBrowserControlStore: {
    getState: () => ({
      trackAgentTab: (tabId: string, sessionId: string) =>
        emit({ kind: 'trackAgentTab', tabId, sessionId }),
      untrackAgentTab: (tabId: string) => emit({ kind: 'untrackAgentTab', tabId })
    })
  }
}))
// The roster watch hands policy and automation hints to their own modules.
vi.mock(`${renderer}/lib/browser-policy-api`, async (importOriginal) => ({
  ...(await importOriginal<object>()),
  requestBrowserPolicyResync: () => emit({ kind: 'requestBrowserPolicyResync' }),
  publishBrowserPolicyChange: (payload: unknown) =>
    emit({ kind: 'publishBrowserPolicyChange', payload })
}))
vi.mock(`${renderer}/lib/automation-api`, async (importOriginal) => ({
  ...(await importOriginal<object>()),
  requestAutomationResync: () => emit({ kind: 'requestAutomationResync' }),
  publishAutomationChange: (payload: unknown) => emit({ kind: 'publishAutomationChange', payload })
}))
vi.mock(`${renderer}/lib/attachment-temp-cleanup`, async (importOriginal) => ({
  ...(await importOriginal<object>()),
  deleteSessionTempFiles: async (sessionId: string) =>
    emit({ kind: 'deleteSessionTempFiles', sessionId })
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
// An agent command answers the next `context.agentReplies` entry.
function agentCommand(effect: Record<string, unknown>): Promise<unknown> {
  emit(effect)
  return answer(agentReplies.shift() ?? null)
}
// `{ reject }` fails, `{ hold: true }` stays in flight for the rest of the
// vector, and `{ hold: true, release }` until a `releaseHeld` step answers it
// with `release`. Anything else is the reply.
function answer(reply: unknown): Promise<unknown> {
  if (typeof reply === 'object' && reply !== null) {
    if ('reject' in reply) return Promise.reject(new Error(String(reply.reject)))
    if ('hold' in reply) {
      if (!('release' in reply)) return new Promise(() => {})
      return new Promise((resolve, reject) =>
        heldReplies.push(() => answer(reply.release).then(resolve, reject))
      )
    }
  }
  return Promise.resolve(reply)
}
// An MCP command answers the next `context.mcpReplies` entry: `{ reject }`
// fails it (an `McpOAuthError` with `code`), `{ hold: true }` leaves it in
// flight, anything else is the reply. An OAuth start's `authUrl` reaches the
// paste-back callback first (with `hold`, the dialog stays), and a held start
// rejects as cancelled once its signal aborts, as on the web.
function mcpCommand(
  effect: Record<string, unknown>,
  options: { signal?: AbortSignal; onAuthUrl?: (url: string, redirect: string) => void } = {}
): Promise<never> {
  emit(effect)
  options.signal?.addEventListener('abort', () => emit({ kind: 'abortMcpOAuth', url: effect.url }))
  const reply = mcpReplies.shift() ?? null
  if (typeof reply === 'object' && reply !== null) {
    const fields = reply as Record<string, unknown>
    if (typeof fields.authUrl === 'string') {
      options.onAuthUrl?.(fields.authUrl, String(fields.redirectUri))
    }
    if ('reject' in fields) {
      const message = String(fields.reject)
      return Promise.reject(
        typeof fields.code === 'string'
          ? new McpOAuthError(message, fields.code)
          : new Error(message)
      )
    }
    if ('hold' in fields) {
      return new Promise((_resolve, reject) =>
        options.signal?.addEventListener('abort', () =>
          reject(new McpOAuthError('OAuth sign-in cancelled', 'OAUTH_CANCELLED'))
        )
      )
    }
  }
  return Promise.resolve(reply as never)
}
vi.mock(`${renderer}/lib/acp-auth-method-memory`, async (importOriginal) => ({
  ...(await importOriginal<object>()),
  saveAuthMethodMemory: async (methods: unknown) => emit({ kind: 'persistAuthMethods', methods })
}))
vi.mock(`${renderer}/lib/acp-api`, async (importOriginal) => {
  const actual = await importOriginal<{ acpApi: object }>()
  return {
    ...actual,
    answerMcpInstall: (request: unknown) => mcpCommand({ kind: 'answerMcpInstall', request }),
    acpApi: {
      ...actual.acpApi,
      sendPrompt: (agentId: string, sessionId: string, text: string, turnId: string) =>
        dispatchPrompt(agentId, sessionId, [{ type: 'text', text }], turnId),
      sendPromptBlocks: dispatchPrompt,
      spawnAgent: (config: unknown, projectId: unknown, sessionId?: string) =>
        agentCommand({
          kind: 'spawnAgent',
          config,
          projectId,
          ...(sessionId ? { sessionId } : {})
        }),
      killAgent: (agentId: string) => agentCommand({ kind: 'killAgent', agentId }),
      closeSession: (agentId: string, sessionId: string) =>
        agentCommand({ kind: 'closeAgentSession', agentId, sessionId }),
      deleteAgentSession: (agentId: string, sessionId: string) =>
        agentCommand({ kind: 'deleteAgentSession', agentId, sessionId }),
      probeMcpServer: (config: unknown) => mcpCommand({ kind: 'probeMcpServer', config }),
      startMcpOAuth: (
        projectId: string,
        url: string,
        options: { signal?: AbortSignal; onAuthUrl?: (url: string, redirect: string) => void }
      ) => mcpCommand({ kind: 'startMcpOAuth', projectId, url }, options),
      completeMcpOAuth: (projectId: string, callbackUrl: string) =>
        mcpCommand({ kind: 'completeMcpOAuth', projectId, callbackUrl }),
      hasMcpOAuthToken: (projectId: string, url: string) =>
        mcpCommand({ kind: 'hasMcpOAuthToken', projectId, url }),
      disconnectMcpOAuth: (projectId: string, url: string) =>
        mcpCommand({ kind: 'disconnectMcpOAuth', projectId, url }),
      authenticate: (agentId: string, methodId: string, input?: unknown) =>
        agentCommand({ kind: 'authenticate', agentId, methodId, ...(input ? { input } : {}) }),
      logout: (agentId: string) => agentCommand({ kind: 'logout', agentId }),
      respondElicitation: (agentId: string, requestId: string, action: string) =>
        agentCommand({ kind: 'respondElicitation', agentId, requestId, action }),
      cancelPrompt: async (agentId: string, sessionId: string) =>
        emit({ kind: 'cancelPrompt', agentId, sessionId }),
      setMode: async (agentId: string, sessionId: string, modeId: string) =>
        emit({ kind: 'setMode', agentId, sessionId, modeId }),
      setModel: async (agentId: string, sessionId: string, modelId: string) =>
        emit({ kind: 'setModel', agentId, sessionId, modelId }),
      setConfigOption: async (
        agentId: string,
        sessionId: string,
        configId: string,
        value: unknown
      ) => {
        emit({ kind: 'setConfigOption', agentId, sessionId, configId, value })
        const reply = configReplies.shift() ?? null
        if (typeof reply === 'object' && reply !== null && 'reject' in reply) {
          throw new Error(String(reply.reject))
        }
        return reply
      }
    }
  }
})

// The model list save and the read of the user's saved composer picks are
// the boundary of the options cache; the vector keeps both calls. The read
// answers `context.composerOptions`.
vi.mock(`${renderer}/stores/acp-store/model-catalog`, async (importOriginal) => ({
  ...(await importOriginal<object>()),
  persistModelCatalog: (configId: string, catalog: unknown) =>
    emit({ kind: 'persistModelCatalog', configId, catalog })
}))
vi.mock(`${renderer}/stores/acp-store/composer-options`, async (importOriginal) => ({
  ...(await importOriginal<object>()),
  persistComposerOptions: (configId: string, patch: unknown) =>
    emit({ kind: 'persistComposerOptions', configId, patch }),
  readComposerOptions: async (configId: string) => {
    emit({ kind: 'readComposerOptions', configId })
    return composerRecord.saved
  }
}))

const { McpOAuthError } = await import('../../../src/renderer/lib/acp-api')
const { acpRecorder, acpScenarios } = await import('./stores/acp')

const ACP_OUT = join(OUT_DIR, 'acp')
const write = process.env.RECORD_VECTORS === '1'

describe('acp test vectors', () => {
  it('match what the ACP chat store does today', async () => {
    vi.spyOn(globalThis.crypto, 'randomUUID').mockImplementation(drawId)
    const recorded: Vector[] = []
    for (const scenario of acpScenarios) recorded.push(await recordVector(acpRecorder, scenario))
    for (const file of ['parity-vectors.yml', 'rune.yml'] as const) {
      expect(sourcesOutsideWorkflow(acpRecorder.sources, file), `${file} paths`).toEqual([])
    }
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
