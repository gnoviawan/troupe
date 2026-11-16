import { useCallback, useEffect, useMemo } from 'react'
import { normalizePath } from '@/lib/working-copy'
import { useChangesSeenStore } from '@/stores/changes-seen-store'
import { useGitSheetStore } from '@/stores/git-sheet-store'
import { useLastColumnTab, useSidePanelVisible } from '@/stores/shell-layout-store'
import type { ChangedFile } from './use-chat-changed-files'

/** Sorted `path:added:removed:status` lines: equal while the files look the same. */
export function changesFingerprint(files: readonly ChangedFile[]): string {
  return files
    .map((file) => `${file.path}:${file.added}:${file.removed}:${file.status ?? ''}`)
    .sort()
    .join('\n')
}

/**
 * True while the Git view shows this folder's changes: the desktop Changes
 * tab in a visible side panel, or the phone Git sheet opened on `cwd`.
 */
function useGitViewShowing(projectId: string | null, cwd: string): boolean {
  const lastTab = useLastColumnTab(projectId ?? '')
  const sidePanelVisible = useSidePanelVisible()
  const sheetOnCwd = useGitSheetStore(
    (state) => state.open && state.cwd !== '' && normalizePath(state.cwd) === normalizePath(cwd)
  )
  return (projectId !== null && lastTab === 'changes' && sidePanelVisible) || sheetOnCwd
}

/**
 * The blue dot of the Changes segment. `hasNew` is true while the chat has
 * changed files the user has not looked at. `markSeen` records the files as
 * they are now (the caller runs it when the Changes panel opens). The Git view
 * showing this folder counts as looking, so the hook marks them seen itself.
 */
export function useChangesSeen({
  sessionId,
  cwd,
  projectId,
  files
}: {
  sessionId: string
  cwd: string
  projectId: string | null
  files: readonly ChangedFile[]
}): { hasNew: boolean; markSeen: () => void } {
  const fingerprint = useMemo(() => changesFingerprint(files), [files])
  const seenFingerprint = useChangesSeenStore((state) => state.seen[sessionId])
  const hasFiles = files.length > 0
  const hasNew = hasFiles && seenFingerprint !== fingerprint

  const markSeen = useCallback(() => {
    useChangesSeenStore.getState().markSeen(sessionId, fingerprint)
  }, [sessionId, fingerprint])

  const gitViewShowing = useGitViewShowing(projectId, cwd)
  useEffect(() => {
    if (gitViewShowing && hasNew) markSeen()
  }, [gitViewShowing, hasNew, markSeen])

  return { hasNew, markSeen }
}
