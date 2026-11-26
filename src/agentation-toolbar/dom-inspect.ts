/**
 * Lightweight DOM inspection utilities for the host-controlled Troupe browser
 * annotation overlay. Extracts selectors, React component names, computed
 * layout styles, and alignment deltas relative to red symmetry guides.
 */

export interface ElementSummary {
  element: string
  elementPath: string
  fullPath: string
  cssClasses?: string
  nearbyText?: string
  computedStyles?: string
  reactComponents?: string
  boundingBox: { x: number; y: number; width: number; height: number }
}

const RELEVANT_CSS_PROPS = [
  'display',
  'position',
  'width',
  'height',
  'padding',
  'margin',
  'gap',
  'font-size',
  'line-height',
  'font-weight',
  'border-radius'
] as const

export function buildShortLabel(el: Element): string {
  const tag = el.tagName.toLowerCase()
  if (el.id) return `${tag}#${el.id}`
  const cls =
    typeof el.className === 'string' ? el.className.trim().split(/\s+/).filter(Boolean) : []
  if (cls.length > 0) {
    return `${tag}.${cls.slice(0, 2).join('.')}`
  }
  return tag
}

export function buildElementPath(el: Element, maxDepth = 4): string {
  const parts: string[] = []
  let current: Element | null = el
  while (current && current !== document.body && parts.length < maxDepth) {
    parts.unshift(buildShortLabel(current))
    if (current.id) break
    current = current.parentElement
  }
  return parts.join(' > ') || el.tagName.toLowerCase()
}

export function extractReactComponents(el: Element): string | undefined {
  const fiberKey = Object.keys(el).find(
    (k) => k.startsWith('__reactFiber$') || k.startsWith('__reactInternalInstance$')
  )
  if (!fiberKey) return undefined
  const names: string[] = []
  let fiber = (el as unknown as Record<string, unknown>)[fiberKey] as
    | { type?: unknown; return?: unknown }
    | undefined
  let guard = 0
  while (fiber && names.length < 5 && guard < 40) {
    guard += 1
    const t = fiber.type as { displayName?: string; name?: string } | undefined
    const name =
      typeof t === 'function' || (typeof t === 'object' && t !== null)
        ? t.displayName || t.name
        : undefined
    if (name && /^[A-Z]/.test(name) && !names.includes(name)) {
      names.push(name)
    }
    fiber = fiber.return as typeof fiber
  }
  return names.length > 0 ? names.join(' < ') : undefined
}

export function extractComputedSummary(el: Element): string {
  const cs = window.getComputedStyle(el)
  const entries: string[] = []
  for (const prop of RELEVANT_CSS_PROPS) {
    const val = cs.getPropertyValue(prop)
    if (val && val !== 'normal' && val !== 'none' && val !== '0px' && val !== 'static') {
      entries.push(`${prop}: ${val}`)
    }
  }
  return entries.join('; ')
}

export function inspectElement(el: Element): ElementSummary {
  const rect = el.getBoundingClientRect()
  const text = (el.textContent ?? '').trim().replace(/\s+/g, ' ').slice(0, 120)
  const cssClasses =
    typeof el.className === 'string' && el.className.trim()
      ? el.className.trim().slice(0, 200)
      : undefined
  return {
    element: buildShortLabel(el),
    elementPath: buildElementPath(el, 4),
    fullPath: buildElementPath(el, 10),
    cssClasses,
    nearbyText: text || undefined,
    computedStyles: extractComputedSummary(el) || undefined,
    reactComponents: extractReactComponents(el),
    boundingBox: {
      x: Math.round(rect.x),
      y: Math.round(rect.y),
      width: Math.round(rect.width),
      height: Math.round(rect.height)
    }
  }
}

export interface GuideAlignmentHit {
  selector: string
  elementPath: string
  deltaPx: number
  rect: { x: number; y: number; width: number; height: number }
}

/**
 * Find the closest meaningful visible block near a symmetry guide axis that is
 * misaligned by 1..24px so the overlay can badge its offset delta (e.g. "+8px off").
 */
export function findMisalignedNearGuide(
  axis: 'horizontal' | 'vertical',
  positionPx: number,
  hostId: string
): GuideAlignmentHit | null {
  const candidates = document.querySelectorAll('main *, header *, section *, form *, article *')
  let best: GuideAlignmentHit | null = null
  let bestAbs = 25
  for (const el of Array.from(candidates).slice(0, 250)) {
    if (el.id === hostId || el.closest(`#${hostId}`)) continue
    const rect = el.getBoundingClientRect()
    if (rect.width < 16 || rect.height < 12) continue
    const edge = axis === 'vertical' ? rect.left : rect.top
    const delta = Math.round(edge - positionPx)
    const abs = Math.abs(delta)
    if (abs >= 1 && abs < bestAbs) {
      bestAbs = abs
      best = {
        selector: buildShortLabel(el),
        elementPath: buildElementPath(el, 4),
        deltaPx: delta,
        rect: {
          x: Math.round(rect.x),
          y: Math.round(rect.y),
          width: Math.round(rect.width),
          height: Math.round(rect.height)
        }
      }
    }
  }
  return best
}
