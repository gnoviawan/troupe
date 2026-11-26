/**
 * Troupe host-controlled browser annotation overlay (Shadow DOM).
 *
 * Replaces the stock floating bottom-right Agentation dock with an unobtrusive
 * overlay controlled exclusively by Troupe's native `BrowserControls` toolbar
 * via `window.__TROUPE_ANNOTATE__.dispatch(cmd)`.
 *
 * Supported tools:
 * - comment: select DOM element + persistent outline + numbered pin + inline comment popover
 * - rectangle: drag bounding-box region + persistent outline + numbered box + inline comment popover
 * - ruler: live guide line preview + draggable red H/V symmetry guides (16px hit target + handle)
 * - screenshot-area: crosshair marquee drag emitting crop rect for clipboard capture
 */

import {
  type CommentPopoverOptions,
  createCommentPopover,
  type PopoverGuideRef
} from './comment-popover'
import {
  buildElementPath,
  buildShortLabel,
  findMisalignedNearGuide,
  inspectElement
} from './dom-inspect'
import { OVERLAY_CSS } from './overlay-styles'
import { type OutboxAckMessage, OverlayTransport } from './overlay-transport'
import {
  computeGuideBadgeLayout,
  findNearestGuideGap,
  formatGuideBadgeText,
  type GuideAxis
} from './ruler-layout'

type AnnotateTool = 'comment' | 'rectangle' | 'ruler' | 'screenshot-area'

interface OverlayCommand {
  type:
    | 'set_active'
    | 'set_tool'
    | 'set_pause'
    | 'set_pins_hidden'
    | 'place_guide'
    | 'clear'
    | 'outbox_ack'
  active?: boolean
  tool?: AnnotateTool
  paused?: boolean
  hidden?: boolean
  axis?: GuideAxis
  ids?: string[]
  reqId?: string
  ok?: boolean
  id?: string
  sessionId?: string
  error?: string
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

type PlacedGuide = PopoverGuideRef

const HOST_ID = 'termul-agentation-host'
const PAUSE_STYLE_ID = 'troupe-annotate-pause-animations'
export const GUIDE_POPOVER_REOPEN_DELAY_MS = 2000

class TroupeAnnotationOverlay {
  private host: HTMLDivElement
  private shadow: ShadowRoot
  private layer: HTMLDivElement
  private hoverBox: HTMLDivElement
  private selectedBox: HTMLDivElement
  private dragBox: HTMLDivElement
  private previewGuideLine: HTMLDivElement
  private previewGuideBadge: HTMLDivElement
  private pinsContainer: HTMLDivElement
  private guidesContainer: HTMLDivElement
  private popoverEl: HTMLDivElement | null = null
  private popoverGuideId: string | null = null
  private popoverTextarea: HTMLTextAreaElement | null = null
  private transport: OverlayTransport

  private active = false
  private tool: AnnotateTool = 'comment'
  private pinsHidden = false
  private pins: PlacedPin[] = []
  private guides: PlacedGuide[] = []
  private dragStart: { x: number; y: number } | null = null
  private lastPointer: { x: number; y: number } | null = null
  private shiftHeld = false
  private draggingGuideId: string | null = null
  private guideReopenTimer: number | null = null
  private activeGuideDragCleanup: (() => void) | null = null
  private onWindowKeyDown: ((e: KeyboardEvent) => void) | null = null
  private onWindowKeyUp: ((e: KeyboardEvent) => void) | null = null

  constructor() {
    const existing = document.getElementById(HOST_ID)
    if (existing) existing.remove()

    this.host = document.createElement('div')
    this.host.id = HOST_ID
    document.body.appendChild(this.host)
    this.shadow = this.host.attachShadow({ mode: 'open' })
    this.transport = new OverlayTransport(this.host)

    const style = document.createElement('style')
    style.textContent = OVERLAY_CSS
    this.shadow.appendChild(style)

    this.layer = document.createElement('div')
    this.layer.className = 'layer'
    this.shadow.appendChild(this.layer)

    this.hoverBox = document.createElement('div')
    this.hoverBox.className = 'hover-box'
    this.shadow.appendChild(this.hoverBox)

    this.selectedBox = document.createElement('div')
    this.selectedBox.className = 'selected-box'
    this.shadow.appendChild(this.selectedBox)

    this.dragBox = document.createElement('div')
    this.dragBox.className = 'drag-box'
    this.shadow.appendChild(this.dragBox)

    this.previewGuideLine = document.createElement('div')
    this.previewGuideLine.className = 'guide-preview vertical'
    this.shadow.appendChild(this.previewGuideLine)

    this.previewGuideBadge = document.createElement('div')
    this.previewGuideBadge.className = 'guide-preview-badge'
    this.shadow.appendChild(this.previewGuideBadge)

    this.guidesContainer = document.createElement('div')
    this.shadow.appendChild(this.guidesContainer)

    this.pinsContainer = document.createElement('div')
    this.shadow.appendChild(this.pinsContainer)

    this.host.style.display = 'none'
    this.bindEvents()
  }

  private hideHoverVisuals(): void {
    this.hoverBox.style.display = 'none'
  }

  private hideRulerPreview(): void {
    this.previewGuideLine.style.display = 'none'
    this.previewGuideBadge.style.display = 'none'
  }

  private showSelectedBox(
    rect: { x: number; y: number; width: number; height: number },
    isRectangle: boolean
  ): void {
    this.selectedBox.className = isRectangle ? 'selected-box rect' : 'selected-box'
    this.selectedBox.style.left = `${rect.x}px`
    this.selectedBox.style.top = `${rect.y}px`
    this.selectedBox.style.width = `${rect.width}px`
    this.selectedBox.style.height = `${rect.height}px`
    this.selectedBox.style.display = 'block'
  }

  private hideSelectedBox(): void {
    this.selectedBox.style.display = 'none'
  }

  private clearGuideReopenTimer(): void {
    if (this.guideReopenTimer !== null) {
      window.clearTimeout(this.guideReopenTimer)
      this.guideReopenTimer = null
    }
  }

  private updateRulerPreview(clientX: number, clientY: number, shiftKey: boolean): void {
    if (
      !this.active ||
      this.tool !== 'ruler' ||
      this.popoverEl ||
      this.draggingGuideId ||
      this.pinsHidden
    ) {
      this.hideRulerPreview()
      return
    }
    const axis: GuideAxis = shiftKey ? 'horizontal' : 'vertical'
    const pos = axis === 'vertical' ? Math.round(clientX) : Math.round(clientY)

    this.layer.className = axis === 'vertical' ? 'layer ruler-v' : 'layer ruler-h'
    this.previewGuideLine.className = `guide-preview ${axis}`
    this.previewGuideLine.style.display = 'block'
    if (axis === 'vertical') {
      this.previewGuideLine.style.left = `${pos}px`
      this.previewGuideLine.style.top = ''
    } else {
      this.previewGuideLine.style.top = `${pos}px`
      this.previewGuideLine.style.left = ''
    }

    const nearestGap = findNearestGuideGap(this.guides, axis, pos)
    const axisLabel = axis === 'vertical' ? `V · x ${pos}px` : `H · y ${pos}px`
    const deltaText = nearestGap !== null ? ` · Δ ${nearestGap}px` : ''
    const hintText = axis === 'vertical' ? ' · Shift: H' : ' · Release Shift: V'
    const text = `${axisLabel}${deltaText}${hintText}`
    this.previewGuideBadge.textContent = text
    this.previewGuideBadge.style.display = 'block'

    const badgeEstWidth = Math.max(150, Math.ceil(text.length * 6.8 + 18))
    const left = Math.max(8, Math.min(window.innerWidth - badgeEstWidth - 8, clientX + 14))
    const top = Math.max(8, Math.min(window.innerHeight - 30, clientY + 14))
    this.previewGuideBadge.style.left = `${left}px`
    this.previewGuideBadge.style.top = `${top}px`
  }

  public dispatch(cmd: OverlayCommand): void {
    switch (cmd.type) {
      case 'outbox_ack':
        this.transport.handleOutboxAck(cmd as OutboxAckMessage)
        break
      case 'set_active':
        this.transport.captureBridgeNonce()
        this.active = Boolean(cmd.active)
        this.clearGuideReopenTimer()
        if (!this.active) {
          this.activeGuideDragCleanup?.()
          this.closePopover()
          this.hideHoverVisuals()
          this.hideRulerPreview()
          this.dragStart = null
          this.dragBox.style.display = 'none'
          this.host.style.display = 'none'
        } else {
          this.host.style.display = ''
          this.renderPinsAndGuides()
        }
        this.syncLayerMode()
        break
      case 'set_tool':
        if (cmd.tool) this.tool = cmd.tool
        this.clearGuideReopenTimer()
        this.closePopover()
        this.hideHoverVisuals()
        this.hideRulerPreview()
        this.dragStart = null
        this.dragBox.style.display = 'none'
        this.syncLayerMode()
        break
      case 'set_pause':
        this.setAnimationsPaused(Boolean(cmd.paused))
        break
      case 'set_pins_hidden':
        this.pinsHidden = Boolean(cmd.hidden)
        if (this.pinsHidden) {
          this.clearGuideReopenTimer()
          this.hideHoverVisuals()
          this.hideRulerPreview()
        }
        this.renderPinsAndGuides()
        break
      case 'place_guide':
        if (cmd.axis) {
          const pos = cmd.axis === 'vertical' ? Math.round(window.innerWidth / 3) : 64
          this.addGuide(cmd.axis, pos)
        }
        break
      case 'clear':
        this.clearGuideReopenTimer()
        if (cmd.ids && cmd.ids.length > 0) {
          const drop = new Set(cmd.ids)
          this.pins = this.pins
            .filter((p) => !drop.has(p.id))
            .map((p, idx) => ({ ...p, num: idx + 1 }))
        } else {
          this.pins = []
          this.guides = []
          this.closePopover()
        }
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
    } else if (this.tool === 'ruler') {
      this.layer.classList.add(this.shiftHeld ? 'ruler-h' : 'ruler-v')
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
      this.lastPointer = { x: e.clientX, y: e.clientY }
      this.shiftHeld = e.shiftKey
      if (!this.active || this.popoverEl || this.draggingGuideId) return

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

      if (this.tool === 'ruler') {
        this.hideHoverVisuals()
        this.updateRulerPreview(e.clientX, e.clientY, e.shiftKey)
        return
      }

      this.hideRulerPreview()

      if (this.tool === 'screenshot-area' || this.tool === 'rectangle') {
        this.hideHoverVisuals()
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
    })

    this.layer.addEventListener('pointerleave', () => {
      this.hideHoverVisuals()
      this.hideRulerPreview()
    })

    this.layer.addEventListener('pointerdown', (e) => {
      if (!this.active || e.button !== 0) return
      if (this.tool === 'rectangle' || this.tool === 'screenshot-area') {
        this.dragStart = { x: e.clientX, y: e.clientY }
        this.dragBox.style.display = 'none'
        return
      }
      if (this.tool === 'ruler') {
        this.clearGuideReopenTimer()
        const axis: GuideAxis = e.shiftKey ? 'horizontal' : 'vertical'
        const pos = axis === 'vertical' ? Math.round(e.clientX) : Math.round(e.clientY)
        const guide = this.addGuide(axis, pos)
        this.hideRulerPreview()
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

      const viewportWidth = window.innerWidth
      const rect = {
        x: Math.round(left),
        y: Math.round(top),
        width: Math.round(width),
        height: Math.round(height)
      }
      if (this.tool === 'screenshot-area') {
        requestAnimationFrame(() => {
          void this.transport.postAction({
            kind: 'screenshot_area',
            rect,
            dpr: window.devicePixelRatio || 1,
            viewportWidth
          })
        })
        return
      }
      this.openCommentPopover({
        title: `Rectangle · ${rect.width}×${rect.height}px`,
        x: rect.x,
        y: rect.y,
        kind: 'rectangle',
        rect,
        viewportWidth
      })
    })

    this.onWindowKeyDown = (e: KeyboardEvent) => {
      if (!this.active) return
      if (e.key === 'Shift' && !this.shiftHeld) {
        this.shiftHeld = true
        if (this.tool === 'ruler' && this.lastPointer && !this.popoverEl && !this.draggingGuideId) {
          this.updateRulerPreview(this.lastPointer.x, this.lastPointer.y, true)
        }
      }
      if (e.key === 'Escape') {
        this.clearGuideReopenTimer()
        if (this.popoverEl) {
          this.closePopover()
          return
        }
        if (this.tool === 'screenshot-area') {
          this.dragStart = null
          this.dragBox.style.display = 'none'
          void this.transport.postAction({ kind: 'screenshot_area_cancelled' })
        }
        return
      }
      if (this.popoverEl) return
      if (this.tool === 'ruler' && (e.key === 'h' || e.key === 'H')) {
        this.addGuide('horizontal', Math.round(window.innerHeight / 4))
      } else if (this.tool === 'ruler' && (e.key === 'v' || e.key === 'V')) {
        this.addGuide('vertical', 32)
      }
    }

    this.onWindowKeyUp = (e: KeyboardEvent) => {
      if (e.key === 'Shift' && this.shiftHeld) {
        this.shiftHeld = false
        if (
          this.active &&
          this.tool === 'ruler' &&
          this.lastPointer &&
          !this.popoverEl &&
          !this.draggingGuideId
        ) {
          this.updateRulerPreview(this.lastPointer.x, this.lastPointer.y, false)
        }
      }
    }

    window.addEventListener('keydown', this.onWindowKeyDown)
    window.addEventListener('keyup', this.onWindowKeyUp)
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

  private startGuideDrag(
    guide: PlacedGuide,
    hitEl: HTMLDivElement,
    badgeEl: HTMLDivElement,
    downEv: PointerEvent
  ): void {
    if (downEv.button !== 0) return
    downEv.stopPropagation()
    downEv.preventDefault()

    this.activeGuideDragCleanup?.()
    this.clearGuideReopenTimer()

    const startClient = guide.axis === 'vertical' ? downEv.clientX : downEv.clientY
    const startPos = guide.pos
    let moved = false

    const onMove = (ev: PointerEvent) => {
      const currentClient = guide.axis === 'vertical' ? ev.clientX : ev.clientY
      const delta = currentClient - startClient
      if (!moved && Math.abs(delta) < 2) return
      if (!moved) {
        moved = true
        this.draggingGuideId = guide.id
        this.hideRulerPreview()
        this.clearGuideReopenTimer()
        this.closePopover(true)
        hitEl.classList.add('dragging')
        badgeEl.classList.add('dragging')
      }
      const maxPos = guide.axis === 'vertical' ? window.innerWidth : window.innerHeight
      const nextPos = Math.max(0, Math.min(maxPos, Math.round(startPos + delta)))
      guide.pos = nextPos
      if (guide.axis === 'vertical') {
        hitEl.style.left = `${nextPos}px`
        badgeEl.style.left = `${Math.max(8, Math.min(window.innerWidth - 130, nextPos + 8))}px`
      } else {
        hitEl.style.top = `${nextPos}px`
        badgeEl.style.top = `${Math.max(8, Math.min(window.innerHeight - 28, nextPos + 6))}px`
      }
      badgeEl.textContent = formatGuideBadgeText({ axis: guide.axis, pos: nextPos })
    }

    const cleanup = () => {
      window.removeEventListener('pointermove', onMove)
      window.removeEventListener('pointerup', onUp)
      window.removeEventListener('pointercancel', onUp)
      hitEl.classList.remove('dragging')
      badgeEl.classList.remove('dragging')
      if (this.draggingGuideId === guide.id) {
        this.draggingGuideId = null
      }
      if (this.activeGuideDragCleanup === cleanup) {
        this.activeGuideDragCleanup = null
      }
    }

    const onUp = (ev: PointerEvent) => {
      cleanup()
      if (!moved) {
        this.openGuidePopover(guide, ev.clientX, ev.clientY)
        return
      }
      const hit = findMisalignedNearGuide(guide.axis, guide.pos, HOST_ID)
      guide.deltaPx = hit?.deltaPx
      guide.targetSelector = hit?.selector
      this.renderPinsAndGuides()

      this.clearGuideReopenTimer()
      const reopenX = guide.axis === 'vertical' ? guide.pos : ev.clientX
      const reopenY = guide.axis === 'horizontal' ? guide.pos : ev.clientY
      const guideId = guide.id
      this.guideReopenTimer = window.setTimeout(() => {
        this.guideReopenTimer = null
        if (!this.active || this.pinsHidden || this.draggingGuideId) return
        const latest = this.guides.find((g) => g.id === guideId)
        if (!latest) return
        this.openGuidePopover(latest, reopenX, reopenY)
      }, GUIDE_POPOVER_REOPEN_DELAY_MS)
    }

    this.activeGuideDragCleanup = cleanup
    window.addEventListener('pointermove', onMove)
    window.addEventListener('pointerup', onUp)
    window.addEventListener('pointercancel', onUp)
  }

  private renderPinsAndGuides(): void {
    this.pinsContainer.innerHTML = ''
    this.guidesContainer.innerHTML = ''

    if (this.pinsHidden) return

    const badgeLayout = computeGuideBadgeLayout(this.guides, window.innerWidth, window.innerHeight)

    for (const guide of this.guides) {
      const hit = document.createElement('div')
      hit.className = `guide-hit ${guide.axis}`
      hit.dataset.guideId = guide.id
      if (guide.axis === 'vertical') {
        hit.style.left = `${guide.pos}px`
      } else {
        hit.style.top = `${guide.pos}px`
      }

      const line = document.createElement('div')
      line.className = 'guide-line'
      const handle = document.createElement('div')
      handle.className = 'guide-handle'
      for (let i = 0; i < 3; i += 1) {
        const dot = document.createElement('span')
        dot.className = 'guide-handle-dot'
        handle.appendChild(dot)
      }
      hit.append(line, handle)

      const badge = document.createElement('div')
      badge.className = `guide-badge ${guide.axis}`
      badge.dataset.guideId = guide.id
      const placement = badgeLayout.get(guide.id)
      badge.textContent = placement?.text ?? formatGuideBadgeText(guide)
      if (placement) {
        badge.style.left = `${placement.left}px`
        if (placement.bottom !== undefined) {
          badge.style.bottom = `${placement.bottom}px`
        } else if (placement.top !== undefined) {
          badge.style.top = `${placement.top}px`
        }
      }

      hit.addEventListener('pointerenter', () => this.hideRulerPreview())
      badge.addEventListener('pointerenter', () => this.hideRulerPreview())
      hit.addEventListener('pointerdown', (ev) => this.startGuideDrag(guide, hit, badge, ev))
      badge.addEventListener('pointerdown', (ev) => this.startGuideDrag(guide, hit, badge, ev))

      this.guidesContainer.append(hit, badge)
    }

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

  private closePopover(preserveDraft = false): void {
    if (preserveDraft && this.popoverGuideId && this.popoverTextarea) {
      const guide = this.guides.find((g) => g.id === this.popoverGuideId)
      if (guide) {
        guide.draftComment = this.popoverTextarea.value
      }
    }
    if (this.popoverEl) {
      this.popoverEl.remove()
      this.popoverEl = null
    }
    this.popoverGuideId = null
    this.popoverTextarea = null
    this.hideSelectedBox()
  }

  private openGuidePopover(guide: PlacedGuide, clickX: number, clickY: number): void {
    const label = `${guide.axis === 'vertical' ? 'Vertical' : 'Horizontal'} guide · ${
      guide.axis === 'vertical' ? 'x' : 'y'
    } = ${guide.pos}px`
    const nearestGap = findNearestGuideGap(this.guides, guide.axis, guide.pos, guide.id)
    this.openCommentPopover({
      title: label,
      x: guide.axis === 'vertical' ? guide.pos : clickX,
      y: guide.axis === 'horizontal' ? guide.pos : clickY,
      kind: 'guide',
      guide,
      guideGapPx: nearestGap ?? undefined
    })
  }

  private openCommentPopover(opts: CommentPopoverOptions): void {
    this.closePopover()
    this.hideHoverVisuals()
    this.hideRulerPreview()

    const selectionRect = opts.rect ?? opts.summary?.boundingBox
    if (selectionRect && selectionRect.width > 0 && selectionRect.height > 0) {
      this.showSelectedBox(selectionRect, opts.kind === 'rectangle')
    }

    const { pop, textarea } = createCommentPopover(opts, {
      onCancel: () => {
        this.clearGuideReopenTimer()
        this.closePopover()
      },
      onRemoveGuide: (guideId) => {
        this.clearGuideReopenTimer()
        this.guides = this.guides.filter((g) => g.id !== guideId)
        this.closePopover()
        this.renderPinsAndGuides()
      },
      onSwitchToSpacing: (nextOpts) => this.openCommentPopover(nextOpts),
      onHideSelectionForCapture: () => {
        const had = this.selectedBox.style.display !== 'none'
        this.selectedBox.style.display = 'none'
        return had
      },
      onRestoreSelectionAfterError: (had) => {
        if (had) this.selectedBox.style.display = 'block'
      },
      onSubmit: (payload) => this.transport.postAnnotation(payload),
      onSaved: (popoverEl, id, comment, pinX, pinY) => {
        const num = this.pins.length + 1
        this.pins.push({
          id: id ?? `pin-${Date.now()}-${num}`,
          num,
          kind: opts.kind,
          x: pinX,
          y: pinY,
          rect: opts.rect,
          comment,
          element: opts.summary?.element ?? opts.title
        })
        if (opts.guide) {
          opts.guide.draftComment = undefined
        }
        if (this.popoverEl === popoverEl) this.closePopover()
        this.renderPinsAndGuides()
      }
    })

    this.shadow.appendChild(pop)
    this.popoverEl = pop
    this.popoverGuideId = opts.guide?.id ?? null
    this.popoverTextarea = textarea
    textarea.focus()
    requestAnimationFrame(() => textarea.focus())
  }

  public isConnected(): boolean {
    return this.host.isConnected
  }

  public destroy(): void {
    this.clearGuideReopenTimer()
    this.activeGuideDragCleanup?.()
    this.transport.destroy()
    this.setAnimationsPaused(false)
    if (this.onWindowKeyDown) {
      window.removeEventListener('keydown', this.onWindowKeyDown)
      this.onWindowKeyDown = null
    }
    if (this.onWindowKeyUp) {
      window.removeEventListener('keyup', this.onWindowKeyUp)
      this.onWindowKeyUp = null
    }
    this.host.remove()
    const win = window as unknown as {
      __TROUPE_ANNOTATE__?: unknown
      __TROUPE_ANNOTATE_INSTANCE__?: unknown
    }
    if (win.__TROUPE_ANNOTATE_INSTANCE__ === this) {
      delete win.__TROUPE_ANNOTATE_INSTANCE__
      delete win.__TROUPE_ANNOTATE__
    }
  }
}

function mountOverlay(): TroupeAnnotationOverlay {
  const win = window as unknown as {
    __TROUPE_ANNOTATE__?: {
      dispatch: (cmd: OverlayCommand) => void
      unmount: () => void
    }
    __TROUPE_ANNOTATE_INSTANCE__?: TroupeAnnotationOverlay
    TermulAgentation?: {
      mount: () => void
      unmount: () => void
    }
  }
  const existing = win.__TROUPE_ANNOTATE_INSTANCE__
  if (win.__TROUPE_ANNOTATE__ && existing?.isConnected()) {
    return existing
  }
  const overlay = new TroupeAnnotationOverlay()
  win.__TROUPE_ANNOTATE_INSTANCE__ = overlay
  win.__TROUPE_ANNOTATE__ = {
    dispatch: (cmd) => overlay.dispatch(cmd),
    unmount: () => overlay.destroy()
  }
  win.TermulAgentation = {
    mount: () => {
      const live = mountOverlay()
      live.dispatch({ type: 'set_active', active: true })
    },
    unmount: () => win.__TROUPE_ANNOTATE_INSTANCE__?.destroy()
  }
  return overlay
}

function init(): void {
  if (typeof document === 'undefined') return
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', () => {
      mountOverlay()
    })
  } else {
    mountOverlay()
  }
}

init()

export { buildElementPath, buildShortLabel, mountOverlay, TroupeAnnotationOverlay }
