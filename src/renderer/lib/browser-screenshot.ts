import type { PendingAnnotationItem } from '@/stores/browser-session-store'

export interface CropRect {
  x: number
  y: number
  width: number
  height: number
  dpr?: number
}

/**
 * Crop a full-viewport PNG (base64) to a sub-rectangle using an offscreen canvas.
 * Returns null if decoding fails or Canvas 2D is unavailable so callers never
 * leak an uncropped full-viewport image in place of a requested area crop.
 */
export async function cropBase64Png(
  base64: string,
  rect: CropRect
): Promise<{ base64: string; dataUrl: string } | null> {
  const dataUrl = `data:image/png;base64,${base64}`
  if (typeof document === 'undefined') {
    return null
  }
  return new Promise((resolve) => {
    const img = new Image()
    img.onload = () => {
      const dpr = rect.dpr && rect.dpr > 0 ? rect.dpr : 1
      const rawX = Math.round(rect.x * dpr)
      const rawY = Math.round(rect.y * dpr)
      const sx = Math.max(0, rawX)
      const sy = Math.max(0, rawY)
      const sw = Math.max(
        1,
        Math.min(img.width - sx, Math.round(rect.width * dpr) + Math.min(0, rawX))
      )
      const sh = Math.max(
        1,
        Math.min(img.height - sy, Math.round(rect.height * dpr) + Math.min(0, rawY))
      )
      const canvas = document.createElement('canvas')
      canvas.width = sw
      canvas.height = sh
      const ctx = canvas.getContext('2d')
      if (!ctx) {
        resolve(null)
        return
      }
      ctx.drawImage(img, sx, sy, sw, sh, 0, 0, sw, sh)
      const croppedUrl = canvas.toDataURL('image/png')
      const comma = croppedUrl.indexOf(',')
      if (comma < 0) {
        resolve(null)
        return
      }
      resolve({
        base64: croppedUrl.slice(comma + 1),
        dataUrl: croppedUrl
      })
    }
    img.onerror = () => resolve(null)
    img.src = dataUrl
  })
}

/**
 * Format pending batch annotations (including ruler guides & measurements) into
 * concise, structured markdown ready for an AI coding agent in chat.
 */
export function formatBatchAnnotationsForChat(url: string, items: PendingAnnotationItem[]): string {
  const lines: string[] = [
    `Page annotations (${items.length} ${items.length === 1 ? 'item' : 'items'}) on ${url || 'current page'}:`,
    ''
  ]
  for (const item of items) {
    const kindTag =
      item.kind === 'guide'
        ? `[Guide ${item.axis ?? ''} ${item.positionPx !== undefined ? `${item.positionPx}px` : ''}]`
        : item.kind === 'measurement'
          ? '[Ruler]'
          : item.kind === 'rectangle'
            ? '[Area]'
            : '[Element]'
    const deltaNote =
      typeof item.deltaPx === 'number' && item.deltaPx !== 0
        ? ` (${item.deltaPx > 0 ? `+${item.deltaPx}` : item.deltaPx}px off)`
        : ''
    lines.push(`${item.num}. ${kindTag} \`${item.elementPath || item.element}\`${deltaNote}`)
    lines.push(`   ${item.comment}`)
  }
  return lines.join('\n')
}
