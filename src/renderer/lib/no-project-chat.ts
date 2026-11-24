import { UNASSIGNED_PROJECT_ID } from '@shared/types/web-projects.types'
import { toast } from 'sonner'
import { UNTITLED_HOST_TITLE } from '@/lib/acp-history-persistence'
import { focusComposerWhenReady } from '@/lib/focus-composer'
import { logFrontendError } from '@/lib/log-api'
import { navigateToChatSession, navigateToWorkspaceRoot } from '@/lib/router-navigate'
import type { AcpState } from '@/stores/acp-store'
import { useAcpStore } from '@/stores/acp-store'
import {
  closeQuickChatView,
  describeQuickChatView,
  followQuickLaunch,
  startedQuickLaunch,
  useQuickChatSessionId,
  useQuickChatStore
} from '@/stores/quick-chat-store'

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

/** Show a no-project chat in the quick view and put it on the `#/c/<id>` route. */
export function showNoProjectChat(sessionId: string, kind: NoProjectChat, source: string): void {
  const store = useQuickChatStore.getState()
  if (kind === 'older') store.openOlderChat(sessionId)
  else store.openChat(sessionId)
  navigateToChatSession(sessionId)
  void logFrontendError({
    level: 'info',
    source,
    message: `Opened ${kind} chat ${sessionId} in the quick chat view`
  })
}

/**
 * Show the agent launcher for a new quick chat in the quick view. No session
 * starts: Send in the launcher starts the chat with its first message. The
 * launcher owns no `#/c/<id>` route, so the route goes to `#/`.
 */
export function showQuickChatLauncher(source: string): void {
  useQuickChatStore.getState().openLauncher()
  navigateToWorkspaceRoot()
  void logFrontendError({
    level: 'info',
    source,
    message: 'Opened the quick chat launcher'
  })
}

/**
 * The quick launch placeholder `from` became session `to`: the view follows
 * it when it is on screen (see {@link followQuickLaunch}).
 */
export function adoptQuickChat(from: string, to: string, source: string): void {
  const followed = followQuickLaunch(from, to)
  void logFrontendError({
    level: 'info',
    source,
    message: `Quick chat ${from} started as ${to}${followed ? '' : ' off screen'}`
  })
}

/**
 * When the route names a quick launch placeholder that started off screen
 * (Back to its stale history entry), replace that entry with its session and
 * return `true`. The route change then shows the session.
 */
export function followStartedQuickLaunchRoute(placeholderId: string, source: string): boolean {
  const sessionId = startedQuickLaunch(placeholderId)
  if (!sessionId) return false
  navigateToChatSession(sessionId, { replace: true })
  void logFrontendError({
    level: 'info',
    source,
    message: `Route of quick chat launch ${placeholderId} now names its session ${sessionId}`
  })
  return true
}

/**
 * Show a no-project chat from a list (Rail, palette) and load its history.
 * A failed load gets a toast.
 */
export function openNoProjectHistoryChat(
  sessionId: string,
  kind: NoProjectChat,
  source: string
): void {
  showNoProjectChat(sessionId, kind, source)
  void useAcpStore
    .getState()
    .openHistorySession(sessionId)
    .catch((error: unknown) => {
      void logFrontendError({
        level: 'warn',
        source,
        message: `Could not open ${kind} chat ${sessionId}: ${error instanceof Error ? error.message : String(error)}`
      })
      toast.error('Could not open that chat. Try again.')
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
  const closed = closeQuickChatView()
  if (!closed) return
  void logFrontendError({
    level: 'info',
    source,
    message: `Closed ${describeQuickChatView(closed)} for a project view`
  })
}

/**
 * Put focus in the quick chat launcher's composer. The launcher focuses itself
 * only on mount, and mounts read-only until its agent resolves, so `+` (a new
 * launcher, or the one already on screen) needs this.
 */
export function focusQuickLauncherComposer(): void {
  focusComposerWhenReady(
    '[data-testid="quick-chat-main"] [aria-label="Agent prompt"][contenteditable="true"]'
  )
}

/** The open quick chat's title (live, then index), or `undefined` when none is open. */
export function useQuickChatTitle(): string | undefined {
  const sessionId = useQuickChatSessionId()
  return useAcpStore((state) => {
    if (!sessionId) return undefined
    return (
      state.sessions[sessionId]?.title ||
      state.sessionIndex.find((entry) => entry.id === sessionId)?.title ||
      UNTITLED_HOST_TITLE
    )
  })
}
