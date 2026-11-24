import DOMPurify, { type DOMPurify as Purifier } from 'dompurify'

/** Strip width/height from the root `<svg>` tag only. */
export function normalizeRootIconSvg(svg: string): string {
  return svg.replace(/<svg\b[^>]*>/i, (tag) =>
    tag.replace(/\s+width="[^"]*"/g, '').replace(/\s+height="[^"]*"/g, '')
  )
}

function rootSvgOpeningTag(normalized: string): string | null {
  const match = normalized.match(/^\s*<svg\b[^>]*>/i)
  return match?.[0] ?? null
}

/**
 * Elements the svg profile keeps that an inlined icon must not carry: `<style>`
 * restyles the whole app, `<image>` and `<a>` reach the network, and
 * `<foreignObject>` embeds HTML. The HTML parser lowercases SVG tag names.
 */
const FORBIDDEN_ICON_TAGS = ['style', 'image', 'a', 'foreignobject', 'foreignObject', 'script']

const URL_REFERENCE = /url\(\s*(['"]?)\s*([^)'"\s]*)/gi

/** Every `url(...)` in the value points at a `#fragment` in the icon itself. */
function urlsStayLocal(value: string): boolean {
  for (const match of value.matchAll(URL_REFERENCE)) {
    if (!match[2]?.startsWith('#')) return false
  }
  return true
}

/**
 * Attribute values that could load a remote resource once inlined: an href
 * outside the icon, a `url()` outside the icon, or a CSS escape (`\75 rl(`)
 * that spells one without matching the text checks.
 */
function isAllowedIconAttribute(name: string, value: string): boolean {
  if (name === 'href' || name === 'xlink:href') return value.trim().startsWith('#')
  if (value.includes('\\')) return false
  return urlsStayLocal(value)
}

let iconPurifier: Purifier | null = null

/** A DOMPurify instance of its own, so the attribute hook never reaches other callers. */
function getIconPurifier(): Purifier {
  if (iconPurifier) return iconPurifier
  const purifier = DOMPurify(window)
  purifier.addHook('uponSanitizeAttribute', (_node, data) => {
    if (!isAllowedIconAttribute(data.attrName, data.attrValue)) data.keepAttr = false
  })
  iconPurifier = purifier
  return purifier
}

/** Sanitize inline agent SVG before rendering with dangerouslySetInnerHTML. */
export function sanitizeInlineAgentSvg(svg: string): string | null {
  const cleaned = getIconPurifier().sanitize(svg, {
    USE_PROFILES: { svg: true, svgFilters: true },
    FORBID_TAGS: FORBIDDEN_ICON_TAGS
  })
  const normalized = normalizeRootIconSvg(cleaned)
  const rootTag = rootSvgOpeningTag(normalized)
  if (!rootTag) return null
  if (!/\bviewBox\s*=/i.test(rootTag)) return null
  return normalized
}
