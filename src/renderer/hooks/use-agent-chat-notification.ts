import { useEffect } from 'react'
import {
  type AgentChatNotifyEvent,
  type AgentChatNotifySnapshot,
  decideAgentChatNotifications,
  isNotifiableStopReason
} from '@/lib/agent-chat-notify'
import { logFrontendError } from '@/lib/log-api'
import { openAgentChatInOwnProject } from '@/lib/open-agent-chat'
import { sendDesktopNotification } from '@/lib/tauri-notification-api'
import { isEphemeralAcpSession, useAcpStore } from '@/stores/acp-store'
import { useAppSettingsStore } from '@/stores/app-settings-store'
import { useProjectStore } from '@/stores/project-store'
import { sessionTurnBusy } from '@/stores/prompt-queue-orchestration'
import { useWorkspaceStore } from '@/stores/workspace-store'
import { sanitizeNotificationText } from './use-terminal-exit-notification'

/**
 * How long a chat must stay idle after its last background command ends
 * before its deferred "finished" ping goes out. The agent usually wakes up
 * right after the command ends (Claude Code runs a turn of its own).
 */
export const DEFERRED_FINISH_SETTLE_MS = 5_000

function windowIsFocused(): boolean {
  if (typeof document !== 'undefined' && typeof document.hasFocus === 'function') {
    return document.hasFocus()
  }
  return true
}

function readSnapshot(): AgentChatNotifySnapshot {
  const acp = useAcpStore.getState()
  const settings = useAppSettingsStore.getState().settings
  const sessions: AgentChatNotifySnapshot['sessions'] = {}
  for (const [sessionId, session] of Object.entries(acp.sessions ?? {})) {
    const notice = acp.turnEndNotices?.[sessionId]
    sessions[sessionId] = {
      projectId: session.projectId,
      ephemeral: isEphemeralAcpSession(sessionId),
      busy: sessionTurnBusy(session),
      queueLength: acp.promptQueues?.[sessionId]?.length ?? 0,
      turnEndSeq: notice?.seq ?? 0,
      stopReason: notice?.stopReason ?? null,
      backgroundRunning: Boolean(
        session.backgroundCommands?.agentId === session.agentId &&
          session.backgroundCommands.commands.some((command) => command.state === 'running')
      )
    }
  }
  const activeTab = useWorkspaceStore.getState().getActiveTab()
  const viewingSessionId =
    windowIsFocused() && activeTab?.type === 'agent-chat' ? activeTab.sessionId : null
  return {
    sessions,
    permissions: Object.values(acp.pendingPermissions ?? {}).map((item) => ({
      id: item.requestId,
      sessionId: item.sessionId
    })),
    questions: Object.values(acp.pendingQuestions ?? {}).map((item) => ({
      id: item.questionId,
      sessionId: item.sessionId
    })),
    viewingSessionId,
    notifyTurnFinished: settings.notifyOnAgentChatTurnFinished,
    notifyNeedsYou: settings.notifyOnAgentChatNeedsYou
  }
}

function notificationCopy(event: AgentChatNotifyEvent): { title: string; body: string } {
  const acp = useAcpStore.getState()
  const session = acp.sessions[event.sessionId]
  const indexed = acp.sessionIndex.find((entry) => entry.id === event.sessionId)
  const project = useProjectStore.getState().projects.find((item) => item.id === session?.projectId)
  const title = sanitizeNotificationText(project?.name ?? 'Troupe')
  const chat = sanitizeNotificationText(session?.title || indexed?.title || 'Agent Chat')
  const suffix =
    event.kind === 'turn-finished' || event.kind === 'turn-finish-deferred'
      ? 'finished'
      : event.kind === 'needs-approval'
        ? 'needs approval'
        : 'has a question'
  return { title, body: `${chat} — ${suffix}` }
}

/**
 * Notify when an Agent Chat turn finishes, or when a permission or question
 * is waiting, unless the user is already looking at that chat.
 */
export function useAgentChatNotification(): void {
  useEffect(() => {
    let prev: AgentChatNotifySnapshot | null = null
    // Finished turns that left a background command running.
    const deferred = new Set<string>()
    const settleTimers = new Map<string, ReturnType<typeof setTimeout>>()

    const send = (event: AgentChatNotifyEvent): void => {
      const copy = notificationCopy(event)
      // Click opens the chat in its own project (switching if needed).
      void sendDesktopNotification(copy.title, copy.body, {
        onClick: () => openAgentChatInOwnProject(event.sessionId, 'useAgentChatNotification')
      })
    }

    const cancelSettle = (sessionId: string): void => {
      const timer = settleTimers.get(sessionId)
      if (timer === undefined) return
      clearTimeout(timer)
      settleTimers.delete(sessionId)
    }

    const settle = (event: AgentChatNotifyEvent): void => {
      if (settleTimers.has(event.sessionId)) return
      const timer = setTimeout(() => {
        settleTimers.delete(event.sessionId)
        if (!deferred.has(event.sessionId)) return
        const now = readSnapshot()
        const session = now.sessions[event.sessionId]
        // Still idle after the settle window: the agent did not wake up.
        if (!session || session.busy || session.backgroundRunning || session.queueLength > 0) {
          return
        }
        deferred.delete(event.sessionId)
        if (!now.notifyTurnFinished || now.viewingSessionId === event.sessionId) return
        send(event)
      }, DEFERRED_FINISH_SETTLE_MS)
      settleTimers.set(event.sessionId, timer)
    }

    const emit = (): void => {
      const next = readSnapshot()
      for (const sessionId of deferred) {
        const session = next.sessions[sessionId]
        const prevSeq = prev?.sessions[sessionId]?.turnEndSeq ?? 0
        // A newer turn that ended without a notifiable reason (cancelled)
        // supersedes the deferred finish.
        const superseded =
          session !== undefined &&
          session.turnEndSeq > prevSeq &&
          !isNotifiableStopReason(session.stopReason)
        if (!session || !next.notifyTurnFinished || superseded) {
          deferred.delete(sessionId)
          cancelSettle(sessionId)
        } else if (session.busy) {
          // The agent woke up: the next idle stretch gets a full settle window.
          cancelSettle(sessionId)
        }
      }
      next.deferredFinishes = new Set(deferred)
      const events = decideAgentChatNotifications(prev, next)
      prev = next
      for (const event of events) {
        if (event.kind === 'turn-finish-deferred') {
          deferred.add(event.sessionId)
          void logFrontendError({
            level: 'info',
            message: `agent chat finished ping deferred: background command running, sessionId=${event.sessionId}`,
            source: 'useAgentChatNotification'
          })
          continue
        }
        if (event.kind === 'turn-finished' && event.deferred) {
          settle(event)
          continue
        }
        if (event.kind === 'turn-finished') deferred.delete(event.sessionId)
        send(event)
      }
    }

    emit()
    const unsubscribeAcp = useAcpStore.subscribe(emit)
    const unsubscribeWorkspace = useWorkspaceStore.subscribe(emit)
    const unsubscribeSettings = useAppSettingsStore.subscribe(emit)
    return () => {
      for (const timer of settleTimers.values()) clearTimeout(timer)
      unsubscribeAcp()
      unsubscribeWorkspace()
      unsubscribeSettings()
    }
  }, [])
}
