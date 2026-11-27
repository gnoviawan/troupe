/**
 * Session-level Computer Use activity, Stop latch, pending consent tickets,
 * and visible card mount tracking across desktop and remote clients.
 */

import type { ComuConsentOptionId, ComuConsentTicket } from '@shared/types/comu.types'
import { toast } from 'sonner'
import { create } from 'zustand'
import { answerComuConsent } from '@/lib/comu-policy-api'
import { logFrontendError } from '@/lib/log-api'
import { agentChatTabId, findPaneContainingTab, useWorkspaceStore } from '@/stores/workspace-store'

export interface ComuRecoveredSessionState {
  active: boolean
  stopped: boolean
  seq: number
}

interface ComuControlState {
  activeSessions: Readonly<Record<string, true>>
  stoppedSessions: Readonly<Record<string, true>>
  activeSeqs: Readonly<Record<string, number>>
  stopSeqs: Readonly<Record<string, number>>
  pendingConsents: Readonly<Record<string, ComuConsentTicket>>
  hostedSessionIds: ReadonlySet<string>
  setActive: (sessionId: string, active: boolean, seq?: number) => void
  setStopped: (sessionId: string, stopped: boolean, seq?: number) => void
  installSessionState: (sessionId: string, recovered: ComuRecoveredSessionState) => void
  addPendingConsent: (ticket: ComuConsentTicket) => void
  removePendingConsent: (requestId: string) => void
  respondConsent: (requestId: string, optionId: ComuConsentOptionId, agentId?: string) => void
  registerHostedSession: (sessionId: string) => void
  unregisterHostedSession: (sessionId: string) => void
  forgetSession: (sessionId: string) => void
}

const MAX_TRACKED_TICKET_GENS = 512
const ticketGenerations = new Map<string, number>()
const sessionGenerations = new Map<string, number>()

function bumpTicketGeneration(requestId: string): number {
  const next = (ticketGenerations.get(requestId) ?? 0) + 1
  ticketGenerations.delete(requestId)
  ticketGenerations.set(requestId, next)
  if (ticketGenerations.size > MAX_TRACKED_TICKET_GENS) {
    const oldest = ticketGenerations.keys().next().value
    if (oldest !== undefined) ticketGenerations.delete(oldest)
  }
  return next
}

function bumpSessionGeneration(sessionId: string): number {
  const next = (sessionGenerations.get(sessionId) ?? 0) + 1
  sessionGenerations.set(sessionId, next)
  return next
}

export const useComuControlStore = create<ComuControlState>((set, get) => ({
  activeSessions: {},
  stoppedSessions: {},
  activeSeqs: {},
  stopSeqs: {},
  pendingConsents: {},
  hostedSessionIds: new Set<string>(),

  setActive: (sessionId, active, seq) =>
    set((s) => {
      if (seq !== undefined && (s.activeSeqs[sessionId] ?? 0) > seq) return s
      const nextActiveSeqs =
        seq !== undefined ? { ...s.activeSeqs, [sessionId]: seq } : s.activeSeqs
      if (active === Boolean(s.activeSessions[sessionId])) {
        return nextActiveSeqs === s.activeSeqs ? s : { activeSeqs: nextActiveSeqs }
      }
      if (active) {
        return {
          activeSessions: { ...s.activeSessions, [sessionId]: true },
          activeSeqs: nextActiveSeqs
        }
      }
      const { [sessionId]: _gone, ...activeSessions } = s.activeSessions
      return { activeSessions, activeSeqs: nextActiveSeqs }
    }),

  setStopped: (sessionId, stopped, seq) =>
    set((s) => {
      if (seq !== undefined && (s.stopSeqs[sessionId] ?? 0) > seq) return s
      const nextStopSeqs = seq !== undefined ? { ...s.stopSeqs, [sessionId]: seq } : s.stopSeqs
      if (stopped === Boolean(s.stoppedSessions[sessionId])) {
        return nextStopSeqs === s.stopSeqs ? s : { stopSeqs: nextStopSeqs }
      }
      if (stopped) {
        return {
          stoppedSessions: { ...s.stoppedSessions, [sessionId]: true },
          stopSeqs: nextStopSeqs
        }
      }
      const { [sessionId]: _gone, ...stoppedSessions } = s.stoppedSessions
      return { stoppedSessions, stopSeqs: nextStopSeqs }
    }),

  installSessionState: (sessionId, recovered) =>
    set((s) => {
      const updateActive = recovered.seq >= (s.activeSeqs[sessionId] ?? 0)
      const updateStop = recovered.seq >= (s.stopSeqs[sessionId] ?? 0)
      if (!updateActive && !updateStop) return s

      let activeSessions = s.activeSessions
      let activeSeqs = s.activeSeqs
      if (updateActive) {
        activeSeqs = { ...s.activeSeqs, [sessionId]: recovered.seq }
        if (recovered.active) {
          activeSessions = { ...s.activeSessions, [sessionId]: true }
        } else if (s.activeSessions[sessionId]) {
          const { [sessionId]: _a, ...rest } = s.activeSessions
          activeSessions = rest
        }
      }

      let stoppedSessions = s.stoppedSessions
      let stopSeqs = s.stopSeqs
      if (updateStop) {
        stopSeqs = { ...s.stopSeqs, [sessionId]: recovered.seq }
        if (recovered.stopped) {
          stoppedSessions = { ...s.stoppedSessions, [sessionId]: true }
        } else if (s.stoppedSessions[sessionId]) {
          const { [sessionId]: _b, ...rest } = s.stoppedSessions
          stoppedSessions = rest
        }
      }

      return { activeSessions, stoppedSessions, activeSeqs, stopSeqs }
    }),

  addPendingConsent: (ticket) => {
    bumpTicketGeneration(ticket.requestId)
    set((s) => ({
      pendingConsents: { ...s.pendingConsents, [ticket.requestId]: ticket }
    }))
    const ws = useWorkspaceStore.getState()
    const tabId = agentChatTabId(ticket.sessionId)
    const pane = findPaneContainingTab(ws.root, tabId)
    if (pane && pane.activeTabId !== tabId) {
      ws.addAgentChatTab(ticket.sessionId)
      void logFrontendError({
        level: 'info',
        message: `[comu] consent ticket activated chat tab (requestId=${ticket.requestId}, sessionId=${ticket.sessionId})`,
        source: 'comu-control-store'
      })
    }
  },

  removePendingConsent: (requestId) => {
    // Bump unconditionally so an in-flight respondConsent that already removed
    // the entry optimistically cannot reinsert this closed ticket on late error.
    bumpTicketGeneration(requestId)
    set((s) => {
      if (!s.pendingConsents[requestId]) return s
      const { [requestId]: _gone, ...pendingConsents } = s.pendingConsents
      return { pendingConsents }
    })
  },

  respondConsent: (requestId, optionId, agentId = '') => {
    const original = get().pendingConsents[requestId]
    if (!original) return
    const reqGen = ticketGenerations.get(requestId) ?? 0
    const sessGen = sessionGenerations.get(original.sessionId) ?? 0
    set((s) => {
      const { [requestId]: _gone, ...pendingConsents } = s.pendingConsents
      return { pendingConsents }
    })
    const restore = (message: string): void => {
      if (
        (ticketGenerations.get(requestId) ?? 0) !== reqGen ||
        (sessionGenerations.get(original.sessionId) ?? 0) !== sessGen
      ) {
        return
      }
      set((s) =>
        s.pendingConsents[requestId]
          ? s
          : { pendingConsents: { ...s.pendingConsents, [requestId]: original } }
      )
      toast.error(message)
    }
    void answerComuConsent(original, optionId, agentId)
      .then((result) => {
        if (result === 'ok' || result === 'stale') return
        restore(
          result === 'too_early'
            ? 'Give the approval card a moment, then click again.'
            : 'Could not send your Computer Use answer. Try again.'
        )
      })
      .catch(() => {
        restore('Could not send your Computer Use answer. Try again.')
      })
  },

  registerHostedSession: (sessionId) =>
    set((s) => {
      const next = new Set(s.hostedSessionIds)
      next.add(sessionId)
      return { hostedSessionIds: next }
    }),

  unregisterHostedSession: (sessionId) =>
    set((s) => {
      const next = new Set(s.hostedSessionIds)
      next.delete(sessionId)
      return { hostedSessionIds: next }
    }),

  forgetSession: (sessionId) => {
    bumpSessionGeneration(sessionId)
    set((s) => {
      for (const [id, ticket] of Object.entries(s.pendingConsents)) {
        if (ticket.sessionId === sessionId) bumpTicketGeneration(id)
      }
      const { [sessionId]: _a, ...activeSessions } = s.activeSessions
      const { [sessionId]: _b, ...stoppedSessions } = s.stoppedSessions
      const { [sessionId]: _c, ...activeSeqs } = s.activeSeqs
      const { [sessionId]: _d, ...stopSeqs } = s.stopSeqs
      const pendingConsents = Object.fromEntries(
        Object.entries(s.pendingConsents).filter(([, t]) => t.sessionId !== sessionId)
      )
      return { activeSessions, stoppedSessions, activeSeqs, stopSeqs, pendingConsents }
    })
  }
}))

export function useSessionDrivesComu(sessionId: string | undefined): boolean {
  return useComuControlStore((s) => (sessionId ? Boolean(s.activeSessions[sessionId]) : false))
}

export function useSessionComuStopped(sessionId: string | undefined): boolean {
  return useComuControlStore((s) => (sessionId ? Boolean(s.stoppedSessions[sessionId]) : false))
}
