import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { GUIDE_POPOVER_REOPEN_DELAY_MS, mountOverlay } from './entry'
import { OUTBOX_ATTR } from './overlay-transport'
import { computeGuideBadgeLayout } from './ruler-layout'

function dispatchPointer(
  target: EventTarget,
  type: string,
  init: { clientX?: number; clientY?: number; button?: number; shiftKey?: boolean } = {}
): void {
  const ev = new Event(type, { bubbles: true, cancelable: true }) as PointerEvent
  Object.defineProperty(ev, 'clientX', { value: init.clientX ?? 0 })
  Object.defineProperty(ev, 'clientY', { value: init.clientY ?? 0 })
  Object.defineProperty(ev, 'button', { value: init.button ?? 0 })
  Object.defineProperty(ev, 'shiftKey', { value: Boolean(init.shiftKey) })
  target.dispatchEvent(ev)
}

describe('computeGuideBadgeLayout', () => {
  it('staggers adjacent vertical and horizontal guide badges into non-overlapping lanes', () => {
    const layout = computeGuideBadgeLayout(
      [
        { id: 'v1', axis: 'vertical', pos: 369 },
        { id: 'v2', axis: 'vertical', pos: 390 },
        { id: 'h1', axis: 'horizontal', pos: 100 },
        { id: 'h2', axis: 'horizontal', pos: 112 }
      ],
      1280,
      800
    )

    expect(layout.get('v1')?.bottom).toBe(14)
    expect(layout.get('v2')?.bottom).toBe(38)
    expect(layout.get('h1')?.left).toBe(16)
    expect(layout.get('h2')?.left).toBe(132)
  })
})

describe('TroupeAnnotationOverlay', () => {
  let targetEl: HTMLHeadingElement

  beforeEach(() => {
    vi.useFakeTimers()
    document.body.innerHTML = ''
    targetEl = document.createElement('h1')
    targetEl.className = 'hero-title'
    targetEl.textContent = 'Bikin Absen Jadi Efisien'
    targetEl.getBoundingClientRect = () =>
      ({
        x: 200,
        y: 120,
        left: 200,
        top: 120,
        right: 800,
        bottom: 240,
        width: 600,
        height: 120,
        toJSON: () => ({})
      }) as DOMRect
    document.body.appendChild(targetEl)

    Object.defineProperty(document, 'elementFromPoint', {
      configurable: true,
      writable: true,
      value: vi.fn(() => targetEl)
    })
    vi.stubGlobal('requestAnimationFrame', (cb: FrameRequestCallback) => {
      cb(0)
      return 1
    })
    ;(window as unknown as Record<string, unknown>).__TERMUL_AGENTATION_NONCE__ = 'test-nonce'
    ;(window as unknown as Record<string, unknown>).__TERMUL_TAB_ID__ = 'tab-1'
  })

  afterEach(() => {
    const win = window as unknown as {
      __TROUPE_ANNOTATE__?: { unmount: () => void }
      __TERMUL_AGENTATION_NONCE__?: string
      __TERMUL_AGENTATION_ENDPOINT__?: string
      __TERMUL_TAB_ID__?: string
    }
    win.__TROUPE_ANNOTATE__?.unmount()
    delete win.__TERMUL_AGENTATION_NONCE__
    delete win.__TERMUL_AGENTATION_ENDPOINT__
    delete win.__TERMUL_TAB_ID__
    vi.useRealTimers()
    vi.restoreAllMocks()
    vi.unstubAllGlobals()
  })

  function setupOverlay() {
    const overlay = mountOverlay()
    overlay.dispatch({ type: 'set_active', active: true })
    const host = document.getElementById('termul-agentation-host') as HTMLDivElement
    const shadow = host.shadowRoot as ShadowRoot
    const layer = shadow.querySelector('.layer') as HTMLDivElement
    return { overlay, host, shadow, layer }
  }

  it('keeps selected element highlight visible while Comment popover is open until cancelled', () => {
    const { shadow, layer } = setupOverlay()

    dispatchPointer(layer, 'pointerdown', { clientX: 300, clientY: 160, button: 0 })
    const selectedBox = shadow.querySelector('.selected-box') as HTMLDivElement
    expect(shadow.querySelector('.popover')).toBeTruthy()
    expect(selectedBox.style.display).toBe('block')
    expect(selectedBox.style.left).toBe('200px')
    expect(selectedBox.style.width).toBe('600px')

    const cancelBtn = shadow.querySelector('.btn-sec') as HTMLButtonElement
    cancelBtn.click()
    expect(shadow.querySelector('.popover')).toBeNull()
    expect(selectedBox.style.display).toBe('none')
  })

  it('keeps drawn rectangle outline visible while Rectangle popover is open until cancelled', () => {
    const { overlay, shadow, layer } = setupOverlay()
    overlay.dispatch({ type: 'set_tool', tool: 'rectangle' })

    dispatchPointer(layer, 'pointerdown', { clientX: 100, clientY: 80, button: 0 })
    dispatchPointer(layer, 'pointermove', { clientX: 420, clientY: 280 })
    dispatchPointer(layer, 'pointerup', { clientX: 420, clientY: 280, button: 0 })

    const selectedBox = shadow.querySelector('.selected-box') as HTMLDivElement
    expect(shadow.querySelector('.popover')).toBeTruthy()
    expect(selectedBox.classList.contains('rect')).toBe(true)
    expect(selectedBox.style.display).toBe('block')
    expect(selectedBox.style.left).toBe('100px')
    expect(selectedBox.style.top).toBe('80px')
    expect(selectedBox.style.width).toBe('320px')
    expect(selectedBox.style.height).toBe('200px')
  })

  it('shows live ruler guide preview on hover without blue element highlight in Ruler mode', () => {
    const { overlay, shadow, layer } = setupOverlay()
    overlay.dispatch({ type: 'set_tool', tool: 'ruler' })

    dispatchPointer(layer, 'pointermove', { clientX: 424, clientY: 180, shiftKey: false })

    const hoverBox = shadow.querySelector('.hover-box') as HTMLDivElement
    const previewLine = shadow.querySelector('.guide-preview') as HTMLDivElement
    const previewBadge = shadow.querySelector('.guide-preview-badge') as HTMLDivElement

    expect(hoverBox.style.display).toBe('none')
    expect(previewLine.style.display).toBe('block')
    expect(previewLine.classList.contains('vertical')).toBe(true)
    expect(previewLine.style.left).toBe('424px')
    expect(previewBadge.style.display).toBe('block')
    expect(previewBadge.textContent).toContain('x 424px')

    // Holding Shift switches preview to horizontal immediately
    dispatchPointer(layer, 'pointermove', { clientX: 424, clientY: 180, shiftKey: true })
    expect(previewLine.classList.contains('horizontal')).toBe(true)
    expect(previewLine.style.top).toBe('180px')
    expect(previewBadge.textContent).toContain('y 180px')
  })

  it('auto-hides guide popover on drag, waits 2000ms after drag end, and resets timer on re-drag', () => {
    const { overlay, shadow, layer } = setupOverlay()
    overlay.dispatch({ type: 'set_tool', tool: 'ruler' })

    // Place vertical guide at x=400 -> popover opens
    dispatchPointer(layer, 'pointerdown', { clientX: 400, clientY: 200, button: 0 })
    expect(shadow.querySelector('.popover')).toBeTruthy()

    const guideHit = shadow.querySelector('.guide-hit.vertical') as HTMLDivElement
    expect(guideHit).toBeTruthy()
    expect(guideHit.querySelector('.guide-handle')).toBeTruthy()

    // Start dragging guide from 400 -> 460
    dispatchPointer(guideHit, 'pointerdown', { clientX: 400, clientY: 200, button: 0 })
    dispatchPointer(window, 'pointermove', { clientX: 460, clientY: 200 })

    // Comment popover immediately auto-hides while dragging
    expect(shadow.querySelector('.popover')).toBeNull()

    // Release drag
    dispatchPointer(window, 'pointerup', { clientX: 460, clientY: 200, button: 0 })
    expect(shadow.querySelector('.popover')).toBeNull()

    // After 1500ms, still hidden — start dragging again to verify timer reset
    vi.advanceTimersByTime(1500)
    expect(shadow.querySelector('.popover')).toBeNull()

    const refreshedHit = shadow.querySelector('.guide-hit.vertical') as HTMLDivElement
    dispatchPointer(refreshedHit, 'pointerdown', { clientX: 460, clientY: 200, button: 0 })
    dispatchPointer(window, 'pointermove', { clientX: 500, clientY: 200 })
    dispatchPointer(window, 'pointerup', { clientX: 500, clientY: 200, button: 0 })

    // 1999ms after second drag: still hidden
    vi.advanceTimersByTime(GUIDE_POPOVER_REOPEN_DELAY_MS - 1)
    expect(shadow.querySelector('.popover')).toBeNull()

    // At 2000ms: popover reopens with updated position x = 500px
    vi.advanceTimersByTime(1)
    const reopened = shadow.querySelector('.popover') as HTMLDivElement
    expect(reopened).toBeTruthy()
    expect(reopened.textContent).toContain('x = 500px')
  })

  it('hides BOTH annotations and ruler guides when set_pins_hidden is true', () => {
    const { overlay, shadow, layer } = setupOverlay()
    overlay.dispatch({ type: 'set_tool', tool: 'ruler' })
    dispatchPointer(layer, 'pointerdown', { clientX: 320, clientY: 180, button: 0 })
    expect(shadow.querySelectorAll('.guide-hit').length).toBe(1)
    expect(shadow.querySelectorAll('.guide-badge').length).toBe(1)

    overlay.dispatch({ type: 'set_pins_hidden', hidden: true })
    expect(shadow.querySelectorAll('.guide-hit').length).toBe(0)
    expect(shadow.querySelectorAll('.guide-badge').length).toBe(0)

    overlay.dispatch({ type: 'set_pins_hidden', hidden: false })
    expect(shadow.querySelectorAll('.guide-hit').length).toBe(1)
    expect(shadow.querySelectorAll('.guide-badge').length).toBe(1)
  })

  it('falls back to the host DOM outbox bridge when fetch fails and completes on outbox_ack', async () => {
    ;(window as unknown as Record<string, unknown>).__TERMUL_AGENTATION_ENDPOINT__ =
      'http://127.0.0.1:9999/secret'
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new TypeError('CSP blocked')))

    const { overlay, host, shadow, layer } = setupOverlay()
    dispatchPointer(layer, 'pointerdown', { clientX: 300, clientY: 160, button: 0 })

    const textarea = shadow.querySelector('.pop-input') as HTMLTextAreaElement
    textarea.value = 'Fix hero spacing'
    const addBtn = shadow.querySelector('.btn-pri') as HTMLButtonElement
    addBtn.click()

    await vi.waitFor(() => {
      expect(host.getAttribute(OUTBOX_ATTR)).toBeTruthy()
    })

    const queued = JSON.parse(host.getAttribute(OUTBOX_ATTR) as string) as Array<{
      reqId: string
      nonce: string
      kind: string
      payload: { comment: string }
    }>
    expect(queued).toHaveLength(1)
    expect(queued[0].nonce).toBe('test-nonce')
    expect(queued[0].kind).toBe('annotation')
    expect(queued[0].payload.comment).toBe('Fix hero spacing')

    overlay.dispatch({
      type: 'outbox_ack',
      reqId: queued[0].reqId,
      ok: true,
      id: 'ann-bridge-1',
      sessionId: 'sess-1'
    })

    await vi.waitFor(() => {
      expect(shadow.querySelector('.popover')).toBeNull()
      expect(shadow.querySelectorAll('.pin').length).toBe(1)
    })
  })
})
