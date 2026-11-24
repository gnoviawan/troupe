import { create } from 'zustand'
import { clearChatRoute, isChatRouteCurrent, navigateToChatSession } from '@/lib/router-navigate'

/** What the main area shows for a quick chat. */
export type QuickChatView =
  | { mode: 'closed' }
  /**
   * The agent launcher for a new quick chat. No session exists until Send,
   * so it owns no `#/c/<id>` route.
   */
  | { mode: 'launcher' }
  | {
      mode: 'chat'
      sessionId: string
      readOnly?: boolean
      /**
       * React key of the chat panel when it is not `sessionId`: a launch
       * placeholder that became its session keeps the placeholder's key, so
       * the text typed while the chat started stays in the composer.
       */
      panelKey?: string
    }

interface QuickChatState {
  view: QuickChatView
  openLauncher: () => void
  openChat: (sessionId: string) => void
  /** An older no-project chat. History can be read. New prompts are refused. */
  openOlderChat: (sessionId: string) => void
  close: () => void
}

/**
 * The open quick chat. A quick chat has no project, so it does not live in a
 * project pane tree. Selecting a project or opening a project chat closes
 * this view (see {@link closeQuickChatView}).
 */
export const useQuickChatStore = create<QuickChatState>((set) => ({
  view: { mode: 'closed' },
  openLauncher: () => set({ view: { mode: 'launcher' } }),
  openChat: (sessionId) => set({ view: { mode: 'chat', sessionId } }),
  openOlderChat: (sessionId) => set({ view: { mode: 'chat', sessionId, readOnly: true } }),
  close: () => set({ view: { mode: 'closed' } })
}))

/** `true` while a quick chat or its launcher owns the main area. */
export function useQuickChatOpen(): boolean {
  return useQuickChatStore((state) => state.view.mode !== 'closed')
}

/** The session id of the open quick chat, or `null` (also for the launcher). */
export function useQuickChatSessionId(): string | null {
  return useQuickChatStore((state) => (state.view.mode === 'chat' ? state.view.sessionId : null))
}

/** Quick launch placeholders that became their session off screen: placeholder id to session id. */
const startedOffScreen = new Map<string, string>()

/**
 * The quick launch placeholder `from` became session `to`. When `from` is the
 * chat on screen, the view moves to `to` with the same panel key (the typed
 * text stays) and the placeholder's route entry is replaced. Otherwise the
 * user moved on while it started: the placeholder's `#/c/<from>` entry is
 * still in history and cannot be rewritten from here, so `to` is remembered
 * for {@link startedQuickLaunch}. Returns whether the view followed.
 */
export function followQuickLaunch(from: string, to: string): boolean {
  const view = useQuickChatStore.getState().view
  if (view.mode !== 'chat' || view.sessionId !== from) {
    startedOffScreen.set(from, to)
    return false
  }
  useQuickChatStore.setState({
    view: { mode: 'chat', sessionId: to, panelKey: view.panelKey ?? from }
  })
  navigateToChatSession(to, isChatRouteCurrent(from) ? { replace: true } : undefined)
  return true
}

/** The session a quick launch placeholder became after it left the screen, if any. */
export function startedQuickLaunch(placeholderId: string): string | undefined {
  return startedOffScreen.get(placeholderId)
}

/** The quick chat whose view just closed, until its route is settled. */
let leavingSessionId: string | null = null

/** `true` while the view of `sessionId` has closed but its route is not settled yet. */
export function isQuickChatLeaving(sessionId: string): boolean {
  return leavingSessionId === sessionId
}

/** The quick view as it was before {@link closeQuickChatView} closed it. */
export type ClosedQuickChatView = Exclude<QuickChatView, { mode: 'closed' }>

/**
 * Close the quick view for a project view and return what it showed (`null`
 * when it was closed). A chat's route leaves `#/c/<id>` on the next task, and
 * only when nothing else changed it first. The launcher has no route.
 *
 * The router commits a navigation some microtasks later. A route change made
 * here at once and the caller's own change (Alt+T adds `#/c/<new>`, a project
 * pick goes to `#/`) would make two history entries, so Back would stop on an
 * extra `#/`. While the route still names the closed chat, `ChatRoute` checks
 * {@link isQuickChatLeaving} and does not open it again.
 */
export function closeQuickChatView(): ClosedQuickChatView | null {
  const view = useQuickChatStore.getState().view
  if (view.mode === 'closed') return null
  useQuickChatStore.getState().close()
  if (view.mode === 'launcher') return view
  const sessionId = view.sessionId
  leavingSessionId = sessionId
  setTimeout(() => {
    if (leavingSessionId === sessionId) leavingSessionId = null
    const now = useQuickChatStore.getState().view
    // Opened again in the meantime: its route is right.
    if (now.mode === 'chat' && now.sessionId === sessionId) return
    clearChatRoute(sessionId)
  }, 0)
  return view
}

/** What a closed quick view showed, for logs. */
export function describeQuickChatView(view: ClosedQuickChatView): string {
  return view.mode === 'launcher' ? 'the quick chat launcher' : `quick chat ${view.sessionId}`
}

/** The React key of the open quick chat's panel. */
export function quickChatPanelKey(view: Extract<QuickChatView, { mode: 'chat' }>): string {
  return view.panelKey ?? view.sessionId
}
