import { useEffect, useRef } from 'react'
import { getAcpTransport } from '@/lib/acp-transport'
import { logFrontendError } from '@/lib/log-api'
import {
  getOrCreateProjectContinuityCorrelation,
  recordTerminalContinuityEvent
} from '@/lib/terminal-continuity-instrumentation'
import { type AcpState, useAcpStore } from '@/stores/acp-store'
import { isLaunchPlaceholderSessionId } from '@/stores/acp-store/live-turn'
import { useProjectStore } from '@/stores/project-store'

/**
 * True while a chat of `projectId` is starting on a launch placeholder. Its
 * real session can reach the index (the host's history update) before
 * `createSession` puts the live record in the store, so in that window the
 * new chat looks like a persisted session to resume (#1941).
 */
export function projectChatLaunchInFlight(
  state: Pick<AcpState, 'launchingSessionIds' | 'sessions'>,
  projectId: string | null | undefined
): boolean {
  if (!projectId) return false
  return Object.keys(state.launchingSessionIds ?? {}).some(
    (id) => isLaunchPlaceholderSessionId(id) && state.sessions[id]?.projectId === projectId
  )
}

/**
 * R1: proactively reattach still-running ACP agent sessions after a refresh
 * (F5 / webview reload / phone browser reload). Mirrors Paseo's
 * daemon-owns-resume: the Rust `AcpManager` keeps the live agent across the
 * renderer reload, so on bootstrap we resume each eligible persisted session
 * from the server-authoritative state and let the missed gap events replay
 * before live streaming continues — no manual session reopen.
 *
 * Runs AFTER `useAcpHistory`'s `loadSessionIndex()` populates the index. For
 * each resume-eligible session scoped to the active project: seed the WS
 * transport's `lastSeq` from the server watermark (R2 — web only), then call
 * `openHistorySession` — the same code path as click-to-open, which remaps
 * stale agent IDs and picks the appropriate reopen strategy. When the remap
 * fails or the strategy lands at read-only local, the session ends up `closed`
 * and the "Chat disconnected (read-only)." banner surfaces — never cold-spawns,
 * never throws on the bootstrap path. Desktop: `session/load` replay covers
 * the gap (no WS cursor needed).
 *
 * Best-effort + idempotent: a ref guards against re-attempting a session whose
 * index identity churns (a resumed session re-persists + re-renders the row),
 * and an already-open session is skipped (mirroring `openHistorySession`).
 */
export function useAcpSessionResume(): void {
  const sessionIndex = useAcpStore((s) => s.sessionIndex)
  const activeProjectId = useProjectStore((s) => s.activeProjectId)
  const launchInFlight = useAcpStore((s) => projectChatLaunchInFlight(s, activeProjectId))
  const attemptedRef = useRef<Set<string>>(new Set())

  useEffect(() => {
    if (!activeProjectId) return
    // The index is populated by `useAcpHistory` on mount; if it is empty here
    // (cold start with no history), there is nothing to resume yet — the
    // effect re-runs when `loadSessionIndex` populates it.
    if (sessionIndex.length === 0) return
    // A chat of this project is starting: its new session may already be in
    // the index without its live record. Resuming it would reopen the chat
    // under the starting one (a restore over its panel that drops the unsent
    // draft, #1941). Wait: the effect runs again when the launch settles, and
    // by then the new session is live and skipped below.
    if (launchInFlight) {
      void logFrontendError({
        level: 'info',
        source: 'acp.resumeBootstrap',
        message: `acp-resume deferred for project ${activeProjectId}: a chat launch is in flight`
      })
      return
    }
    let cancelled = false
    void (async () => {
      const correlationId = getOrCreateProjectContinuityCorrelation(activeProjectId)
      const transport = getAcpTransport()
      const { openHistorySession } = useAcpStore.getState()
      // Eligible: belongs to this project, advertises an agent (resumeEligible
      // = agentConfigId || agentId), has the authoritative agent id needed to
      // resume, and is neither closed nor errored (closed chats re-open
      // lazily on click via `openHistorySession`; 'error' is a persisted
      // dead-turn state — an open restored tab self-loads through
      // AgentChatPanel's mount effect, so resuming it here would only
      // cold-spawn an agent for every crashed chat in history on each
      // reload, tab or no tab).
      const eligible = sessionIndex.filter(
        (entry) =>
          entry.projectId === activeProjectId &&
          (entry.agentConfigId || entry.agentId) &&
          entry.agentId &&
          entry.status !== 'closed' &&
          entry.status !== 'error'
      )
      for (const entry of eligible) {
        if (cancelled) return
        if (attemptedRef.current.has(entry.id)) continue
        // Skip a session already open in this lifetime (e.g. a chat the user
        // just started) — mirroring `openHistorySession`'s cached early-return.
        // Read the store now, not before the loop: an earlier entry's awaits
        // can outlast a launch that made this session live meanwhile.
        const live = useAcpStore.getState()
        if (projectChatLaunchInFlight(live, activeProjectId)) return
        const cached = live.sessions[entry.id]
        if (cached && cached.status !== 'closed') continue
        recordTerminalContinuityEvent({
          name: 'acp-resume-attempted',
          correlationId,
          projectId: activeProjectId,
          terminalId: entry.id,
          details: { agentId: entry.agentId, cwd: entry.cwd }
        })
        try {
          // R2/R3 (web): seed the fresh transport's `lastSeq` from the
          // server-authoritative watermark BEFORE resume, so the built-in
          // re-subscribe replays only the reload gap (not the whole log).
          // Desktop has no WS cursor (`session/load` replays) → no-op there.
          if (transport.fetchSessionCursor) {
            const cursor = await transport.fetchSessionCursor(entry.id)
            // A rerun (a launch starting, a new index) cancelled this attempt
            // before it opened anything: the chat stays eligible for it.
            if (cancelled) return
            transport.seedSessionCursor?.(entry.id, cursor)
          }
          // Marked only now, after the last cancellation point before the
          // open: an attempt interrupted above must not keep the chat from
          // ever resuming. Only one run is uncancelled at a time, so this
          // still stops a second open of the same chat.
          if (attemptedRef.current.has(entry.id)) continue
          attemptedRef.current.add(entry.id)
          await openHistorySession(entry.id)
          if (cancelled) return
          // 'local' strategy resolves without throwing but leaves the session
          // closed (read-only) — record a skip, not a false-positive success.
          const resumed = useAcpStore.getState().sessions[entry.id]
          if (!resumed || resumed.status === 'closed' || resumed.status === 'error') {
            recordTerminalContinuityEvent({
              name: 'acp-resume-skipped',
              correlationId,
              projectId: activeProjectId,
              terminalId: entry.id,
              details: { reason: 'read-only (local strategy)' }
            })
          } else {
            recordTerminalContinuityEvent({
              name: 'acp-resume-succeeded',
              correlationId,
              projectId: activeProjectId,
              terminalId: entry.id
            })
          }
        } catch (err) {
          if (cancelled) return
          // A failed cursor fetch counts as an attempt too: no retry loop.
          attemptedRef.current.add(entry.id)
          // `openHistorySession` rejected (spawn failed, or resume/load threw
          // after the remap) → read-only local. Degrades safely; never throws
          // on the bootstrap path. A load-capable (not resume-capable) agent
          // also lands here — the user's click re-opens it via
          // `openHistorySession` (the capability gate is honored either way).
          recordTerminalContinuityEvent({
            name: 'acp-resume-skipped',
            correlationId,
            projectId: activeProjectId,
            terminalId: entry.id,
            details: { reason: String(err) }
          })
          void logFrontendError({
            level: 'warn',
            source: 'acp.resumeBootstrap',
            message: `acp-resume skipped for ${entry.id}: ${String(err)}`
          })
        }
      }
    })()
    return () => {
      cancelled = true
    }
  }, [sessionIndex, activeProjectId, launchInFlight])
}
