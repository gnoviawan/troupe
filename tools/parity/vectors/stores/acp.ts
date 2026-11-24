import { vi } from 'vitest'
import {
  _setAcpTransportForTests,
  type AcpTransport
} from '../../../../src/renderer/lib/acp-transport'
import {
  _flushCoalescedForTesting,
  _resetCoalesceForTesting,
  _resetHistorySeqWatermarksForTesting,
  type AcpState,
  useAcpStore
} from '../../../../src/renderer/stores/acp-store'
import { _resetComposerSessionsForTesting } from '../../../../src/renderer/stores/acp-store/composer-options'
import { forgetHostReports } from '../../../../src/renderer/stores/acp-store/host-session-reports'
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
import {
  agentReplies,
  composerRecord,
  configReplies,
  dispatchOutcomes,
  emit,
  openChatTabs,
  type StoreRecorder
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

/** Module functions a step may call besides the store's actions. */
const MODULE_CALLS: Record<string, (...args: never[]) => void> = {
  noteHistoryWatermark: shared.noteHistoryWatermark,
  noteLiveTurnReplayFloor: shared.noteLiveTurnReplayFloor
}

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
    'src/renderer/components/chat/subagent-strip.ts',
    'src/renderer/components/chat/tool-call-summary.ts',
    'src/renderer/lib/agents/acp-spawn-errors.ts',
    'src/renderer/lib/acp-api.ts',
    'src/shared/types/web-protocol.types.ts',
    'src/renderer/stores/acp-store/composer-options.ts',
    'src/renderer/stores/acp-store/chat-tab-prune.ts',
    'src/renderer/lib/acp-history-persistence.ts',
    'src/renderer/stores/acp-store/session-agent-config.ts',
    'src/shared/types/antigravity-accounts.types.ts'
  ],
  reset(context) {
    const state = useAcpStore.getState()
    for (const sessionId of new Set([...SCENARIO_SESSIONS, ...Object.keys(state.sessions)])) {
      forgetHostReports(sessionId)
    }
    // Forgets each session's clamp-log ids, so a vector logs its own clamps.
    for (const sessionId of Object.keys(state.toolCalls)) {
      dropSessionTranscriptState(state, sessionId)
    }
    _resetCoalesceForTesting()
    _resetHistorySeqWatermarksForTesting()
    _resetSubagentRecordLogsForTesting()
    _resetSubagentTranscriptLogsForTesting()
    shared._resetPromptReconcileForTesting()
    shared._resetBusyQueueRetriesForTesting()
    dispatchOutcomes.splice(0, dispatchOutcomes.length, ...((context.dispatch as unknown[]) ?? []))
    configReplies.splice(0, configReplies.length, ...((context.configReplies as unknown[]) ?? []))
    composerRecord.saved = context.composerOptions ?? null
    agentReplies.splice(0, agentReplies.length, ...((context.agentReplies as unknown[]) ?? []))
    _resetAcpAuthForTesting()
    _resetComposerSessionsForTesting()
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
    useAcpStore.setState({ ...FRESH, pendingBrowserConsents: {} } as Partial<AcpState>)
  },
  seed(initial) {
    useAcpStore.setState(initial as Partial<AcpState>)
  },
  snapshot() {
    const state = useAcpStore.getState()
    return {
      ...Object.fromEntries(DATA_KEYS.map((key) => [key, state[key]])),
      book: {
        seq: peekSeq(),
        historyWatermarks: Object.fromEntries(shared.historySeqWatermarks),
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
      if (result instanceof Promise) result.catch(rejected)
    } catch (error) {
      rejected(error)
    }
    _flushCoalescedForTesting()
    // The deferred turn end (`scheduleTurnEnd`) runs on the next macrotask.
    await flushTurnEnd()
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
import { controlScenarios } from './acp-scenarios-controls'
import { indexScenarios } from './acp-scenarios-index'
import { pickScenarios } from './acp-scenarios-picks'
import { promptScenarios } from './acp-scenarios-prompt'
import { restoreScenarios } from './acp-scenarios-restore'
import { sessionScenarios } from './acp-scenarios-session'
import { subagentScenarios } from './acp-scenarios-subagents'
import { turnScenarios } from './acp-scenarios-turn'

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
  ...agentCommandScenarios
]
