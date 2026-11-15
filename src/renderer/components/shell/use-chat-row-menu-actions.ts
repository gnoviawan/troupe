import { useCallback } from 'react'
import { toast } from 'sonner'
import type { ProjectChatEntry } from '@/components/sidebar/project-chat-row'
import { openerApi } from '@/lib/api'
import { copyChatReference } from '@/lib/chat-reference'
import { copyTextWithToast } from '@/lib/copy-text'
import { logFrontendError } from '@/lib/log-api'
import { openAgentChatPlaced } from '@/lib/open-agent-chat'
import { conversationDeleteIds } from '@/lib/session-chain'
import { openTerminalAtCwd } from '@/lib/terminal-spawn'
import { useAcpStore } from '@/stores/acp-store'
import { liveSwitchSources } from '@/stores/acp-store/shared-state'
import { useWorkspaceStore } from '@/stores/workspace-store'

const LOG_SOURCE = 'shell-chat-row-menu'

export interface ChatRowMenuActions {
  onOpenInSplit: (entry: ProjectChatEntry) => void
  onOpenTerminal: (entry: ProjectChatEntry) => Promise<void>
  onOpenInFileExplorer: (cwd: string) => Promise<void>
  onCopyPath: (cwd: string) => Promise<void>
  onCopyChatReference: (entry: ProjectChatEntry) => Promise<void>
  onCopyChatId: (entry: ProjectChatEntry) => Promise<void>
  /** Deletes the whole conversation. Call it only after the user confirms. */
  onDelete: (entry: ProjectChatEntry) => void
}

/**
 * The actions of the Rail chat row menu (Paper "Chat row menu"): open in a
 * split, open a terminal or the folder at the chat's cwd, copy its path,
 * reference or id, and delete it.
 */
export function useChatRowMenuActions(projectId: string): ChatRowMenuActions {
  const deleteHistorySession = useAcpStore((s) => s.deleteHistorySession)

  const onOpenInSplit = useCallback((entry: ProjectChatEntry) => {
    // Read at call time: the active pane can change after the render.
    const { activePaneId } = useWorkspaceStore.getState()
    openAgentChatPlaced(entry.id, LOG_SOURCE, { paneId: activePaneId, zone: 'right' })
  }, [])

  const onOpenTerminal = useCallback(
    async (entry: ProjectChatEntry) => {
      if (!entry.cwd) return
      const outcome = await openTerminalAtCwd(projectId, entry.cwd)
      if (outcome.status === 'opened') return
      if (outcome.status === 'no-pane') {
        toast.error('No active pane', { description: 'Open a pane first, then try again.' })
        return
      }
      toast.error('Could not open a terminal', {
        description: outcome.error || 'Try again.'
      })
    },
    [projectId]
  )

  const onCopyPath = useCallback(async (cwd: string) => {
    await copyTextWithToast(cwd, {
      title: 'Path copied',
      description: cwd,
      failureTitle: 'Could not copy the path',
      logSource: LOG_SOURCE
    })
  }, [])

  const onOpenInFileExplorer = useCallback(
    async (cwd: string) => {
      const result = await openerApi.revealInFileManager(cwd)
      // The browser client has no file manager: copy the path instead.
      if (!result.success) await onCopyPath(cwd)
    },
    [onCopyPath]
  )

  const onCopyChatReference = useCallback(async (entry: ProjectChatEntry) => {
    await copyChatReference(entry.id, entry.title, LOG_SOURCE)
  }, [])

  const onCopyChatId = useCallback(async (entry: ProjectChatEntry) => {
    await copyTextWithToast(entry.id, {
      title: 'Chat ID copied',
      description: entry.id,
      failureTitle: 'Could not copy the chat ID',
      logSource: LOG_SOURCE
    })
  }, [])

  // A folded row is one conversation. Membership is read at click time, not
  // from the rendered `chainIds`: a switch that ends while the confirm is
  // open adds a session, and an old list would leave it behind.
  const onDelete = useCallback(
    (entry: ProjectChatEntry) => {
      const { sessionIndex } = useAcpStore.getState()
      for (const memberId of conversationDeleteIds(sessionIndex, entry.id, liveSwitchSources)) {
        void deleteHistorySession(memberId).catch((error: unknown) => {
          // Never the chat title or prompt text: the error message only.
          const reason = error instanceof Error ? error.message : 'unknown error'
          void logFrontendError({
            level: 'warn',
            source: LOG_SOURCE,
            message: `[rail-chats] delete chat failed: ${reason}`
          })
          toast.error('Could not delete that chat. Try again.')
        })
      }
    },
    [deleteHistorySession]
  )

  return {
    onOpenInSplit,
    onOpenTerminal,
    onOpenInFileExplorer,
    onCopyPath,
    onCopyChatReference,
    onCopyChatId,
    onDelete
  }
}
