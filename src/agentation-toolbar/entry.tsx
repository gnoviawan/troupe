/**
 * Troupe host-controlled browser annotation overlay (Shadow DOM).
 *
 * Replaces the stock floating bottom-right Agentation dock with an unobtrusive
 * overlay controlled exclusively by Troupe's native `BrowserControls` toolbar
 * via `window.__TROUPE_ANNOTATE__.dispatch(cmd)`.
 *
 * Supported tools:
 * - comment: select DOM element + numbered pin + inline comment popover
 * - rectangle: drag bounding-box region + numbered box + inline comment popover
 * - ruler: hover spacing measurements + red H/V symmetry guide lines + click-to-comment
 * - screenshot-area: crosshair marquee drag emitting crop rect for native Rust capture
 */

import {
  buildElementPath,
  buildShortLabel,
  type ElementSummary,
  findMisalignedNearGuide,
  inspectElement
} from './dom-inspect'
import { OVERLAY_CSS } from './overlay-styles'

type AnnotateTool = 'comment' | 'rectangle' | 'ruler' | 'screenshot-area'
type GuideAxis = 'horizontal' | 'vertical'

interface OverlayCommand {
  type: 'set_active' | 'set_tool' | 'set_pause' | 'set_pins_hidden' | 'place_guide' | 'clear'
  active?: boolean
  tool?: AnnotateTool
  paused?: boolean
  hidden?: boolean
  axis?: GuideAxis
}

interface PlacedPin {
  id: string
  num: number
  kind: 'feedback' | 'rectangle' | 'guide' | 'measurement'
  x: number
  y: number
  rect?: { x: number; y: number; width: number; height: number }
  comment: string
  element: string
}

interface PlacedGuide {
  id: string
  axis: GuideAxis
  pos: number
  deltaPx?: number
  targetSelector?: string
}

const HOST_ID = 'termul-agentation-host'
const PAUSE_STYLE_ID = 'troupe-annotate-pause-animations'

class TroupeAnnotationOverlay {
  private host: HTMLDivElement
  private shadow: ShadowRoot
  private layer: HTMLDivElement
  private hoverBox: HTMLDivElement
  private dragBox: HTMLDivElement
  private spacingPill: HTMLDivElement
  private spacingTopLine: HTMLDivElement
  private spacingLeftLine: HTMLDivElement
  private pinsContainer: HTMLDivElement
  private guidesContainer: HTMLDivElement
  private popoverEl: HTMLDivElement | null = null

  private active = false
  private tool: AnnotateTool = 'comment'
  private pinsHidden = false
  private pins: PlacedPin[] = []
  private guides: PlacedGuide[] = []
  private sessionId: string | null = null
  private dragStart: { x: number; y: number } | null = null

  constructor() {
    const existing = document.getElementById(HOST_ID)
    if (existing) existing.remove()

    this.host = document.createElement('div')
    this.host.id = HOST_ID
    document.body.appendChild(this.host)
    this.shadow = this.host.attachShadow({ mode: 'open' })

    const style = document.createElement('style')
    style.textContent = OVERLAY_CSS
    this.shadow.appendChild(style)

    this.layer = document.createElement('div')
    this.layer.className = 'layer'
    this.shadow.appendChild(this.layer)

    this.hoverBox = document.createElement('div')
    this.hoverBox.className = 'hover-box'
    this.shadow.appendChild(this.hoverBox)

    this.dragBox = document.createElement('div')
    this.dragBox.className = 'drag-box'
    this.shadow.appendChild(this.dragBox)

    this.spacingPill = document.createElement('div')
    this.spacingPill.className = 'spacing-pill'
    this.shadow.appendChild(this.spacingPill)

    this.spacingTopLine = document.createElement('div')
    this.spacingTopLine.className = 'spacing-line v'
    this.shadow.appendChild(this.spacingTopLine)

    this.spacingLeftLine = document.createElement('div')
    this.spacingLeftLine.className = 'spacing-line h'
    this.shadow.appendChild(this.spacingLeftLine)

    this.guidesContainer = document.createElement('div')
    this.shadow.appendChild(this.guidesContainer)

    this.pinsContainer = document.createElement('div')
    this.shadow.appendChild(this.pinsContainer)

    this.host.style.display = 'none'
    this.bindEvents()
  }

  private getEndpoint(): string | undefined {
    return (window as unknown as { __TERMUL_AGENTATION_ENDPOINT__?: string })
      .__TERMUL_AGENTATION_ENDPOINT__
  }

  private async ensureSession(): Promise<string | null> {
    if (this.sessionId) return this.sessionId
    const preset = (window as unknown as { __TERMUL_AGENTATION_SESSION_ID__?: string })
      .__TERMUL_AGENTATION_SESSION_ID__
    if (preset) {
      this.sessionId = preset
      return preset
    }
    const ep = this.getEndpoint()
    if (!ep) return null
    try {
      const res = await fetch(`${ep}/sessions`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ url: window.location.href })
      })
      if (!res.ok) return null
      const json = (await res.json()) as { id?: string }
      if (json.id) {
        this.sessionId = json.id
        return json.id
      }
    } catch {
      // Ignore network failure in standalone preview
    }
    return null
  }

  private async postAnnotation(payload: Record<string, unknown>): Promise<void> {
    const ep = this.getEndpoint()
    if (!ep) return
    const sid = await this.ensureSession()
    if (!sid) return
    try {
      await fetch(`${ep}/sessions/${sid}/annotations`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          url: window.location.href,
          timestamp: Date.now(),
          ...payload
        })
      })
    } catch {
      // Best effort sync; batch UI remains responsive
    }
  }

  private async postAction(payload: Record<string, unknown>): Promise<void> {
    const ep = this.getEndpoint()
    if (!ep) return
    const sid = await this.ensureSession()
    if (!sid) return
    try {
      await fetch(`${ep}/sessions/${sid}/action`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          sessionId: sid,
          annotations: [],
          output: JSON.stringify(payload),
          timestamp: new Date().toISOString()
        })
      })
    } catch {
      // Ignore
    }
  }

  private hideHoverVisuals(): void {
    this.hoverBox.style.display = 'none'
    this.spacingPill.style.display = 'none'
    this.spacingTopLine.style.display = 'none'
    this.spacingLeftLine.style.display = 'none'
  }

  public dispatch(cmd: OverlayCommand): void {
    switch (cmd.type) {
      case 'set_active':
        this.active = Boolean(cmd.active)
        if (!this.active) {
          this.closePopover()
          this.hideHoverVisuals()
          this.host.style.display = 'none'
        } else {
          this.host.style.display = ''
          this.renderPinsAndGuides()
        }
        this.syncLayerMode()
        break
      case 'set_tool':
        if (cmd.tool) this.tool = cmd.tool
        this.closePopover()
        this.hideHoverVisuals()
        this.syncLayerMode()
        break
      case 'set_pause':
        this.setAnimationsPaused(Boolean(cmd.paused))
        break
      case 'set_pins_hidden':
        this.pinsHidden = Boolean(cmd.hidden)
        this.renderPinsAndGuides()
        break
      case 'place_guide':
        if (cmd.axis) {
          const pos = cmd.axis === 'vertical' ? Math.round(window.innerWidth / 3) : 64
          this.addGuide(cmd.axis, pos)
        }
        break
      case 'clear':
        this.pins = []
        this.guides = []
        this.closePopover()
        this.renderPinsAndGuides()
        break
    }
  }

  private setAnimationsPaused(paused: boolean): void {
    const existing = document.getElementById(PAUSE_STYLE_ID)
    if (paused && !existing) {
      const style = document.createElement('style')
      style.id = PAUSE_STYLE_ID
      style.textContent =
        '*, *::before, *::after { animation-play-state: paused !important; transition-duration: 0.001ms !important; }'
      document.head.appendChild(style)
    } else if (!paused && existing) {
      existing.remove()
    }
  }

  private syncLayerMode(): void {
    this.layer.className = 'layer'
    if (!this.active) return
    if (this.tool === 'rectangle' || this.tool === 'screenshot-area') {
      this.layer.classList.add('capture')
    } else {
      this.layer.classList.add('comment-mode')
    }
  }

  private elementUnderPoint(x: number, y: number): Element | null {
    this.host.style.display = 'none'
    const el = document.elementFromPoint(x, y)
    this.host.style.display = ''
    if (!el || el === document.documentElement || el === document.body) return null
    return el
  }

  private bindEvents(): void {
    this.layer.addEventListener('pointermove', (e) => {
      if (!this.active || this.popoverEl) return
      if (this.dragStart && (this.tool === 'rectangle' || this.tool === 'screenshot-area')) {
        const left = Math.min(this.dragStart.x, e.clientX)
        const top = Math.min(this.dragStart.y, e.clientY)
        const width = Math.abs(e.clientX - this.dragStart.x)
        const height = Math.abs(e.clientY - this.dragStart.y)
        this.dragBox.style.display = 'block'
        this.dragBox.style.left = `${left}px`
        this.dragBox.style.top = `${top}px`
        this.dragBox.style.width = `${width}px`
        this.dragBox.style.height = `${height}px`
        return
      }
      const el = this.elementUnderPoint(e.clientX, e.clientY)
      if (!el) {
        this.hideHoverVisuals()
        return
      }
      const rect = el.getBoundingClientRect()
      this.hoverBox.style.display = 'block'
      this.hoverBox.style.left = `${rect.left}px`
      this.hoverBox.style.top = `${rect.top}px`
      this.hoverBox.style.width = `${rect.width}px`
      this.hoverBox.style.height = `${rect.height}px`

      if (this.tool === 'ruler') {
        const cx = Math.round(rect.left + rect.width / 2)
        const cy = Math.round(rect.top + rect.height / 2)
        const topGap = Math.max(0, Math.round(rect.top))
        const leftGap = Math.max(0, Math.round(rect.left))
        this.spacingTopLine.style.display = topGap > 0 ? 'block' : 'none'
        this.spacingTopLine.style.left = `${cx}px`
        this.spacingTopLine.style.top = '0px'
        this.spacingTopLine.style.height = `${topGap}px`
        this.spacingLeftLine.style.display = leftGap > 0 ? 'block' : 'none'
        this.spacingLeftLine.style.left = '0px'
        this.spacingLeftLine.style.top = `${cy}px`
        this.spacingLeftLine.style.width = `${leftGap}px`

        this.spacingPill.style.display = 'block'
        this.spacingPill.textContent = `${Math.round(rect.width)}×${Math.round(rect.height)}px · ↑${topGap} ←${leftGap}`
        this.spacingPill.style.left = `${Math.min(window.innerWidth - 200, Math.max(8, rect.left))}px`
        this.spacingPill.style.top = `${Math.max(8, rect.top - 24)}px`
      } else {
        this.spacingPill.style.display = 'none'
        this.spacingTopLine.style.display = 'none'
        this.spacingLeftLine.style.display = 'none'
      }
    })

    this.layer.addEventListener('pointerdown', (e) => {
      if (!this.active || e.button !== 0) return
      if (this.tool === 'rectangle' || this.tool === 'screenshot-area') {
        this.dragStart = { x: e.clientX, y: e.clientY }
        this.dragBox.style.display = 'none'
        return
      }
      if (this.tool === 'ruler') {
        const axis: GuideAxis = e.shiftKey ? 'horizontal' : 'vertical'
        const pos = axis === 'vertical' ? Math.round(e.clientX) : Math.round(e.clientY)
        const guide = this.addGuide(axis, pos)
        this.openGuidePopover(guide, e.clientX, e.clientY)
        return
      }
      if (this.tool === 'comment') {
        const el = this.elementUnderPoint(e.clientX, e.clientY)
        if (!el) return
        const summary = inspectElement(el)
        this.openCommentPopover({
          title: summary.element,
          x: e.clientX,
          y: e.clientY,
          kind: 'feedback',
          summary
        })
      }
    })

    this.layer.addEventListener('pointerup', (e) => {
      if (!this.dragStart) return
      const left = Math.min(this.dragStart.x, e.clientX)
      const top = Math.min(this.dragStart.y, e.clientY)
      const width = Math.abs(e.clientX - this.dragStart.x)
      const height = Math.abs(e.clientY - this.dragStart.y)
      this.dragStart = null
      this.dragBox.style.display = 'none'
      if (width < 8 || height < 8) return

      const rect = {
        x: Math.round(left),
        y: Math.round(top),
        width: Math.round(width),
        height: Math.round(height)
      }
      if (this.tool === 'screenshot-area') {
        void this.postAction({
          kind: 'screenshot_area',
          rect,
          dpr: window.devicePixelRatio || 1
        })
        return
      }
      this.openCommentPopover({
        title: `Rectangle · ${rect.width}×${rect.height}px`,
        x: rect.x,
        y: rect.y,
        kind: 'rectangle',
        rect
      })
    })

    window.addEventListener('keydown', (e) => {
      if (!this.active) return
      if (e.key === 'Escape') {
        this.closePopover()
        return
      }
      if (this.popoverEl) return
      if (this.tool === 'ruler' && (e.key === 'h' || e.key === 'H')) {
        this.addGuide('horizontal', Math.round(window.innerHeight / 4))
      } else if (this.tool === 'ruler' && (e.key === 'v' || e.key === 'V')) {
        this.addGuide('vertical', 32)
      }
    })
  }

  private addGuide(axis: GuideAxis, pos: number): PlacedGuide {
    const hit = findMisalignedNearGuide(axis, pos, HOST_ID)
    const guide: PlacedGuide = {
      id: `guide-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
      axis,
      pos,
      deltaPx: hit?.deltaPx,
      targetSelector: hit?.selector
    }
    this.guides.push(guide)
    this.renderPinsAndGuides()
    return guide
  }

  private renderPinsAndGuides(): void {
    this.pinsContainer.innerHTML = ''
    this.guidesContainer.innerHTML = ''

    for (const guide of this.guides) {
      const line = document.createElement('div')
      line.className = `guide ${guide.axis}`
      if (guide.axis === 'vertical') {
        line.style.left = `${guide.pos}px`
      } else {
        line.style.top = `${guide.pos}px`
      }
      line.addEventListener('click', (ev) => {
        ev.stopPropagation()
        this.openGuidePopover(guide, ev.clientX, ev.clientY)
      })
      this.guidesContainer.appendChild(line)

      const badge = document.createElement('div')
      badge.className = 'guide-badge'
      const prefix = guide.axis === 'vertical' ? `x ${guide.pos}px` : `y ${guide.pos}px`
      const delta =
        typeof guide.deltaPx === 'number' && guide.deltaPx !== 0
          ? ` · ${guide.deltaPx > 0 ? `+${guide.deltaPx}` : guide.deltaPx}px off`
          : ''
      badge.textContent = `${prefix}${delta}`
      if (guide.axis === 'vertical') {
        badge.style.left = `${Math.min(window.innerWidth - 130, guide.pos + 6)}px`
        badge.style.bottom = '14px'
      } else {
        badge.style.top = `${Math.min(window.innerHeight - 28, guide.pos + 6)}px`
        badge.style.left = '16px'
      }
      badge.addEventListener('click', (ev) => {
        ev.stopPropagation()
        this.openGuidePopover(guide, ev.clientX, ev.clientY)
      })
      this.guidesContainer.appendChild(badge)
    }

    if (this.pinsHidden) return

    for (const pin of this.pins) {
      if (pin.rect) {
        const box = document.createElement('div')
        box.className = 'saved-rect'
        box.style.left = `${pin.rect.x}px`
        box.style.top = `${pin.rect.y}px`
        box.style.width = `${pin.rect.width}px`
        box.style.height = `${pin.rect.height}px`
        this.pinsContainer.appendChild(box)
      }
      const marker = document.createElement('div')
      marker.className = pin.kind === 'guide' ? 'pin guide-pin' : 'pin'
      marker.textContent = String(pin.num)
      marker.title = `${pin.element}: ${pin.comment}`
      marker.style.left = `${pin.x}px`
      marker.style.top = `${pin.y}px`
      this.pinsContainer.appendChild(marker)
    }
  }

  private closePopover(): void {
    if (this.popoverEl) {
      this.popoverEl.remove()
      this.popoverEl = null
    }
  }

  private openGuidePopover(guide: PlacedGuide, clickX: number, clickY: number): void {
    const label = `${guide.axis === 'vertical' ? 'Vertical' : 'Horizontal'} guide · ${
      guide.axis === 'vertical' ? 'x' : 'y'
    } = ${guide.pos}px`
    this.openCommentPopover({
      title: label,
      x: guide.axis === 'vertical' ? guide.pos : clickX,
      y: guide.axis === 'horizontal' ? guide.pos : clickY,
      kind: 'guide',
      guide
    })
  }

  private openCommentPopover(opts: {
    title: string
    x: number
    y: number
    kind: 'feedback' | 'rectangle' | 'guide'
    summary?: ElementSummary
    rect?: { x: number; y: number; width: number; height: number }
    guide?: PlacedGuide
  }): void {
    this.closePopover()
    this.hideHoverVisuals()
    const pop = document.createElement('div')
    pop.className = 'popover'
    const left = Math.max(12, Math.min(window.innerWidth - 348, opts.x + 14))
    const top = Math.max(12, Math.min(window.innerHeight - 190, opts.y + 14))
    pop.style.left = `${left}px`
    pop.style.top = `${top}px`

    const head = document.createElement('div')
    head.className = 'pop-head'
    const title = document.createElement('span')
    title.className = 'pop-title'
    title.textContent = opts.title
    const pill = document.createElement('span')
    pill.className = 'pop-pill'
    pill.textContent = 'Auto-crop on add'
    head.append(title, pill)

    const textarea = document.createElement('textarea')
    textarea.className = 'pop-input'
    textarea.placeholder =
      opts.kind === 'guide'
        ? `Describe alignment change at ${opts.title}…`
        : 'What should change on this element…'

    const foot = document.createElement('div')
    foot.className = 'pop-foot'
    if (opts.guide) {
      const removeBtn = document.createElement('button')
      removeBtn.type = 'button'
      removeBtn.className = 'pop-link'
      removeBtn.textContent = 'Remove guide'
      removeBtn.addEventListener('click', () => {
        this.guides = this.guides.filter((g) => g.id !== opts.guide?.id)
        this.closePopover()
        this.renderPinsAndGuides()
      })
      foot.appendChild(removeBtn)
    } else {
      const hint = document.createElement('span')
      hint.className = 'pop-hint'
      const isMacOs = /mac/i.test(navigator.platform || navigator.userAgent)
      hint.textContent = isMacOs ? '⌘Enter to add' : 'Ctrl+Enter to add'
      foot.appendChild(hint)
    }

    const actions = document.createElement('div')
    actions.className = 'pop-actions'
    const cancelBtn = document.createElement('button')
    cancelBtn.type = 'button'
    cancelBtn.className = 'btn btn-sec'
    cancelBtn.textContent = 'Cancel'
    cancelBtn.addEventListener('click', () => this.closePopover())

    const addBtn = document.createElement('button')
    addBtn.type = 'button'
    addBtn.className = 'btn btn-pri'
    addBtn.textContent = 'Add comment'

    const submit = () => {
      const comment = textarea.value.trim()
      if (!comment) return
      const num = this.pins.length + 1
      const pin: PlacedPin = {
        id: `pin-${Date.now()}-${num}`,
        num,
        kind: opts.kind,
        x: Math.round(opts.x),
        y: Math.round(opts.y),
        rect: opts.rect,
        comment,
        element: opts.summary?.element ?? opts.title
      }
      this.pins.push(pin)
      this.closePopover()
      this.renderPinsAndGuides()

      const payload: Record<string, unknown> = {
        x: pin.x,
        y: pin.y,
        comment,
        element: opts.summary?.element ?? opts.title,
        elementPath: opts.summary?.elementPath ?? opts.guide?.targetSelector ?? opts.title,
        fullPath: opts.summary?.fullPath,
        cssClasses: opts.summary?.cssClasses,
        nearbyText: opts.summary?.nearbyText,
        computedStyles: opts.summary?.computedStyles,
        reactComponents: opts.summary?.reactComponents,
        boundingBox: opts.rect ?? opts.summary?.boundingBox,
        kind: opts.kind
      }
      if (opts.guide) {
        payload.measurement = {
          axis: opts.guide.axis,
          positionPx: opts.guide.pos,
          toSelector: opts.guide.targetSelector,
          deltaPx: opts.guide.deltaPx
        }
      }
      void this.postAnnotation(payload)
    }

    addBtn.addEventListener('click', submit)
    textarea.addEventListener('keydown', (ev) => {
      if ((ev.metaKey || ev.ctrlKey) && ev.key === 'Enter') {
        ev.preventDefault()
        submit()
      }
    })

    actions.append(cancelBtn, addBtn)
    foot.appendChild(actions)
    pop.append(head, textarea, foot)
    this.shadow.appendChild(pop)
    this.popoverEl = pop
    textarea.focus()
    requestAnimationFrame(() => textarea.focus())
  }

  public destroy(): void {
    this.setAnimationsPaused(false)
    this.host.remove()
  }
}

function mountOverlay(): TroupeAnnotationOverlay {
  const win = window as unknown as {
    __TROUPE_ANNOTATE__?: {
      dispatch: (cmd: OverlayCommand) => void
      unmount: () => void
    }
    TermulAgentation?: {
      mount: () => void
      unmount: () => void
    }
  }
  if (win.__TROUPE_ANNOTATE__) {
    return (win as unknown as { __TROUPE_ANNOTATE_INSTANCE__: TroupeAnnotationOverlay })
      .__TROUPE_ANNOTATE_INSTANCE__
  }
  const overlay = new TroupeAnnotationOverlay()
  ;(
    win as unknown as { __TROUPE_ANNOTATE_INSTANCE__: TroupeAnnotationOverlay }
  ).__TROUPE_ANNOTATE_INSTANCE__ = overlay
  win.__TROUPE_ANNOTATE__ = {
    dispatch: (cmd) => overlay.dispatch(cmd),
    unmount: () => overlay.destroy()
  }
  win.TermulAgentation = {
    mount: () => overlay.dispatch({ type: 'set_active', active: true }),
    unmount: () => overlay.destroy()
  }
  return overlay
}

function init(): void {
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', () => {
      mountOverlay()
    })
  } else {
    mountOverlay()
  }
}

init()

export { buildElementPath, buildShortLabel }
