import { describe, expect, it } from 'vitest'
import { applyComuOverlayState } from './state'

function overlayRoot(): HTMLElement {
  const root = document.createElement('div')
  root.dataset.state = 'off'
  root.innerHTML = '<div role="status"><span data-comu-overlay-label></span></div>'
  return root
}

describe('applyComuOverlayState', () => {
  it('turns the indicator on with the agent label', () => {
    const root = overlayRoot()
    applyComuOverlayState(root, { on: true, label: 'Claude is using this computer' })
    expect(root.dataset.state).toBe('on')
    expect(root.querySelector('[data-comu-overlay-label]')?.textContent).toBe(
      'Claude is using this computer'
    )
  })

  it('keeps the last label while fading out', () => {
    const root = overlayRoot()
    applyComuOverlayState(root, { on: true, label: 'Codex is using this computer' })
    applyComuOverlayState(root, { on: false })
    expect(root.dataset.state).toBe('off')
    expect(root.querySelector('[data-comu-overlay-label]')?.textContent).toBe(
      'Codex is using this computer'
    )
  })

  it('renders a name with markup as text', () => {
    const root = overlayRoot()
    applyComuOverlayState(root, { on: true, label: '<img src=x> is using this computer' })
    expect(root.querySelector('img')).toBeNull()
  })
})
