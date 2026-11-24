import { vi } from 'vitest'
import {
  _resetAcpTransportForTests,
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
import * as shared from '../../../../src/renderer/stores/acp-store/shared-state'
import { dropSessionTranscriptState } from '../../../../src/renderer/stores/acp-store/slices/transcript'
import { _resetSubagentRecordLogsForTesting } from '../../../../src/renderer/stores/acp-store/subagent-lifecycle'
import { _resetSubagentTranscriptLogsForTesting } from '../../../../src/renderer/stores/acp-store/subagent-transcript'
import { FRESH } from '../../../../src/renderer/stores/acp-store/testkit'
import type { StoreRecorder } from '../record'

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
  'pendingBrowserConsents'
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
    'src/renderer/lib/browser-consent-api.ts'
  ],
  reset(context) {
    const state = useAcpStore.getState()
    // Forgets each session's clamp-log ids, so a vector logs its own clamps.
    for (const sessionId of Object.keys(state.toolCalls)) {
      dropSessionTranscriptState(state, sessionId)
    }
    _resetCoalesceForTesting()
    _resetHistorySeqWatermarksForTesting()
    _resetSubagentRecordLogsForTesting()
    _resetSubagentTranscriptLogsForTesting()
    // The counter only moves up; each vector starts on the next hundred.
    shared.rebaseSeqCounter(Math.ceil((peekSeq() + 1) / 100) * 100)
    if (context.serverHistory) {
      _setAcpTransportForTests({
        historyMode: () => 'server',
        dispose: () => {}
      } as unknown as AcpTransport)
    } else {
      _resetAcpTransportForTests(null)
    }
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
        historyWatermarks: Object.fromEntries(shared.historySeqWatermarks)
      }
    }
  },
  dispatch({ action, args }) {
    clock += 1000
    const call =
      MODULE_CALLS[action] ??
      (useAcpStore.getState() as unknown as Record<string, (...a: unknown[]) => void>)[action]
    ;(call as (...a: unknown[]) => void)(...args)
    _flushCoalescedForTesting()
  }
}

/** The live `seqCounter`, through the hook `record-acp.test.ts` mocks in. */
function peekSeq(): number {
  return (shared as unknown as { peekSeqCounter: () => number }).peekSeqCounter()
}

import { transcriptScenarios } from './acp-scenarios'
import { subagentScenarios } from './acp-scenarios-subagents'

export const acpScenarios = [...transcriptScenarios, ...subagentScenarios]
