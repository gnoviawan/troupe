/**
 * The host moves an Antigravity chat that reaches its account's limit. This
 * client only marks a held chat as at its limit from the usage probe, so the
 * limit notice shows before the agent reports it.
 */

import type { ClaudeRateLimit } from '@shared/types/claude-accounts.types'
import { useEffect } from 'react'
import { antigravityAccountsApi } from '@/lib/antigravity-accounts-api'
import {
  antigravityResetSeconds,
  antigravityWindowOpen,
  primaryAntigravityWindow
} from '@/lib/antigravity-usage-format'
import { logFrontendError } from '@/lib/log-api'
import { type AcpState, useAcpStore } from '@/stores/acp-store'
import { withRateLimit } from '@/stores/acp-store/rate-limit'
import { isAntigravitySession, runsProjectAccount } from '@/stores/antigravity-account-switch'
import { ensureAntigravityResumeTurnWatch } from './use-antigravity-limit'
import { antigravityAccountFor } from './use-project-antigravity-account'

const PROBE_STAMP_MS = 20_000

/**
 * A probe whose most used model is at 100% is the same limit as a mid-turn
 * `quota_exhausted` error. The reset time is the probe's.
 */
async function stampProbeLimits(): Promise<void> {
  const [snapshot, usage] = await Promise.all([
    antigravityAccountsApi.list(),
    antigravityAccountsApi.usage(false)
  ])
  const reads = Object.fromEntries(usage.map((read) => [read.accountId, read]))
  const state = useAcpStore.getState()
  let next: AcpState['sessionUsage'] | null = null
  for (const [sessionId, session] of Object.entries(state.sessions)) {
    if (!session.projectId || !isAntigravitySession(state, sessionId)) continue
    if (state.sessionUsage[sessionId]?.rateLimit?.status === 'rejected') continue
    // A finished chat stays on screen. Only a turn that is still held is marked.
    const held = session.activeTurn === true || session.openTurnId != null
    if (!held || !runsProjectAccount(state, sessionId)) continue
    // The chat's own account, else the project's. A pin is not the project account.
    const accountId = antigravityAccountFor(snapshot, session.projectId, sessionId)
    const window = primaryAntigravityWindow(reads[accountId])
    if (!window || window.percent < 100 || !antigravityWindowOpen(window)) continue
    const limit: ClaudeRateLimit = {
      status: 'rejected',
      resetsAt: antigravityResetSeconds(window),
      kind: null,
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
export function startAntigravityLimitProbe(): () => void {
  ensureAntigravityResumeTurnWatch()
  void stampProbeLimits().catch((error) => {
    void logFrontendError({
      level: 'warn',
      source: 'antigravityLimitProbe',
      message: `Could not read Antigravity usage: ${error instanceof Error ? error.message : String(error)}`
    })
  })
  const timer = setInterval(() => {
    void stampProbeLimits().catch(() => undefined)
  }, PROBE_STAMP_MS)
  return () => clearInterval(timer)
}

/** Mount once per renderer root. */
export function useAntigravityLimitProbe(): void {
  useEffect(() => startAntigravityLimitProbe(), [])
}
