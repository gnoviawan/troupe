import type {
  AntigravityAccountUsage,
  AntigravityUsageWindow
} from '@shared/types/antigravity-accounts.types'
import type { ClaudeUsageWindow } from '@shared/types/claude-accounts.types'
import {
  formatUsageChecked,
  formatUsageReset,
  type UsageReading,
  type UsageTone,
  usageTone
} from '@/lib/claude-usage-format'

export type { UsageReading, UsageTone }
export { formatUsageChecked, formatUsageReset, usageTone }

export const ANTIGRAVITY_USAGE_WARNING_PERCENT = 80

/** The model name of a window, e.g. "Gemini 3 Pro". */
export function antigravityWindowLabel(window: AntigravityUsageWindow): string {
  return window.label || window.modelId
}

/** Unix seconds of a window reset, for the live limit record. */
export function antigravityResetSeconds(
  window: AntigravityUsageWindow | null | undefined
): number | null {
  if (!window?.resetsAt) return null
  const at = Date.parse(window.resetsAt)
  return Number.isNaN(at) ? null : Math.floor(at / 1000)
}

/** A window whose reset time is still ahead. A missing time stays usable. */
export function antigravityWindowOpen(window: AntigravityUsageWindow, now = Date.now()): boolean {
  const resetAt = antigravityResetSeconds(window)
  return resetAt === null || resetAt * 1000 > now
}

/**
 * The window that sets the Account's room: the most used model that has not
 * reset. `null` when the read failed or reported no model.
 */
export function primaryAntigravityWindow(
  usage: AntigravityAccountUsage | undefined,
  now = Date.now()
): AntigravityUsageWindow | null {
  if (usage?.status !== 'ok') return null
  return usage.windows.reduce<AntigravityUsageWindow | null>((top, window) => {
    if (!antigravityWindowOpen(window, now)) return top
    return !top || window.percent > top.percent ? window : top
  }, null)
}

/** Shape the shared usage bar accepts. */
export function antigravityBarWindow(window: AntigravityUsageWindow): ClaudeUsageWindow {
  return {
    kind: 'model',
    group: null,
    scope: antigravityWindowLabel(window),
    percent: window.percent,
    resetsAt: window.resetsAt,
    severity: null
  }
}

/** The most used model, for the composer chip. */
export function antigravityChipUsage(usage: AntigravityAccountUsage | undefined): {
  main: UsageReading
  other: UsageReading | null
} | null {
  const window = primaryAntigravityWindow(usage)
  if (!window) return null
  const percent = Math.round(window.percent)
  const label = antigravityWindowLabel(window)
  return {
    main: { short: label, label, percent, tone: usageTone(percent) },
    other: null
  }
}

export function formatAntigravityChecked(checkedAt: number, now = Date.now()): string {
  const at = new Date(checkedAt)
  const time = at.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
  if (at.toDateString() === new Date(now).toDateString()) return `Checked ${time}`
  const day = at.toLocaleDateString([], { month: 'short', day: 'numeric' })
  return `Checked ${day}, ${time}`
}

export function planLabel(planType: string | null): string | null {
  if (!planType) return null
  return planType.charAt(0).toUpperCase() + planType.slice(1)
}
