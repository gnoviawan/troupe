import { UNASSIGNED_PROJECT_ID } from '@shared/types/web-projects.types'
import { logFrontendError } from '@/lib/log-api'
import { navigateToChatSession } from '@/lib/router-navigate'
import type { AcpState } from '@/stores/acp-store'
import { useAcpStore } from '@/stores/acp-store'
import { closeQuickChatView, useQuickChatStore } from '@/stores/quick-chat-store'

/**
 * A chat with no project: `quick` (ADR-0006) or `older` (a legacy
 * unassigned session, read-only). Neither lives in a project pane tree.
 */
export type NoProjectChat = 'quick' | 'older'

/** `quick`, `older`, or `null` for a project chat or an unknown session. */
export function noProjectChatKind(
  state: Pick<AcpState, 'sessions' | 'sessionIndex'>,
  sessionId: string
): NoProjectChat | null {
  const session = state.sessions[sessionId]
  const entry = state.sessionIndex.find((item) => item.id === sessionId)
  if ((session?.kind ?? entry?.kind) === 'quick') return 'quick'
  const projectId = session?.projectId || entry?.projectId
  return projectId === UNASSIGNED_PROJECT_ID ? 'older' : null
}

/**
 * Show a no-project chat in the quick view and put it on the `#/c/<id>` route.
 * `replaceRoute` changes the current history entry in place (a launch
 * placeholder that became its session).
 */
export function showNoProjectChat(
  sessionId: string,
  kind: NoProjectChat,
  source: string,
  replaceRoute = false
): void {
  const store = useQuickChatStore.getState()
  if (kind === 'older') store.openOlderChat(sessionId)
  else store.openChat(sessionId)
  navigateToChatSession(sessionId, replaceRoute ? { replace: true } : undefined)
  void logFrontendError({
    level: 'info',
    source,
    message: `Opened ${kind} chat ${sessionId} in the quick chat view`
  })
}

/**
 * When `sessionId` is a no-project chat, show it in the quick view and return
 * `true`. A project chat or an unknown session returns `false` and changes
 * nothing.
 */
export function openNoProjectChat(sessionId: string, source: string): boolean {
  const kind = noProjectChatKind(useAcpStore.getState(), sessionId)
  if (!kind) return false
  showNoProjectChat(sessionId, kind, source)
  return true
}

/**
 * Close the quick view before a project chat, the project launcher or a
 * project goes on screen. Without this the quick view stays on top and the
 * project change is not visible. A no-op when the view is closed.
 */
export function leaveQuickChatView(source: string): void {
  const sessionId = closeQuickChatView()
  if (!sessionId) return
  void logFrontendError({
    level: 'info',
    source,
    message: `Closed the quick chat view (${sessionId}) for a project view`
  })
}

/** Frames {@link focusQuickChatComposer} waits for the composer to mount. */
const FOCUS_COMPOSER_MAX_FRAMES = 30

/**
 * Put focus in the open quick chat's composer. It waits a few frames for a
 * new chat's composer to mount. The composer focuses itself only on mount
 * in some layouts, so `+` (a new chat, or the empty one already on screen)
 * needs this.
 */
export function focusQuickChatComposer(): void {
  let frames = 0
  const tryFocus = (): void => {
    const editor = document.querySelector<HTMLElement>(
      '[data-testid="quick-chat-main"] [contenteditable="true"]'
    )
    if (editor) {
      editor.focus()
      return
    }
    frames += 1
    if (frames < FOCUS_COMPOSER_MAX_FRAMES) requestAnimationFrame(tryFocus)
  }
  requestAnimationFrame(tryFocus)
}
