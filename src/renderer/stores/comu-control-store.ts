/**
 * Session-level Computer Use activity, Stop latch, pending consent tickets,
 * and visible card mount tracking across desktop and remote clients.
 */

import type { ComuConsentOptionId, ComuConsentTicket } from '@shared/types/comu.types'
import { toast } from 'sonner'
import { create } from 'zustand'
import { answerComuConsent } from '@/lib/comu-consent-api'
import { logFrontendError } from '@/lib/log-api'
import { agentChatTabId, findPaneContainingTab, useWorkspaceStore } from '@/stores/workspace-store'

interface ComuControlState {
  activeSessions: Readonly<Record<string, true>>
  stoppedSessions: Readonly<Record<string, true>>
  pendingConsents: Readonly<Record<string, ComuConsentTicket>>
  hostedSessionIds: ReadonlySet<string>
  setActive: (sessionId: string, active: boolean) => void
  setStopped: (sessionId: string, stopped: boolean) => void
  addPendingConsent: (ticket: ComuConsentTicket) => void
  removePendingConsent: (requestId: string) => void
  respondConsent: (requestId: string, optionId: ComuConsentOptionId, agentId?: string) => void
  registerHostedSession: (sessionId: string) => void
  unregisterHostedSession: (sessionId: string) => void
  forgetSession: (sessionId: string) => void
}

export const useComuControlStore = create<ComuControlState>((set, get) => ({
  activeSessions: {},
  stoppedSessions: {},
  pendingConsents: {},
  hostedSessionIds: new Set<string>(),

  setActive: (sessionId, active) =>
    set((s) => {
      if (active === Boolean(s.activeSessions[sessionId])) return s
      if (active) return { activeSessions: { ...s.activeSessions, [sessionId]: true } }
      const { [sessionId]: _gone, ...activeSessions } = s.activeSessions
      return { activeSessions }
    }),

  setStopped: (sessionId, stopped) =>
    set((s) => {
      if (stopped === Boolean(s.stoppedSessions[sessionId])) return s
      if (stopped) return { stoppedSessions: { ...s.stoppedSessions, [sessionId]: true } }
      const { [sessionId]: _gone, ...stoppedSessions } = s.stoppedSessions
      return { stoppedSessions }
    }),

  addPendingConsent: (ticket) => {
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

  removePendingConsent: (requestId) =>
    set((s) => {
      if (!s.pendingConsents[requestId]) return s
      const { [requestId]: _gone, ...pendingConsents } = s.pendingConsents
      return { pendingConsents }
    }),

  respondConsent: (requestId, optionId, agentId = '') => {
    const original = get().pendingConsents[requestId]
    if (!original) return
    set((s) => {
      const { [requestId]: _gone, ...pendingConsents } = s.pendingConsents
      return { pendingConsents }
    })
    const restore = (message: string): void => {
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

  forgetSession: (sessionId) =>
    set((s) => {
      const { [sessionId]: _a, ...activeSessions } = s.activeSessions
      const { [sessionId]: _b, ...stoppedSessions } = s.stoppedSessions
      const pendingConsents = Object.fromEntries(
        Object.entries(s.pendingConsents).filter(([, t]) => t.sessionId !== sessionId)
      )
      return { activeSessions, stoppedSessions, pendingConsents }
    })
}))

export function useSessionDrivesComu(sessionId: string | undefined): boolean {
  return useComuControlStore((s) => (sessionId ? Boolean(s.activeSessions[sessionId]) : false))
}

export function useSessionComuStopped(sessionId: string | undefined): boolean {
  return useComuControlStore((s) => (sessionId ? Boolean(s.stoppedSessions[sessionId]) : false))
}
