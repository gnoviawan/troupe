/**
 * The working-copy pick of an empty chat (the context row under its
 * composer): Current checkout, an existing worktree, or a new worktree from a
 * base branch. Nothing moves while the user picks. The first send applies it
 * (`sendFromChatStart`): the still-empty chat starts again in the picked
 * folder, or in a new worktree made at that moment.
 *
 * Keyed by session id; a chat with no entry stays where it is. Never
 * persisted: the durable preference is `lastComposerOptions` (isolationMode,
 * baseBranch), written by the row like the launcher wrote it.
 */

import { create } from 'zustand'
import type { LaunchIsolationMode } from '@/components/agents/launcher/prepare-launch-worktree'

export interface ChatStartPick {
  isolationMode: LaunchIsolationMode
  /** `Worktree.id` for `existing`. */
  existingWorktreeId: string | null
  /** Base branch for `worktree`. */
  baseBranch: string | null
}

interface ChatStartPickState {
  picks: Record<string, ChatStartPick>
  setPick: (sessionId: string, pick: ChatStartPick) => void
  clearPick: (sessionId: string) => void
}

export const useChatStartPickStore = create<ChatStartPickState>((set) => ({
  picks: {},
  setPick: (sessionId, pick) => set((s) => ({ picks: { ...s.picks, [sessionId]: pick } })),
  clearPick: (sessionId) =>
    set((s) => {
      if (!(sessionId in s.picks)) return s
      const picks = { ...s.picks }
      delete picks[sessionId]
      return { picks }
    })
}))
