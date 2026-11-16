import { useRef } from 'react'
import { isLaunchPlaceholderSessionId } from '@/stores/acp-store'

/**
 * React key for the quick chat panel. Another quick chat gets a new key, so
 * its composer mounts fresh and cannot keep the text typed in the previous
 * chat. A launch placeholder that becomes its real session keeps the key: the
 * text typed while the chat starts must stay in the composer.
 */
export function quickChatPanelKey(
  previous: { sessionId: string; key: string } | null,
  sessionId: string
): string {
  if (!previous) return sessionId
  if (previous.sessionId === sessionId) return previous.key
  if (
    isLaunchPlaceholderSessionId(previous.sessionId) &&
    !isLaunchPlaceholderSessionId(sessionId)
  ) {
    return previous.key
  }
  return sessionId
}

/** {@link quickChatPanelKey} across renders. */
export function useQuickChatPanelKey(sessionId: string): string {
  const previous = useRef<{ sessionId: string; key: string } | null>(null)
  const key = quickChatPanelKey(previous.current, sessionId)
  previous.current = { sessionId, key }
  return key
}
