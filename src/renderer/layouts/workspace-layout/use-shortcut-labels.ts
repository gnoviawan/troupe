import { useCallback } from 'react'
import { formatKeyForDisplay, useKeyboardShortcutsStore } from '@/stores/keyboard-shortcuts-store'

/** Resolved shortcut key lookups (custom key wins over the default). */
export function useShortcutLabels() {
  // Keyboard shortcuts
  const shortcuts = useKeyboardShortcutsStore((state) => state.shortcuts)

  // Display labels ("⌘T", "Ctrl+T"): the same formatter as the Settings page.
  const getShortcutLabel = useCallback(
    (id: string): string | undefined => {
      const shortcut = shortcuts[id]
      return shortcut
        ? formatKeyForDisplay(shortcut.customKey ?? shortcut.defaultKey) || undefined
        : undefined
    },
    [shortcuts]
  )

  const getProjectShortcutLabel = useCallback(
    (index: number): string | undefined => {
      const shortcut = shortcuts[`project-${index + 1}`]
      return shortcut
        ? formatKeyForDisplay(shortcut.customKey ?? shortcut.defaultKey) || undefined
        : undefined
    },
    [shortcuts]
  )

  // Helper to get active key for a shortcut
  const getActiveKey = useCallback(
    (id: string): string => {
      const shortcut = shortcuts[id]
      return shortcut?.customKey ?? shortcut?.defaultKey ?? ''
    },
    [shortcuts]
  )

  return { getShortcutLabel, getProjectShortcutLabel, getActiveKey }
}
