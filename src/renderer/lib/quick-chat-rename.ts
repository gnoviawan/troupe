import { toast } from 'sonner'
import { acpRenameQuickChat } from '@/lib/acp-api'
import { logFrontendError } from '@/lib/log-api'
import { useAcpStore } from '@/stores/acp-store'

/** Set a chat title in the index and in the open session record. */
function setChatTitle(sessionId: string, title: string): void {
  useAcpStore.setState((state) => ({
    sessionIndex: state.sessionIndex.map((entry) =>
      entry.id === sessionId ? { ...entry, title } : entry
    ),
    sessions: state.sessions[sessionId]
      ? { ...state.sessions, [sessionId]: { ...state.sessions[sessionId], title } }
      : state.sessions
  }))
}

/**
 * Rename a quick chat (a host `LocalAlias`). The new title shows at once; a
 * failed host write puts `previous` back and tells the user. A blank or
 * unchanged title does nothing. Shared by the desktop Rail and the phone
 * Recents sheet.
 */
export function renameQuickChat(sessionId: string, nextTitle: string, previous: string): void {
  const title = nextTitle.trim()
  if (!title || title === previous) return
  setChatTitle(sessionId, title)
  void acpRenameQuickChat(sessionId, title).catch((error: unknown) => {
    setChatTitle(sessionId, previous)
    void logFrontendError({
      level: 'error',
      source: 'quick-chat.rename',
      message: `Could not rename quick chat ${sessionId}: ${error instanceof Error ? error.message : String(error)}`
    })
    toast.error('Could not rename the chat. Try again.')
  })
}
