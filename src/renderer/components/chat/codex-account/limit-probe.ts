/**
 * The host moves a Codex chat that reaches its account's limit. This client
 * only marks a held chat as at its limit from the usage probe, so the limit
 * notice shows before the agent reports it.
 */

import type { ClaudeRateLimit } from '@shared/types/claude-accounts.types'
import type { CodexUsageWindow } from '@shared/types/codex-accounts.types'
import { useEffect } from 'react'
import { codexAccountsApi } from '@/lib/codex-accounts-api'
import { codexResetSeconds, primaryCodexWindow } from '@/lib/codex-usage-format'
import { logFrontendError } from '@/lib/log-api'
import { type AcpState, useAcpStore } from '@/stores/acp-store'
import { withRateLimit } from '@/stores/acp-store/rate-limit'
import { isCodexSession, runsProjectAccount } from '@/stores/codex-account-switch'
import { ensureCodexResumeTurnWatch } from './use-codex-limit'
import { codexAccountFor } from './use-project-codex-account'

const PROBE_STAMP_MS = 20_000

/** A window whose reset time is still ahead. A missing time stays usable. */
function windowStillOpen(window: CodexUsageWindow, now = Date.now()): boolean {
  const resetAt = codexResetSeconds(window)
  return resetAt === null || resetAt * 1000 > now
}

/**
 * A probe at 100% is the same limit as a mid-turn `rate_limited` /
 * `quota_exhausted` error. The reset time is the probe's.
 */
async function stampProbeLimits(): Promise<void> {
  const [snapshot, usage] = await Promise.all([
    codexAccountsApi.list(),
    codexAccountsApi.usage(false)
  ])
  const reads = Object.fromEntries(usage.map((read) => [read.accountId, read]))
  const state = useAcpStore.getState()
  let next: AcpState['sessionUsage'] | null = null
  for (const [sessionId, session] of Object.entries(state.sessions)) {
    if (!session.projectId || !isCodexSession(state, sessionId)) continue
    if (state.sessionUsage[sessionId]?.rateLimit?.status === 'rejected') continue
    // A finished chat stays on screen. Only a turn that is still held is marked.
    const held = session.activeTurn === true || session.openTurnId != null
    if (!held || !runsProjectAccount(state, sessionId)) continue
    // The chat's own account, else the project's. A pin is not the project account.
    const accountId = codexAccountFor(snapshot, session.projectId, sessionId)
    const window = primaryCodexWindow(reads[accountId])
    if (!window || window.percent < 100 || !windowStillOpen(window)) continue
    const limit: ClaudeRateLimit = {
      status: 'rejected',
      resetsAt: codexResetSeconds(window),
      kind: window.kind === 'weekly' ? 'seven_day' : 'five_hour',
      percent: 100,
      overageInUse: false
    }
    next = {
      ...(next ?? state.sessionUsage),
      [sessionId]: withRateLimit(state.sessionUsage[sessionId], limit)
    }
  }
  if (next) useAcpStore.setState({ sessionUsage: next })
}

/** Stamp probe limits now and every 20 s. Returns the stop function. */
export function startCodexLimitProbe(): () => void {
  ensureCodexResumeTurnWatch()
  void stampProbeLimits().catch((error) => {
    void logFrontendError({
      level: 'warn',
      source: 'codexLimitProbe',
      message: `Could not read Codex usage: ${error instanceof Error ? error.message : String(error)}`
    })
  })
  const timer = setInterval(() => {
    void stampProbeLimits().catch(() => undefined)
  }, PROBE_STAMP_MS)
  return () => clearInterval(timer)
}

/** Mount once per renderer root. */
export function useCodexLimitProbe(): void {
  useEffect(() => startCodexLimitProbe(), [])
}
