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

  it('reflows overflowing horizontal guide badges vertically on narrow 320px viewports without overlap', () => {
    const layout = computeGuideBadgeLayout(
      [
        { id: 'h1', axis: 'horizontal', pos: 64 },
        { id: 'h2', axis: 'horizontal', pos: 65 },
        { id: 'h3', axis: 'horizontal', pos: 66 },
        { id: 'h4', axis: 'horizontal', pos: 67 }
      ],
      320,
      640
    )

    const placements = ['h1', 'h2', 'h3', 'h4'].map((id) => layout.get(id)!)
    for (let i = 0; i < placements.length; i += 1) {
      for (let j = i + 1; j < placements.length; j += 1) {
        const a = placements[i]
        const b = placements[j]
        const sameColumn = Math.abs(a.left - b.left) < 68
        const verticalOverlap = Math.abs((a.top ?? 0) - (b.top ?? 0)) < 20
        expect(sameColumn && verticalOverlap).toBe(false)
      }
    }
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
    ;(window as unknown as Record<string, unknown>).__TERMUL_TAB_ID__ = 'tab-1'
  })

  afterEach(() => {
    const win = window as unknown as {
      __TROUPE_ANNOTATE__?: { unmount: () => void }
      __TERMUL_AGENTATION_ENDPOINT__?: string
      __TERMUL_TAB_ID__?: string
    }
    win.__TROUPE_ANNOTATE__?.unmount()
    delete win.__TERMUL_AGENTATION_ENDPOINT__
    delete win.__TERMUL_TAB_ID__
    vi.useRealTimers()
    vi.restoreAllMocks()
    vi.unstubAllGlobals()
  })

  function setupOverlay(bridgeNonce: string | null = 'test-nonce') {
    const overlay = mountOverlay(bridgeNonce)
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

  it('hides selectedBox, dragBox, and suppresses comment hoverBox when pinsHidden is true and restores selectedBox on show', () => {
    const { overlay, shadow, layer } = setupOverlay()
    const hoverBox = shadow.querySelector('.hover-box') as HTMLDivElement
    const selectedBox = shadow.querySelector('.selected-box') as HTMLDivElement

    dispatchPointer(layer, 'pointerdown', { clientX: 300, clientY: 160, button: 0 })
    expect(selectedBox.style.display).toBe('block')

    overlay.dispatch({ type: 'set_pins_hidden', hidden: true })
    expect(selectedBox.style.display).toBe('none')

    // Re-enabling visibility while popover is still open restores selectedBox
    overlay.dispatch({ type: 'set_pins_hidden', hidden: false })
    expect(selectedBox.style.display).toBe('block')

    // Close popover, hide visuals, then verify comment hover/click, rectangle, and ruler are gated while screenshot-area still works
    ;(shadow.querySelector('.btn-sec') as HTMLButtonElement).click()
    overlay.dispatch({ type: 'set_pins_hidden', hidden: true })
    dispatchPointer(layer, 'pointermove', { clientX: 300, clientY: 160 })
    expect(hoverBox.style.display).toBe('none')
    dispatchPointer(layer, 'pointerdown', { clientX: 300, clientY: 160, button: 0 })
    expect(shadow.querySelector('.popover')).toBeNull()

    overlay.dispatch({ type: 'set_tool', tool: 'ruler' })
    dispatchPointer(layer, 'pointerdown', { clientX: 320, clientY: 160, button: 0 })
    expect(shadow.querySelector('.popover')).toBeNull()

    overlay.dispatch({ type: 'set_tool', tool: 'screenshot-area' })
    dispatchPointer(layer, 'pointerdown', { clientX: 100, clientY: 100, button: 0 })
    dispatchPointer(layer, 'pointermove', { clientX: 250, clientY: 220 })
    expect((shadow.querySelector('.drag-box') as HTMLDivElement).style.display).toBe('block')
  })

  it('enables passthrough on guides and pins during screenshot-area and rectangle tools', () => {
    const { overlay, shadow } = setupOverlay()
    const guidesLayer = shadow.querySelector('.guides-layer') as HTMLDivElement
    const pinsLayer = shadow.querySelector('.pins-layer') as HTMLDivElement

    overlay.dispatch({ type: 'set_tool', tool: 'ruler' })
    expect(guidesLayer.classList.contains('passthrough')).toBe(false)

    overlay.dispatch({ type: 'set_tool', tool: 'screenshot-area' })
    expect(guidesLayer.classList.contains('passthrough')).toBe(true)
    expect(pinsLayer.classList.contains('passthrough')).toBe(true)

    overlay.dispatch({ type: 'set_tool', tool: 'rectangle' })
    expect(guidesLayer.classList.contains('passthrough')).toBe(true)

    overlay.dispatch({ type: 'set_tool', tool: 'comment' })
    expect(guidesLayer.classList.contains('passthrough')).toBe(false)
  })

  it('temporarily hides and restores overlay host on set_capture_hidden', () => {
    const { overlay, host } = setupOverlay()
    expect(host.style.display).toBe('')

    overlay.dispatch({ type: 'set_capture_hidden', hidden: true })
    expect(host.style.display).toBe('none')

    overlay.dispatch({ type: 'set_capture_hidden', hidden: false })
    expect(host.style.display).toBe('')
  })

  it('notifies host to remove saved annotations when removing a guide', async () => {
    const { overlay, host, shadow, layer } = setupOverlay()
    overlay.dispatch({ type: 'set_tool', tool: 'ruler' })

    // Place vertical guide and save a comment on it
    dispatchPointer(layer, 'pointerdown', { clientX: 350, clientY: 180, button: 0 })
    ;(shadow.querySelector('.pop-input') as HTMLTextAreaElement).value = 'Align edge'
    ;(shadow.querySelector('.btn-pri') as HTMLButtonElement).click()

    await vi.waitFor(() => {
      expect(host.getAttribute(OUTBOX_ATTR)).toBeTruthy()
    })
    const saveQueue = JSON.parse(host.getAttribute(OUTBOX_ATTR) as string) as Array<{
      reqId: string
    }>
    host.removeAttribute(OUTBOX_ATTR)
    overlay.dispatch({
      type: 'outbox_ack',
      reqId: saveQueue[0].reqId,
      ok: true,
      id: 'ann-guide-1',
      sessionId: 'sess-g1'
    })

    await vi.waitFor(() => {
      expect(shadow.querySelectorAll('.pin').length).toBe(1)
    })

    // Click guide to reopen popover and click "Remove guide"
    const guideHit = shadow.querySelector('.guide-hit') as HTMLDivElement
    dispatchPointer(guideHit, 'pointerdown', { clientX: 350, clientY: 180, button: 0 })
    dispatchPointer(window, 'pointerup', { clientX: 350, clientY: 180, button: 0 })
    const removeBtn = shadow.querySelector('.pop-link') as HTMLButtonElement
    expect(removeBtn.textContent).toBe('Remove guide')
    removeBtn.click()

    await vi.waitFor(() => {
      expect(host.getAttribute(OUTBOX_ATTR)).toBeTruthy()
    })
    const actionQueue = JSON.parse(host.getAttribute(OUTBOX_ATTR) as string) as Array<{
      kind: string
      payload: { output: string }
    }>
    expect(actionQueue[0].kind).toBe('action')
    expect(JSON.parse(actionQueue[0].payload.output)).toMatchObject({
      kind: 'annotations_removed',
      ids: ['ann-guide-1']
    })
    expect(shadow.querySelectorAll('.pin').length).toBe(0)
  })

  it('does not restore selectedBox after failed save when popover was closed with Escape', async () => {
    const { host, shadow, layer } = setupOverlay()
    const selectedBox = shadow.querySelector('.selected-box') as HTMLDivElement

    dispatchPointer(layer, 'pointerdown', { clientX: 300, clientY: 160, button: 0 })
    ;(shadow.querySelector('.pop-input') as HTMLTextAreaElement).value = 'Will time out'
    ;(shadow.querySelector('.btn-pri') as HTMLButtonElement).click()

    await vi.waitFor(() => {
      expect(host.getAttribute(OUTBOX_ATTR)).toBeTruthy()
    })

    // Close popover with Escape while save is in flight
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    expect(shadow.querySelector('.popover')).toBeNull()
    expect(selectedBox.style.display).toBe('none')

    // Advance past OUTBOX_TIMEOUT_MS so save fails
    await vi.advanceTimersByTimeAsync(6500)
    expect(selectedBox.style.display).toBe('none')
    expect(host.getAttribute(OUTBOX_ATTR)).toBeNull()
  })

  it('coalesces initial host-bridge submissions so both reuse the first acknowledged sessionId', async () => {
    const { overlay, host, shadow, layer } = setupOverlay()

    dispatchPointer(layer, 'pointerdown', { clientX: 300, clientY: 160, button: 0 })
    ;(shadow.querySelector('.pop-input') as HTMLTextAreaElement).value = 'First comment'
    ;(shadow.querySelector('.btn-pri') as HTMLButtonElement).click()

    await vi.waitFor(() => {
      expect(host.getAttribute(OUTBOX_ATTR)).toBeTruthy()
    })
    const firstQueue = JSON.parse(host.getAttribute(OUTBOX_ATTR) as string) as Array<{
      reqId: string
      sessionId?: string
    }>
    expect(firstQueue).toHaveLength(1)
    expect(firstQueue[0].sessionId).toBeUndefined()
    host.removeAttribute(OUTBOX_ATTR)

    // Press Escape and immediately submit a second comment before first ack arrives
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    dispatchPointer(layer, 'pointerdown', { clientX: 300, clientY: 160, button: 0 })
    ;(shadow.querySelector('.pop-input') as HTMLTextAreaElement).value = 'Second comment'
    ;(shadow.querySelector('.btn-pri') as HTMLButtonElement).click()

    // Second submission waits until first ack provides sessionId
    await Promise.resolve()
    expect(host.getAttribute(OUTBOX_ATTR)).toBeNull()

    overlay.dispatch({
      type: 'outbox_ack',
      reqId: firstQueue[0].reqId,
      ok: true,
      id: 'ann-1',
      sessionId: 'sess-shared'
    })

    await vi.waitFor(() => {
      expect(host.getAttribute(OUTBOX_ATTR)).toBeTruthy()
    })
    const secondQueue = JSON.parse(host.getAttribute(OUTBOX_ATTR) as string) as Array<{
      reqId: string
      sessionId?: string
    }>
    expect(secondQueue).toHaveLength(1)
    expect(secondQueue[0].sessionId).toBe('sess-shared')
  })

  it('skips HTTP fallback when a timed-out outbox envelope was already drained by the host', async () => {
    const fetchMock = vi.fn().mockResolvedValue({ ok: true, json: async () => ({ id: 'dup' }) })
    vi.stubGlobal('fetch', fetchMock)
    ;(window as unknown as Record<string, unknown>).__TERMUL_AGENTATION_ENDPOINT__ =
      'http://127.0.0.1:9999/secret'

    const origLocation = window.location
    Object.defineProperty(window, 'location', {
      configurable: true,
      value: { ...origLocation, protocol: 'https:', href: 'https://absenin.id/' }
    })

    try {
      const { host, shadow, layer } = setupOverlay()
      dispatchPointer(layer, 'pointerdown', { clientX: 300, clientY: 160, button: 0 })
      ;(shadow.querySelector('.pop-input') as HTMLTextAreaElement).value = 'Drained comment'
      ;(shadow.querySelector('.btn-pri') as HTMLButtonElement).click()

      await vi.waitFor(() => {
        expect(host.getAttribute(OUTBOX_ATTR)).toBeTruthy()
      })
      // Host drains the attribute, then ack is lost/slow until timeout
      host.removeAttribute(OUTBOX_ATTR)

      await vi.advanceTimersByTimeAsync(6500)
      expect(fetchMock).not.toHaveBeenCalled()
    } finally {
      Object.defineProperty(window, 'location', { configurable: true, value: origLocation })
    }
  })

  it('skips HTTP fallback when overlay is destroyed after host already drained the outbox', async () => {
    const fetchMock = vi.fn().mockResolvedValue({ ok: true, json: async () => ({ id: 'dup' }) })
    vi.stubGlobal('fetch', fetchMock)
    ;(window as unknown as Record<string, unknown>).__TERMUL_AGENTATION_ENDPOINT__ =
      'http://127.0.0.1:9999/secret'

    const origLocation = window.location
    Object.defineProperty(window, 'location', {
      configurable: true,
      value: { ...origLocation, protocol: 'https:', href: 'https://absenin.id/' }
    })

    try {
      const { overlay, host, shadow, layer } = setupOverlay()
      dispatchPointer(layer, 'pointerdown', { clientX: 300, clientY: 160, button: 0 })
      ;(shadow.querySelector('.pop-input') as HTMLTextAreaElement).value = 'Drained before unmount'
      ;(shadow.querySelector('.btn-pri') as HTMLButtonElement).click()

      await vi.waitFor(() => {
        expect(host.getAttribute(OUTBOX_ATTR)).toBeTruthy()
      })
      // Host drains the attribute before overlay unmounts
      host.removeAttribute(OUTBOX_ATTR)
      overlay.destroy()

      await Promise.resolve()
      await Promise.resolve()
      expect(fetchMock).not.toHaveBeenCalled()
    } finally {
      Object.defineProperty(window, 'location', { configurable: true, value: origLocation })
    }
  })
})
