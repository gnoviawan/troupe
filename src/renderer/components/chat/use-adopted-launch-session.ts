import { useState } from 'react'
import { isLaunchPlaceholderSessionId } from '@/stores/acp-store/live-turn'

/**
 * True when this mounted chat panel showed a launch placeholder and the
 * placeholder became `sessionId` in place (`adoptSession` →
 * `remapAgentChatSession` / `adoptQuickChat`, which keep the panel's React
 * key). The panel already shows that chat with its composer, so a restore
 * that runs for the new id (a resend answered by lookup, or another open of
 * the session) must not swap the chat for the restore preload: that unmounts
 * the composer and drops the unsent draft (#1941).
 *
 * Derived during render (React's "adjust state on prop change" pattern), so
 * the first render with the new id already knows, and nothing commits the
 * preload in between.
 */
export function useAdoptedLaunchSession(sessionId: string): boolean {
  const [shownId, setShownId] = useState(sessionId)
  const [adoptedId, setAdoptedId] = useState<string | null>(null)
  if (shownId !== sessionId) {
    setShownId(sessionId)
    setAdoptedId(isLaunchPlaceholderSessionId(shownId) ? sessionId : null)
    return isLaunchPlaceholderSessionId(shownId)
  }
  return adoptedId === sessionId
}
