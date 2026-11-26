import type { ClaudeRateLimit } from '@shared/types/claude-accounts.types'
import { useEffect, useState } from 'react'
import { create } from 'zustand'
import { toastAccountChoiceError } from '@/components/settings/provider/account-choice-error'
import { antigravityAccountsApi } from '@/lib/antigravity-accounts-api'
import { formatUsageReset } from '@/lib/antigravity-usage-format'
import { logFrontendError } from '@/lib/log-api'
import { useAcpStore } from '@/stores/acp-store'
import {
  chatSwitchNotice,
  chatSwitchToast,
  showChatSwitchToast,
  switchChatAntigravityAccount
} from '@/stores/antigravity-account-switch'
import { useChatAntigravityAccount } from './use-chat-antigravity-account'
import type {
  AntigravityAccountChoice,
  ProjectAntigravityAccount
} from './use-project-antigravity-account'

const WINDOW_NAMES: Record<string, string> = {
  five_hour: '5-hour',
  seven_day: 'weekly',
  session: '5-hour',
  weekly: 'weekly'
}

export const windowName = (limit: ClaudeRateLimit): string =>
  (limit.kind && WINDOW_NAMES[limit.kind]) || 'usage'

export const formatClock = (ms: number): string =>
  new Date(ms).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })

/** Near the limit: a calm warning from 80%. */
export const isNearLimit = (limit: ClaudeRateLimit): boolean =>
  limit.status === 'allowed_warning' || (limit.status === 'allowed' && (limit.percent ?? 0) >= 80)

/** At the limit. Antigravity has no paid overage that carries the turn. */
export const isAtLimit = (limit: ClaudeRateLimit): boolean => limit.status === 'rejected'

/**
 * The limit to show: only a turn the agent rejected at its limit, with the
 * reset time its error named. A usage check is for display only, so it never
 * raises a warning, marks a limit or names a reset. Antigravity quotas are
 * per model, not a 5-hour or weekly window, so the limit has no window kind
 * and reads as the "usage" limit.
 */
export function antigravityDisplayLimit(live: ClaudeRateLimit | null): ClaudeRateLimit | null {
  if (live?.status !== 'rejected') return null
  return {
    status: 'rejected',
    resetsAt: live.resetsAt,
    kind: live.kind ?? null,
    percent: live.percent ?? 100,
    overageInUse: false
  }
}

interface ResumeState {
  at: Record<string, number>
  /** `openTurnId` when the resume was scheduled. `null` means the turn had already ended. */
  turns: Record<string, string | null>
  timers: Record<string, ReturnType<typeof setTimeout>>
  schedule: (sessionId: string, at: number) => void
  /** Drop the local schedule. Does not talk to the host. */
  forget: (sessionId: string) => void
  /** Drop the local schedule and ask the host to drop it. */
  cancel: (sessionId: string) => void
}

/** A newer turn started after the resume was scheduled. */
function turnReplaced(scheduled: string | null, current: string | null): boolean {
  if (scheduled === null) return current !== null
  return current !== null && current !== scheduled
}

function ownsResume(sessionId: string, at: number, turnId: string | null): boolean {
  const state = useAntigravityResumeStore.getState()
  return state.at[sessionId] === at && state.turns[sessionId] === turnId
}

/** Drop the local schedule. The host copy is a separate call. */
function forgetResume(
  set: (partial: Partial<ResumeState>) => void,
  get: () => ResumeState,
  sessionId: string
): void {
  const timer = get().timers[sessionId]
  if (timer) clearTimeout(timer)
  set({
    at: Object.fromEntries(Object.entries(get().at).filter(([id]) => id !== sessionId)),
    turns: Object.fromEntries(Object.entries(get().turns).filter(([id]) => id !== sessionId)),
    timers: Object.fromEntries(Object.entries(get().timers).filter(([id]) => id !== sessionId))
  })
}

export const useAntigravityResumeStore = create<ResumeState>((set, get) => ({
  at: {},
  turns: {},
  timers: {},
  schedule: (sessionId, at) => {
    forgetResume(set, get, sessionId)
    const turnId = useAcpStore.getState().sessions[sessionId]?.openTurnId ?? null
    set((state) => ({
      at: { ...state.at, [sessionId]: at },
      turns: { ...state.turns, [sessionId]: turnId }
    }))
    void antigravityAccountsApi.setResume(sessionId, at).then(
      () => {
        const current = useAcpStore.getState().sessions[sessionId]?.openTurnId ?? null
        const owns = ownsResume(sessionId, at, turnId)
        const slotEmpty = !(sessionId in get().at)
        // A newer schedule for this chat owns the host wait. Cancelling here
        // would drop that newer time. Cancel only this attempt, or a slot
        // that is already empty because the user cleared it locally.
        if (owns && turnReplaced(turnId, current)) {
          return antigravityAccountsApi.setResume(sessionId, null).then(() => {
            if (ownsResume(sessionId, at, turnId)) get().forget(sessionId)
          })
        }
        if (!owns && slotEmpty) return antigravityAccountsApi.setResume(sessionId, null)
      },
      (error: unknown) => {
        void logFrontendError({
          level: 'warn',
          source: 'antigravityLimit.resume',
          message: `Could not ask the host to continue chat ${sessionId}: ${error instanceof Error ? error.message : String(error)}`
        })
        if (!ownsResume(sessionId, at, turnId)) return
        get().forget(sessionId)
        toastAccountChoiceError(error, 'Could not schedule the continue')
      }
    )
  },
  forget: (sessionId) => forgetResume(set, get, sessionId),
  cancel: (sessionId) => {
    forgetResume(set, get, sessionId)
    void antigravityAccountsApi.setResume(sessionId, null).catch((error: unknown) => {
      void logFrontendError({
        level: 'warn',
        source: 'antigravityLimit.resume',
        message: `Could not cancel the continue for chat ${sessionId}: ${error instanceof Error ? error.message : String(error)}`
      })
    })
  }
}))

let stopResumeTurnWatch: (() => void) | undefined

/** Watch turn changes even when no limit card is mounted. Idempotent, and a
 * no-op when the store mock has no `subscribe` (component tests). */
export function ensureAntigravityResumeTurnWatch(): void {
  if (stopResumeTurnWatch) return
  if (typeof useAcpStore.subscribe !== 'function') return
  stopResumeTurnWatch = useAcpStore.subscribe((state, prev) => {
    if (state.sessions === prev.sessions) return
    const { turns, cancel } = useAntigravityResumeStore.getState()
    for (const sessionId of Object.keys(turns)) {
      const current = state.sessions[sessionId]?.openTurnId ?? null
      if (turnReplaced(turns[sessionId], current)) cancel(sessionId)
    }
  })
}

export const useAntigravitySwitchingStore = create<Record<string, string>>(() => ({}))

export function setSwitchingTo(sessionId: string, accountName: string | null): void {
  useAntigravitySwitchingStore.setState((state) => {
    const { [sessionId]: _done, ...rest } = state
    return accountName ? { ...rest, [sessionId]: accountName } : rest
  }, true)
}

const dismissed = new Set<string>()

export interface AntigravityLimit {
  ctx: ProjectAntigravityAccount
  limit: ClaudeRateLimit
  name: string
  reset: string | null
  dismiss: () => void
  switchTo: (target: AntigravityAccountChoice) => Promise<void>
  switchingTo: string | null
}

/**
 * Live limit state of one Antigravity chat, shared by the composer warning and the
 * at-limit card. `null` when there is no limit to show or the user dismissed it.
 */
export function useAntigravityLimit(sessionId: string): AntigravityLimit | null {
  const ctx = useChatAntigravityAccount(sessionId)
  const live = useAcpStore((state) => state.sessionUsage?.[sessionId]?.rateLimit ?? null)
  const openTurnId = useAcpStore((state) => state.sessions[sessionId]?.openTurnId ?? null)
  const cancelResume = useAntigravityResumeStore((state) => state.cancel)
  const scheduledTurn = useAntigravityResumeStore((state) =>
    sessionId in state.turns ? state.turns[sessionId] : undefined
  )
  const switchingTo = useAntigravitySwitchingStore((state) => state[sessionId] ?? null)
  useEffect(() => {
    ensureAntigravityResumeTurnWatch()
  }, [])
  useEffect(() => {
    if (scheduledTurn === undefined) return
    if (turnReplaced(scheduledTurn, openTurnId)) cancelResume(sessionId)
  }, [scheduledTurn, openTurnId, sessionId, cancelResume])
  const [, rerender] = useState(0)
  const limit = ctx ? antigravityDisplayLimit(live) : null
  if (!ctx || !limit) return null

  const key = `${sessionId}:${limit.status}:${limit.resetsAt ?? ''}`
  if (dismissed.has(key)) return null

  const projectId = ctx.projectId
  const currentAccountId = ctx.current.account.id
  const currentName = ctx.current.account.name
  const ownAccountId = ctx.ownAccount ? currentAccountId : null
  const projectAccountId = ctx.projectAccount.id
  const reloadAccounts = ctx.accounts.reload
  const atLimit = limit.status === 'rejected'
  const limitWindow = windowName(limit)
  async function switchTo(target: AntigravityAccountChoice): Promise<void> {
    if (useAntigravitySwitchingStore.getState()[sessionId]) return
    setSwitchingTo(sessionId, target.account.name)
    const savedAt = useAntigravityResumeStore.getState().at[sessionId]
    const savedTurn = useAntigravityResumeStore.getState().turns[sessionId]
    const hadResume = sessionId in useAntigravityResumeStore.getState().at
    useAntigravityResumeStore.getState().forget(sessionId)
    try {
      const moved = await switchChatAntigravityAccount(
        {
          sessionId,
          projectId,
          runsOn: currentAccountId,
          ownAccountId,
          projectAccountId
        },
        target.account.id,
        atLimit ? { notice: chatSwitchNotice(currentName, target.account.name, limitWindow) } : {}
      )
      await reloadAccounts()
      if (moved === 'restarted' || moved === 'pending' || moved === 'notice-failed') {
        void antigravityAccountsApi.setResume(sessionId, null).catch(() => undefined)
      } else if (hadResume) {
        useAntigravityResumeStore.setState((state) => ({
          at: { ...state.at, [sessionId]: savedAt },
          turns: { ...state.turns, [sessionId]: savedTurn }
        }))
      }
      showChatSwitchToast(chatSwitchToast(moved, target.account.name))
    } catch (err) {
      if (hadResume && !(sessionId in useAntigravityResumeStore.getState().at)) {
        useAntigravityResumeStore.setState((state) => ({
          at: { ...state.at, [sessionId]: savedAt },
          turns: { ...state.turns, [sessionId]: savedTurn }
        }))
      }
      toastAccountChoiceError(err, 'Could not switch')
      void reloadAccounts()
    } finally {
      setSwitchingTo(sessionId, null)
    }
  }

  return {
    ctx,
    limit,
    name: ctx.current.account.name,
    reset: formatUsageReset(limit.resetsAt),
    dismiss: () => {
      dismissed.add(key)
      rerender((count) => count + 1)
    },
    switchTo,
    switchingTo
  }
}
