/**
 * The host moves an Antigravity chat that reaches its account's limit. This client
 * does not move the project and does not send the last prompt again.
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
import { useAgentChatLifetimeStore } from '@/stores/agent-chat-lifetime-store'
import {
  isAntigravitySession,
  isChatOpenHere,
  openChatIds,
  runsProjectAccount
} from '@/stores/antigravity-account-switch'
import { useWorkspaceStore } from '@/stores/workspace-store'
import { ensureAntigravityResumeTurnWatch, isAtLimit } from './use-antigravity-limit'
import {
  type AntigravityAccountChoice,
  antigravityAccountFor
} from './use-project-antigravity-account'

export type AutoSwitchDecision =
  | { kind: 'switch'; to: AntigravityAccountChoice }
  | { kind: 'resume'; at: number }
  | { kind: 'none'; reason: 'off' | 'no-room' }

export interface AutoSwitchInput {
  limit: ClaudeRateLimit
  autoSwitch: boolean
  candidates: readonly AntigravityAccountChoice[]
  limited: ReadonlyMap<string, number>
  now: number
}

export function decideAutoSwitch({
  limit,
  autoSwitch,
  candidates,
  limited,
  now
}: AutoSwitchInput): AutoSwitchDecision {
  if (!autoSwitch) return { kind: 'none', reason: 'off' }
  const to = candidates.find((candidate) => (limited.get(candidate.account.id) ?? 0) <= now)
  if (to) return { kind: 'switch', to }
  if (limit.resetsAt !== null) return { kind: 'resume', at: limit.resetsAt * 1000 }
  return { kind: 'none', reason: 'no-room' }
}

const log = (message: string, level: 'info' | 'warn' = 'info'): void =>
  void logFrontendError({ level, source: 'antigravityAutoSwitch', message })

const READ_RETRY_MS = 30_000
const PROBE_STAMP_MS = 20_000

const limitEvent = (limit: ClaudeRateLimit): string => `${limit.kind ?? ''}:${limit.resetsAt ?? ''}`

interface LimitedChat {
  sessionId: string
  projectId: string
  limit: ClaudeRateLimit
}

function limitedChats(state: AcpState): LimitedChat[] {
  const chats: LimitedChat[] = []
  for (const [sessionId, usage] of Object.entries(state.sessionUsage)) {
    const limit = usage?.rateLimit
    const projectId = state.sessions[sessionId]?.projectId
    if (!limit || !isAtLimit(limit) || !projectId || !isAntigravitySession(state, sessionId))
      continue
    chats.push({ sessionId, projectId, limit })
  }
  return chats
}

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

/**
 * Watch every Antigravity chat's limit and act on each new limit once. Only a
 * client that has the chat open acts. Returns the stop function.
 */
export function startAntigravityAutoSwitch(): () => void {
  const handled = new Map<string, string>()
  const busyProjects = new Set<string>()
  let lastUsage: AcpState['sessionUsage'] | null = null
  let stopped = false
  ensureAntigravityResumeTurnWatch()

  async function act(projectId: string, chats: readonly LimitedChat[]): Promise<void> {
    log(
      `Chats ${chats.map((chat) => chat.sessionId).join(', ')} of project ${projectId} reached a limit; the host continues them`
    )
    return
  }

  let openKey = ''
  function check(state: AcpState): void {
    const nextOpen = [...openChatIds()].sort().join('\0')
    const opened = nextOpen !== openKey
    if (stopped || (state.sessionUsage === lastUsage && !opened)) return
    lastUsage = state.sessionUsage
    openKey = nextOpen
    for (const sessionId of handled.keys()) {
      const limit = state.sessionUsage[sessionId]?.rateLimit
      if (!limit || !isAtLimit(limit)) handled.delete(sessionId)
    }
    const byProject = new Map<string, LimitedChat[]>()
    for (const chat of limitedChats(state)) {
      if (handled.get(chat.sessionId) === limitEvent(chat.limit)) continue
      if (busyProjects.has(chat.projectId) || !isChatOpenHere(chat.sessionId)) continue
      handled.set(chat.sessionId, limitEvent(chat.limit))
      byProject.set(chat.projectId, [...(byProject.get(chat.projectId) ?? []), chat])
    }
    for (const [projectId, chats] of byProject) {
      busyProjects.add(projectId)
      void act(projectId, chats)
        .catch((error) => {
          log(
            `Automatic switching of project ${projectId} failed: ${error instanceof Error ? error.message : String(error)}; trying again in ${READ_RETRY_MS / 1000}s`,
            'warn'
          )
          setTimeout(() => {
            for (const { sessionId } of chats) handled.delete(sessionId)
            lastUsage = null
            check(useAcpStore.getState())
          }, READ_RETRY_MS)
        })
        .finally(() => {
          busyProjects.delete(projectId)
          lastUsage = null
          check(useAcpStore.getState())
        })
    }
  }

  check(useAcpStore.getState())
  void stampProbeLimits().catch((error) => {
    log(
      `Could not read Antigravity usage: ${error instanceof Error ? error.message : String(error)}`,
      'warn'
    )
  })
  const timer = setInterval(() => {
    void stampProbeLimits().catch(() => undefined)
  }, PROBE_STAMP_MS)
  const unsubscribe = useAcpStore.subscribe(check)
  const unsubscribeOpen = useWorkspaceStore.subscribe(() => check(useAcpStore.getState()))
  const unsubscribeKept = useAgentChatLifetimeStore.subscribe(() => check(useAcpStore.getState()))
  return () => {
    stopped = true
    clearInterval(timer)
    unsubscribe()
    unsubscribeOpen()
    unsubscribeKept()
  }
}

/** Mount once per renderer root. */
export function useAntigravityAutoSwitch(): void {
  useEffect(() => startAntigravityAutoSwitch(), [])
}
