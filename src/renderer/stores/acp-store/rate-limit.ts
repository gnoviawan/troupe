import {
  ANTIGRAVITY_AGENT_CONFIG_ID,
  ANTIGRAVITY_ERROR_AGENT
} from '@shared/types/antigravity-accounts.types'
import type { ClaudeRateLimit } from '@shared/types/claude-accounts.types'
import type { SessionUsage } from '@/lib/acp-api'

export function withRateLimit(
  usage: SessionUsage | undefined,
  rateLimit: ClaudeRateLimit
): SessionUsage {
  const base = usage ?? { used: 0, size: 0, baselineUsed: 0, source: 'reported' as const }
  return { ...base, rateLimit, updatedAt: Date.now() }
}

export function withoutRejectedRateLimit(
  sessionUsage: Record<string, SessionUsage>,
  sessionId: string
): { sessionUsage: Record<string, SessionUsage> } | Record<string, never> {
  const usage = sessionUsage[sessionId]
  if (usage?.rateLimit?.status !== 'rejected') return {}
  const { rateLimit: _cleared, ...rest } = usage
  return { sessionUsage: { ...sessionUsage, [sessionId]: rest } }
}

const isAntigravityPlanLimit = (data: object): boolean =>
  'agent' in data &&
  data.agent === ANTIGRAVITY_ERROR_AGENT &&
  'errorKind' in data &&
  data.errorKind === 'quota_exhausted'

/**
 * An adapter plan-limit error of the chat's agent. `configId` is the agent
 * config of the chat (`null` when unknown). An Antigravity limit
 * (`agent: "antigravity"`) counts only for an Antigravity chat, and an
 * Antigravity chat counts only that limit, so each limit reaches its own
 * account flow. Codex kinds count for every other agent, as before. A hit
 * uses the error's `resetsAt` when it has one, else the last good usage probe.
 */
export function isAdapterPlanLimit(data: unknown, configId: string | null = null): boolean {
  if (!data || typeof data !== 'object') return false
  if (configId === ANTIGRAVITY_AGENT_CONFIG_ID) return isAntigravityPlanLimit(data)
  if ('agent' in data && data.agent === ANTIGRAVITY_ERROR_AGENT) return false
  const kind = 'errorKind' in data ? data.errorKind : undefined
  if (kind === 'rate_limited' || kind === 'quota_exhausted') return true
  const info = 'codexErrorInfo' in data ? data.codexErrorInfo : undefined
  return info === 'usageLimitExceeded' || info === 'rateLimitExceeded'
}

/**
 * Unix seconds of an Antigravity plan-limit error's `resetsAt` (epoch
 * milliseconds); `null` when absent or for any other error.
 */
export function errorResetSeconds(data: unknown): number | null {
  if (!data || typeof data !== 'object' || !isAntigravityPlanLimit(data)) return null
  if (!('resetsAt' in data)) return null
  const at = data.resetsAt
  return typeof at === 'number' && Number.isFinite(at) && at > 0 ? Math.floor(at / 1000) : null
}

export function rejectedRateLimit(message: string, data?: unknown): ClaudeRateLimit {
  const kind = /5-hour/i.test(message) ? 'five_hour' : /weekly/i.test(message) ? 'seven_day' : null
  return {
    status: 'rejected',
    resetsAt: errorResetSeconds(data),
    kind,
    percent: null,
    overageInUse: false
  }
}
