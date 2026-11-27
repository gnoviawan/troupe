import type { ElementSummary } from './dom-inspect'
import type { GuideAxis } from './ruler-layout'

export interface PopoverGuideRef {
  id: string
  axis: GuideAxis
  pos: number
  deltaPx?: number
  targetSelector?: string
  draftComment?: string
}

export interface CommentPopoverOptions {
  title: string
  x: number
  y: number
  kind: 'feedback' | 'rectangle' | 'guide' | 'measurement'
  summary?: ElementSummary
  rect?: { x: number; y: number; width: number; height: number }
  viewportWidth?: number
  guide?: PopoverGuideRef
  guideGapPx?: number
  measurement?: { axis: GuideAxis; positionPx: number; gapPx: number }
}

export interface CommentPopoverCallbacks {
  onCancel: () => void
  onRemoveGuide: (guideId: string) => void
  onSwitchToSpacing: (opts: CommentPopoverOptions) => void
  onSubmit: (
    payload: Record<string, unknown>,
    meta: { comment: string; pinX: number; pinY: number }
  ) => Promise<{ ok: boolean; id?: string }>
  onHideSelectionForCapture: () => boolean
  onRestoreSelectionAfterError: (pop: HTMLDivElement, hadSelected: boolean) => void
  onSaved: (
    pop: HTMLDivElement,
    id: string | undefined,
    comment: string,
    pinX: number,
    pinY: number
  ) => void
}

export function createCommentPopover(
  opts: CommentPopoverOptions,
  callbacks: CommentPopoverCallbacks
): { pop: HTMLDivElement; textarea: HTMLTextAreaElement } {
  const pop = document.createElement('div')
  pop.className = 'popover'
  const popWidth = Math.min(332, Math.max(240, window.innerWidth - 24))
  const left = Math.max(12, Math.min(window.innerWidth - popWidth - 12, opts.x + 14))
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
  if (opts.guide?.draftComment) {
    textarea.value = opts.guide.draftComment
  }
  textarea.placeholder =
    opts.kind === 'guide'
      ? `Describe alignment change at ${opts.title}…`
      : 'What should change on this element…'

  const foot = document.createElement('div')
  foot.className = 'pop-foot'
  if (opts.guide) {
    const guideRef = opts.guide
    const links = document.createElement('div')
    links.className = 'pop-links'
    const removeBtn = document.createElement('button')
    removeBtn.type = 'button'
    removeBtn.className = 'pop-link'
    removeBtn.textContent = 'Remove guide'
    removeBtn.addEventListener('click', () => callbacks.onRemoveGuide(guideRef.id))
    links.appendChild(removeBtn)

    if (typeof opts.guideGapPx === 'number' && opts.guideGapPx > 0) {
      const gapPx = opts.guideGapPx
      const measureBtn = document.createElement('button')
      measureBtn.type = 'button'
      measureBtn.className = 'pop-link measure'
      measureBtn.textContent = `Save spacing (${gapPx}px)`
      measureBtn.addEventListener('click', () => {
        callbacks.onSwitchToSpacing({
          title: `Spacing · ${gapPx}px (${guideRef.axis})`,
          x: opts.x,
          y: opts.y,
          kind: 'measurement',
          measurement: { axis: guideRef.axis, positionPx: guideRef.pos, gapPx }
        })
      })
      links.appendChild(measureBtn)
    }
    foot.appendChild(links)
  } else {
    const hint = document.createElement('span')
    hint.className = 'pop-hint'
    const isMacOs = /mac/i.test(navigator.platform || navigator.userAgent)
    hint.textContent = isMacOs ? '⌘Enter to add' : 'Ctrl+Enter to add'
    foot.appendChild(hint)
  }

  const errorEl = document.createElement('div')
  errorEl.className = 'pop-error'
  errorEl.style.display = 'none'

  const actions = document.createElement('div')
  actions.className = 'pop-actions'
  const cancelBtn = document.createElement('button')
  cancelBtn.type = 'button'
  cancelBtn.className = 'btn btn-sec'
  cancelBtn.textContent = 'Cancel'
  cancelBtn.addEventListener('click', () => callbacks.onCancel())

  const addBtn = document.createElement('button')
  addBtn.type = 'button'
  addBtn.className = 'btn btn-pri'
  addBtn.textContent = 'Add comment'

  let submitting = false
  const submit = async () => {
    if (submitting) return
    const comment = textarea.value.trim()
    if (!comment) return
    submitting = true
    addBtn.disabled = true
    addBtn.textContent = 'Saving…'
    errorEl.style.display = 'none'

    const pinX = Math.round(opts.x)
    const pinY = Math.round(opts.y)
    const payload: Record<string, unknown> = {
      x: pinX,
      y: pinY,
      comment,
      element: opts.summary?.element ?? opts.title,
      elementPath: opts.summary?.elementPath ?? opts.guide?.targetSelector ?? opts.title,
      fullPath: opts.summary?.fullPath,
      cssClasses: opts.summary?.cssClasses,
      nearbyText: opts.summary?.nearbyText,
      computedStyles: opts.summary?.computedStyles,
      reactComponents: opts.summary?.reactComponents,
      boundingBox: opts.rect ?? opts.summary?.boundingBox,
      viewportWidth: opts.viewportWidth ?? window.innerWidth,
      kind: opts.kind
    }
    if (opts.guide) {
      payload.measurement = {
        axis: opts.guide.axis,
        positionPx: opts.guide.pos,
        toSelector: opts.guide.targetSelector,
        deltaPx: opts.guide.deltaPx
      }
    } else if (opts.measurement) {
      payload.measurement = {
        axis: opts.measurement.axis,
        positionPx: opts.measurement.positionPx,
        toSelector: opts.summary?.elementPath,
        gapPx: opts.measurement.gapPx
      }
    }

    const hadSelected = callbacks.onHideSelectionForCapture()
    pop.style.display = 'none'
    const saved = await callbacks.onSubmit(payload, { comment, pinX, pinY })
    submitting = false
    if (!saved.ok) {
      if (!pop.isConnected) return
      pop.style.display = 'flex'
      callbacks.onRestoreSelectionAfterError(pop, hadSelected)
      addBtn.disabled = false
      addBtn.textContent = 'Retry'
      errorEl.textContent = 'Could not save annotation. Try again.'
      errorEl.style.display = 'block'
      return
    }
    callbacks.onSaved(pop, saved.id, comment, pinX, pinY)
  }

  addBtn.addEventListener('click', () => void submit())
  textarea.addEventListener('keydown', (ev) => {
    if ((ev.metaKey || ev.ctrlKey) && ev.key === 'Enter') {
      ev.preventDefault()
      void submit()
    }
  })

  actions.append(cancelBtn, addBtn)
  foot.appendChild(actions)
  pop.append(head, textarea, errorEl, foot)

  return { pop, textarea }
}
