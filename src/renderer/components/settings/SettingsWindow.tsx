import { AnimatePresence, motion, useReducedMotion } from 'framer-motion'
import { type ReactNode, useEffect, useRef } from 'react'
import { NativeWebviewOccluder } from '@/hooks/use-native-webview-occlusion'
import { setTrafficLightsHidden } from '@/lib/tauri-traffic-lights'
import { useShellLayoutStore } from '@/stores/shell-layout-store'

const FOCUSABLE_SELECTOR =
  'a[href],button:not([disabled]),input:not([disabled]),select:not([disabled]),textarea:not([disabled]),[tabindex]:not([tabindex="-1"])'

/** Strong ease-out for anything the user caused. */
const EASE_OUT = [0.23, 1, 0.32, 1] as const

/** Controls the user can reach. Hidden pages stay mounted, so skip them. */
function renderedFocusables(root: HTMLElement): HTMLElement[] {
  return Array.from(root.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR)).filter(
    (el) => el.getClientRects().length > 0
  )
}

interface SettingsWindowProps {
  /** Whether the window is visible. */
  isOpen: boolean
  /**
   * Close handler for Esc (pass it to `SettingsLayout` for Back to app too).
   * The caller may gate this behind an unsaved-changes confirm.
   */
  onClose: () => void
  /** Accessible name of the window. The visible title lives in `SettingsLayout`. */
  title: string
  /** Body content: the `SettingsLayout` and its pages. */
  children: ReactNode
}

/**
 * Full-window shell for App Preferences and Project settings. It covers the
 * whole app window above the workspace, which stays mounted under it, so
 * "Back to app" returns to the same place. Esc closes it unless a sibling
 * confirm is open or a shortcut is being recorded.
 *
 * Native browser webviews draw above the DOM, so they are hidden while the
 * window is open. On macOS the traffic lights show even when the Rail is
 * collapsed, and come back to the Rail's state on close.
 */
export function SettingsWindow({
  isOpen,
  onClose,
  title,
  children
}: SettingsWindowProps): React.JSX.Element {
  const windowRef = useRef<HTMLDivElement>(null)
  const previouslyFocusedRef = useRef<HTMLElement | null>(null)
  const reducedMotion = useReducedMotion() ?? false

  useEffect(() => {
    if (!isOpen) return
    previouslyFocusedRef.current =
      document.activeElement instanceof HTMLElement ? document.activeElement : null
    const id = requestAnimationFrame(() => windowRef.current?.focus())
    return () => {
      cancelAnimationFrame(id)
      previouslyFocusedRef.current?.focus()
    }
  }, [isOpen])

  // The settings rail has room for the lights; the Rail hides them while it
  // is collapsed, so show them here and give them back on close.
  useEffect(() => {
    if (!isOpen) return
    void setTrafficLightsHidden(false)
    return () => {
      void setTrafficLightsHidden(!useShellLayoutStore.getState().railExpanded)
    }
  }, [isOpen])

  // Escape closes the window. It defers to an open sibling dialog (the reset
  // or unsaved-changes confirm), which handles its own Escape, and to a
  // shortcut recorder that is recording, where Escape cancels the recording.
  // A recorder that is only mounted (the Shortcuts page stays mounted) does
  // not block Escape.
  useEffect(() => {
    if (!isOpen) return
    const handleEscape = (e: globalThis.KeyboardEvent): void => {
      if (e.key !== 'Escape') return
      if (e.defaultPrevented) return
      if (document.querySelector('[data-sibling-dialog]')) return
      if (document.querySelector('[data-shortcut-recorder][data-recording="true"]')) return
      e.preventDefault()
      onClose()
    }
    window.addEventListener('keydown', handleEscape)
    return () => window.removeEventListener('keydown', handleEscape)
  }, [isOpen, onClose])

  // Trap Tab within the window so focus doesn't leak into the workspace under it.
  useEffect(() => {
    if (!isOpen) return
    const handleTab = (e: globalThis.KeyboardEvent): void => {
      if (e.key !== 'Tab' || !windowRef.current) return
      const focusables = renderedFocusables(windowRef.current)
      if (focusables.length === 0) return
      const first = focusables[0]
      const last = focusables[focusables.length - 1]
      if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault()
        first.focus()
      } else if (e.shiftKey && document.activeElement === first) {
        e.preventDefault()
        last.focus()
      }
    }
    window.addEventListener('keydown', handleTab)
    return () => window.removeEventListener('keydown', handleTab)
  }, [isOpen])

  const rise = reducedMotion ? 0 : 8

  return (
    <AnimatePresence>
      {isOpen && (
        <motion.div
          ref={windowRef}
          tabIndex={-1}
          initial={{ opacity: 0, y: rise }}
          animate={{ opacity: 1, y: 0, transition: { duration: 0.2, ease: EASE_OUT } }}
          exit={{ opacity: 0, y: rise, transition: { duration: 0.15, ease: EASE_OUT } }}
          className="fixed inset-0 z-50 flex flex-col overflow-hidden bg-background focus:outline-none"
          role="dialog"
          aria-modal="true"
          aria-label={title}
          data-settings-window
        >
          <NativeWebviewOccluder />
          {children}
        </motion.div>
      )}
    </AnimatePresence>
  )
}
