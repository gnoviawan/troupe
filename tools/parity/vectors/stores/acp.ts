import { vi } from 'vitest'
import { _clearPayloadCacheForTesting } from '../../../../src/renderer/lib/acp-history-persistence'
import {
  _setAcpTransportForTests,
  type AcpTransport
} from '../../../../src/renderer/lib/acp-transport'
import { __resetCommandEngineForTesting } from '../../../../src/renderer/lib/command-engine'
import {
  _flushCoalescedForTesting,
  _resetCoalesceForTesting,
  _resetHistorySeqWatermarksForTesting,
  _resetInFlightHistoryOpensForTesting,
  type AcpState,
  useAcpStore
} from '../../../../src/renderer/stores/acp-store'
import { attachAgentRosterWatch } from '../../../../src/renderer/stores/acp-store/agent-roster-watch'
import { _resetComposerSessionsForTesting } from '../../../../src/renderer/stores/acp-store/composer-options'
import { forgetHostReports } from '../../../../src/renderer/stores/acp-store/host-session-reports'
import { _resetRegistryMcpFingerprintsForTesting } from '../../../../src/renderer/stores/acp-store/registry-mcp-fingerprints'
import * as shared from '../../../../src/renderer/stores/acp-store/shared-state'
import {
  _resetAcpAuthForTesting,
  authenticatedAgents,
  rememberedAuthMethods
} from '../../../../src/renderer/stores/acp-store/slices/agent'
import { dropSessionTranscriptState } from '../../../../src/renderer/stores/acp-store/slices/transcript'
import { _resetSubagentRecordLogsForTesting } from '../../../../src/renderer/stores/acp-store/subagent-lifecycle'
import { _resetSubagentTranscriptLogsForTesting } from '../../../../src/renderer/stores/acp-store/subagent-transcript'
import { FRESH, flushTurnEnd } from '../../../../src/renderer/stores/acp-store/testkit'
import { useProjectStore } from '../../../../src/renderer/stores/project-store'
import {
  agentReplies,
  composerRecord,
  configReplies,
  dispatchOutcomes,
  emit,
  factoryKey,
  heldReplies,
  mcpReplies,
  mcpSharedSync,
  openChatTabs,
  platform,
  type StoreRecorder,
  sessionPayloads,
  setReplies
} from '../record'

/**
 * Wall clock of a vector: `context.now` (default {@link NOW}) while seeding,
 * then 1 s later at every step. `tests/acp_vectors.rs` steps the same clock.
 */
const NOW = 1_700_000_000_000
let clock = NOW
vi.spyOn(Date, 'now').mockImplementation(() => clock)

/** The data fields the model ports, and the module-level maps beside them. */
const DATA_KEYS = [
  'sessions',
  'messages',
  'toolCalls',
  'plans',
  'commands',
  'commandsSeq',
  'sessionUsage',
  'sessionIndex',
  'pendingPermissions',
  'pendingQuestions',
  'pendingElicitations',
  'pendingBrowserConsents',
  'agentStatus',
  'turnEndNotices',
  'promptQueues',
  'suppressQueueFlush',
  'agentOptionsCache',
  'configToLiveAgent',
  'activeSessionId',
  'preparedSessions',
  'agentSwitches',
  'agents',
  'pendingSignIn',
  'autoOpenBrowserUrl',
  'launchingSessionIds',
  'discoveredSessions'
] as const

/** The host's `agents` watch, as `attachAgentRosterWatch` subscribes to it. */
const agentsWatch: { onState?: (state: unknown) => void; onDelta?: (delta: unknown) => void } = {}
attachAgentRosterWatch(
  {
    subscribe: (_topic: string, handlers: typeof agentsWatch) => {
      Object.assign(agentsWatch, handlers)
      return () => {}
    }
  } as unknown as Parameters<typeof attachAgentRosterWatch>[0],
  () => useAcpStore.getState()
)

/** The MCP and launch fields, snapshotted only once a vector set one. */
const MCP_KEYS = [
  'agentConfigs',
  'selectedAgentConfigId',
  'preparingChatKeys',
  'prepareChatErrors',
  'promptCommandStates',
  'warmingConfigs',
  'discoveringKeys',
  'mcpServers',
  'mcpServersLoaded',
  'pendingMcpInstalls',
  'mcpProbeStatus',
  'mcpOAuthConnecting',
  'mcpOAuthConnected',
  'mcpOAuthPasteBack',
  'mcpTools',
  'mcpToolsLoaded',
  'mcpProbing',
  'mcpProbeError'
] as const

const MCP_FRESH = {
  agentConfigs: [],
  selectedAgentConfigId: null,
  preparingChatKeys: {},
  prepareChatErrors: {},
  promptCommandStates: {},
  warmingConfigs: {},
  discoveringKeys: {},
  mcpServers: [],
  mcpServersLoaded: false,
  pendingMcpInstalls: {},
  mcpProbeStatus: {},
  mcpOAuthConnecting: {},
  mcpOAuthConnected: {},
  mcpOAuthPasteBack: {},
  mcpTools: {},
  mcpToolsLoaded: {},
  mcpProbing: {},
  mcpProbeError: {}
}

/** `value` as JSON keeps it, when that is not empty, null, `false` or `[]`. */
function setField(value: unknown): unknown {
  const plain = value === undefined ? undefined : JSON.parse(JSON.stringify(value))
  if (plain === false || plain === undefined || plain === null) return undefined
  if (typeof plain === 'object' && plain !== null && Object.keys(plain).length === 0)
    return undefined
  return plain
}

/** Module functions a step may call besides the store's actions. */
const MODULE_CALLS: Record<string, (...args: never[]) => void> = {
  noteHistoryWatermark: shared.noteHistoryWatermark,
  noteLiveTurnReplayFloor: shared.noteLiveTurnReplayFloor,
  mcpSharedChanged: (change: unknown) => mcpSharedSync.apply?.(change),
  agentsWatchState: (state: unknown) => agentsWatch.onState?.(state),
  agentsWatchDelta: (delta: unknown) => agentsWatch.onDelta?.(delta),
  renewAgentCredentials: shared.renewAgentCredentials,
  setProjects: (projects: unknown) =>
    useProjectStore.setState({ projects } as Parameters<typeof useProjectStore.setState>[0]),
  agentConfigsChanged: (agentConfigs: unknown) =>
    useAcpStore.setState({ agentConfigs } as Partial<AcpState>),
  releaseHeld: () => heldReplies.shift()?.(),
  // The import's progress callback is the model's `importProgress` effect,
  // and its stop poll reads what a `stopImport` step set.
  importAgentSessions: (request: object) =>
    useAcpStore.getState().importAgentSessions({
      ...(request as Parameters<AcpState['importAgentSessions']>[0]),
      onProgress: (done, outcome) => emit({ kind: 'importProgress', done, outcome }),
      shouldStop: () => importStop.requested
    }),
  stopImport: () => {
    importStop.requested = true
  }
}

/** A `stopImport` step asked the running import to stop. */
const importStop = { requested: false }

/** Actions that return their answer at once: it is the `actionResolved` effect. */
const SYNC_RESOLVES = new Set(['claimPreparedChat'])

/** Actions whose answer the caller reads: it is the `actionResolved` effect. */
const RESOLVES = new Set([
  'listImportableSessions',
  'importAgentSessions',
  'createSession',
  'openHistorySession',
  'generateCommitMessage',
  'assistTerminal',
  'draftBugReport'
])

export const acpRecorder: StoreRecorder = {
  store: 'acp',
  sources: [
    'src/renderer/stores/acp-store/slices/transcript.ts',
    'src/renderer/stores/acp-store/helpers.ts',
    'src/renderer/stores/acp-store/shared-state.ts',
    'src/renderer/stores/acp-store/rate-limit.ts',
    'src/renderer/stores/prompt-queue-orchestration.ts',
    'src/renderer/lib/plan-session.ts',
    'src/renderer/lib/log-point.ts',
    'src/renderer/lib/github-handoff/read-only-guard.ts',
    'src/renderer/components/chat/terminal-output.ts',
    'src/renderer/components/chat/handoff-summary.ts',
    'src/renderer/stores/acp-store/subagent-lifecycle.ts',
    'src/renderer/stores/acp-store/subagent-transcript.ts',
    'src/renderer/stores/acp-store/child-approvals.ts',
    'src/renderer/stores/acp-store/slices/prompt.ts',
    'src/renderer/stores/acp-store/slices/agent.ts',
    'src/renderer/lib/browser-consent-api.ts',
    'src/renderer/stores/acp-store/host-session-reports.ts',
    'src/renderer/lib/agent-chat-notify.ts',
    'src/renderer/components/chat/background-commands.ts',
    'src/renderer/components/chat/chat-input-bar-config.ts',
    'src/renderer/components/chat/agent-model-selector/selector-model.ts',
    'src/renderer/stores/acp-store/model-catalog.ts',
    'src/renderer/stores/acp-reuse-keys.ts',
    'src/renderer/lib/command-engine.ts',
    'src/renderer/stores/acp-store/slices/session.ts',
    'src/renderer/stores/acp-store/slices/import-sessions.ts',
    'src/renderer/components/chat/subagent-strip.ts',
    'src/renderer/components/chat/tool-call-summary.ts',
    'src/renderer/lib/agents/acp-spawn-errors.ts',
    'src/renderer/lib/acp-api.ts',
    'src/shared/types/web-protocol.types.ts',
    'src/renderer/stores/acp-store/composer-options.ts',
    'src/renderer/stores/acp-store/chat-tab-prune.ts',
    'src/renderer/lib/acp-history-persistence.ts',
    'src/renderer/stores/acp-store/agent-roster-watch.ts',
    'src/renderer/stores/acp-store/browser-agent-tabs.ts',
    'src/renderer/stores/acp-store/session-agent-config.ts',
    'src/shared/types/antigravity-accounts.types.ts',
    'src/renderer/stores/acp-store/slices/mcp.ts',
    'src/renderer/lib/acp-mcp-persistence.ts',
    'src/renderer/lib/mcp-install-request.ts',
    'src/renderer/stores/acp-store/slices/launch.ts',
    'src/renderer/stores/acp-store/slices/config.ts',
    'src/renderer/lib/env-parser.ts',
    'src/renderer/lib/agents/agent-aliases.ts',
    'src/shared/acp-agent-aliases.json',
    'src/renderer/lib/acp-transport/tauri-verdicts.ts',
    'src/renderer/lib/agents/acp-registry.ts',
    'src/renderer/stores/acp-store/slices/misc.ts',
    'src/renderer/stores/acp-store/slices/switch.ts',
    'src/renderer/stores/acp-store/registry-mcp-fingerprints.ts',
    'src/renderer/stores/acp-store/ephemeral-ids.ts',
    'src/renderer/stores/acp-store/slices/prompt-reconcile.ts'
  ],
  reset(context) {
    const state = useAcpStore.getState()
    for (const sessionId of new Set([...SCENARIO_SESSIONS, ...Object.keys(state.sessions)])) {
      forgetHostReports(sessionId)
    }
    // Forgets each session's clamp-log ids and window bookkeeping, so a
    // vector logs its own clamps and probes its own history.
    for (const sessionId of new Set([
      ...Object.keys(state.toolCalls),
      ...Object.keys(state.messages)
    ])) {
      dropSessionTranscriptState(state, sessionId)
    }
    _clearPayloadCacheForTesting()
    sessionPayloads.clear()
    for (const [id, payload] of Object.entries(
      (context.payloads as Record<string, unknown>) ?? {}
    )) {
      sessionPayloads.set(id, payload)
    }
    _resetCoalesceForTesting()
    _resetHistorySeqWatermarksForTesting()
    // A vector may leave a history open in flight; the next one starts clean.
    _resetInFlightHistoryOpensForTesting()
    _resetSubagentRecordLogsForTesting()
    _resetSubagentTranscriptLogsForTesting()
    shared._resetPromptReconcileForTesting()
    shared._resetBusyQueueRetriesForTesting()
    shared._resetDefiniteSpawnRejectionsForTesting()
    shared._resetInFlightPreparedForTesting()
    shared._resetEphemeralSessionIdsForTesting()
    __resetCommandEngineForTesting()
    shared._resetInFlightPromotionsForTesting()
    _resetRegistryMcpFingerprintsForTesting()
    shared.inFlightWarms.clear()
    useProjectStore.setState({
      projects: (context.projects as unknown[]) ?? []
    } as Parameters<typeof useProjectStore.setState>[0])
    dispatchOutcomes.splice(0, dispatchOutcomes.length, ...((context.dispatch as unknown[]) ?? []))
    configReplies.splice(0, configReplies.length, ...((context.configReplies as unknown[]) ?? []))
    composerRecord.saved = context.composerOptions ?? null
    agentReplies.splice(0, agentReplies.length, ...((context.agentReplies as unknown[]) ?? []))
    mcpReplies.splice(0, mcpReplies.length, ...((context.mcpReplies as unknown[]) ?? []))
    heldReplies.length = 0
    importStop.requested = false
    setReplies.splice(0, setReplies.length, ...((context.setReplies as unknown[]) ?? []))
    factoryKey.stored = context.factoryKey ?? false
    _resetAcpAuthForTesting()
    _resetComposerSessionsForTesting()
    platform.desktop = context.desktop === true
    openChatTabs.clear()
    for (const id of (context.openChatTabs as string[]) ?? []) openChatTabs.add(id)
    // The counter only moves up; each vector starts on the next hundred.
    shared.rebaseSeqCounter(Math.ceil((peekSeq() + 1) / 100) * 100)
    // The desktop keeps history in its own store and reloads the index after
    // a lifecycle event; the web client's host owns it (`serverHistory`).
    _setAcpTransportForTests({
      historyMode: () => (context.serverHistory ? 'server' : 'tauri_store'),
      dispose: () => {}
    } as unknown as AcpTransport)
    clock = typeof context.now === 'number' ? context.now : NOW
    useAcpStore.setState({
      ...FRESH,
      ...MCP_FRESH,
      pendingBrowserConsents: {}
    } as Partial<AcpState>)
  },
  seed(initial) {
    useAcpStore.setState(initial as Partial<AcpState>)
  },
  snapshot() {
    const state = useAcpStore.getState()
    return {
      ...Object.fromEntries(DATA_KEYS.map((key) => [key, state[key]])),
      ...Object.fromEntries(
        MCP_KEYS.map((key) => [key, setField(state[key])]).filter(
          ([, value]) => value !== undefined
        )
      ),
      book: {
        seq: peekSeq(),
        historyWatermarks: Object.fromEntries(shared.historySeqWatermarks),
        // The store starts the shared registry watch once per page.
        ...(mcpSharedSync.apply ? { mcpSharedSync: true } : {}),
        // The auth bookkeeping, only once a vector touched it.
        ...(authenticatedAgents.size > 0
          ? { authenticatedAgents: [...authenticatedAgents].sort() }
          : {}),
        ...(rememberedAuthMethods.size > 0
          ? { rememberedAuthMethods: Object.fromEntries([...rememberedAuthMethods].sort()) }
          : {})
      }
    }
  },
  async dispatch({ action, args }) {
    clock += 1000
    const call =
      MODULE_CALLS[action] ??
      (useAcpStore.getState() as unknown as Record<string, (...a: unknown[]) => void>)[action]
    // A step does not wait for an action to finish: a send-now that stops a
    // turn resumes in the step that ends it, as the app sees it.
    const rejected = (error: unknown) => emit({ kind: 'actionRejected', message: String(error) })
    try {
      const result = (call as (...a: unknown[]) => unknown)(...args)
      if (SYNC_RESOLVES.has(action)) emit({ kind: 'actionResolved', value: result ?? null })
      if (result instanceof Promise) {
        result.then(
          (value: unknown) =>
            RESOLVES.has(action) && emit({ kind: 'actionResolved', value: value ?? null }),
          rejected
        )
      }
    } catch (error) {
      rejected(error)
    }
    _flushCoalescedForTesting()
    // The deferred turn end (`scheduleTurnEnd`) runs on the next macrotask.
    // Stop on a prompt still waiting to be confirmed schedules it only once
    // the cancel command answered: one macrotask more.
    await flushTurnEnd()
    if (action === 'cancelPrompt') await flushTurnEnd()
  }
}

/** Sessions the scenarios name, whose host reports outlive a vector. */
const SCENARIO_SESSIONS = ['s1', 's9']

/** The live `seqCounter`, through the hook `record-acp.test.ts` mocks in. */
function peekSeq(): number {
  return (shared as unknown as { peekSeqCounter: () => number }).peekSeqCounter()
}

import { transcriptScenarios } from './acp-scenarios'
import { agentCommandScenarios } from './acp-scenarios-agent-commands'
import { agentScenarios } from './acp-scenarios-agents'
import { authRetryScenarios } from './acp-scenarios-auth-retry'
import { closeScenarios } from './acp-scenarios-close'
import { controlScenarios } from './acp-scenarios-controls'
import { createScenarios } from './acp-scenarios-create'
import { discoveryScenarios } from './acp-scenarios-discovery'
import { ensureScenarios } from './acp-scenarios-ensure'
import { historyScenarios } from './acp-scenarios-history'
import { importScenarios } from './acp-scenarios-import'
import { indexScenarios } from './acp-scenarios-index'
import { launchScenarios } from './acp-scenarios-launch'
import { mcpScenarios } from './acp-scenarios-mcp'
import { oneShotScenarios } from './acp-scenarios-oneshot'
import { pickScenarios } from './acp-scenarios-picks'
import { promptScenarios } from './acp-scenarios-prompt'
import { reconcileScenarios } from './acp-scenarios-reconcile'
import { restoreScenarios } from './acp-scenarios-restore'
import { rosterScenarios } from './acp-scenarios-roster'
import { sessionScenarios } from './acp-scenarios-session'
import { subagentScenarios } from './acp-scenarios-subagents'
import { switchScenarios } from './acp-scenarios-switch'
import { turnScenarios } from './acp-scenarios-turn'
import { warmScenarios } from './acp-scenarios-warm'
import { windowScenarios } from './acp-scenarios-window'

export const acpScenarios = [
  ...transcriptScenarios,
  ...subagentScenarios,
  ...turnScenarios,
  ...controlScenarios,
  ...promptScenarios,
  ...sessionScenarios,
  ...agentScenarios,
  ...pickScenarios,
  ...indexScenarios,
  ...restoreScenarios,
  ...agentCommandScenarios,
  ...rosterScenarios,
  ...windowScenarios,
  ...mcpScenarios,
  ...launchScenarios,
  ...closeScenarios,
  ...ensureScenarios,
  ...authRetryScenarios,
  ...discoveryScenarios,
  ...importScenarios,
  ...createScenarios,
  ...historyScenarios,
  ...oneShotScenarios,
  ...switchScenarios,
  ...warmScenarios,
  ...reconcileScenarios
]
