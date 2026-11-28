/** What `src-tauri/src/comu_screen_overlay.rs` pushes into the indicator page. */
export interface ComuOverlayState {
  on: boolean
  label?: string
}

/** Apply `state` to the indicator root rendered by `comu-overlay.html`. */
export function applyComuOverlayState(root: HTMLElement, state: ComuOverlayState): void {
  if (typeof state.label === 'string') {
    const label = root.querySelector<HTMLElement>('[data-comu-overlay-label]')
    // textContent: the label carries a user-chosen agent name.
    if (label) label.textContent = state.label
  }
  root.dataset.state = state.on ? 'on' : 'off'
}
