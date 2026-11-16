import { create } from 'zustand'

/** What the main area shows for a quick chat. */
export type QuickChatView =
  | { mode: 'closed' }
  | { mode: 'chat'; sessionId: string; readOnly?: boolean }

interface QuickChatState {
  view: QuickChatView
  openChat: (sessionId: string) => void
  /** An older no-project chat. History can be read. New prompts are refused. */
  openOlderChat: (sessionId: string) => void
  close: () => void
}

/**
 * The open quick chat. A quick chat has no project, so it does not live in a
 * project pane tree. Selecting a project or opening a project chat closes
 * this view (see `leaveQuickChatView` in `lib/no-project-chat`).
 */
export const useQuickChatStore = create<QuickChatState>((set) => ({
  view: { mode: 'closed' },
  openChat: (sessionId) => set({ view: { mode: 'chat', sessionId } }),
  openOlderChat: (sessionId) => set({ view: { mode: 'chat', sessionId, readOnly: true } }),
  close: () => set({ view: { mode: 'closed' } })
}))

/** `true` while a quick chat owns the main area. */
export function useQuickChatOpen(): boolean {
  return useQuickChatStore((state) => state.view.mode !== 'closed')
}
