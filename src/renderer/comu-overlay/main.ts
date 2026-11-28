/**
 * Page inside each Computer Use screen-edge indicator window
 * (`src-tauri/src/comu_screen_overlay.rs`). It has no IPC: Rust sets the
 * first state before the page loads (`__comuOverlayState`) and pushes later
 * ones by calling `window.__comuOverlay`.
 */
import './overlay.css'
import { applyComuOverlayState, type ComuOverlayState } from './state'

declare global {
  interface Window {
    __comuOverlay?: (state: ComuOverlayState) => void
    __comuOverlayState?: ComuOverlayState
  }
}

const root = document.getElementById('comu-overlay')
if (root) {
  window.__comuOverlay = (state) => applyComuOverlayState(root, state)
  const initial = window.__comuOverlayState
  if (initial) {
    // Set the label now, and turn on after the resting "off" frame has
    // painted so the enter transition runs.
    applyComuOverlayState(root, { ...initial, on: false })
    requestAnimationFrame(() => {
      requestAnimationFrame(() => applyComuOverlayState(root, initial))
    })
  }
}
