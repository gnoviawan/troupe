import { writeClipboardImagePng } from '@/lib/composer-attachments-io'
import { logFrontendError } from '@/lib/log-api'
import { isTauriContext } from '@/lib/tauri-runtime'
import type { PendingAnnotationItem } from '@/stores/browser-session-store'

export interface CropRect {
  x: number
  y: number
  width: number
  height: number
  dpr?: number
  /** Page CSS viewport width (`window.innerWidth`) from the injected overlay for zoom-accurate scaling. */
  viewportWidth?: number
}

/**
 * Compute pixel-aligned source crop rectangle for a given image size and rect.
 * Derives scale from `imgWidth / rect.viewportWidth` when `viewportWidth` is provided
 * so native WebView zoom and OS DPR are both accounted for accurately.
 */
export function computeCropSourceRect(
  imgWidth: number,
  imgHeight: number,
  rect: CropRect
): { sx: number; sy: number; sw: number; sh: number } {
  const scale =
    rect.viewportWidth && rect.viewportWidth > 0
      ? imgWidth / rect.viewportWidth
      : rect.dpr && rect.dpr > 0
        ? rect.dpr
        : 1
  const rawX = Math.round(rect.x * scale)
  const rawY = Math.round(rect.y * scale)
  const sx = Math.max(0, Math.min(imgWidth - 1, rawX))
  const sy = Math.max(0, Math.min(imgHeight - 1, rawY))
  const sw = Math.max(
    1,
    Math.min(imgWidth - sx, Math.round(rect.width * scale) + Math.min(0, rawX))
  )
  const sh = Math.max(
    1,
    Math.min(imgHeight - sy, Math.round(rect.height * scale) + Math.min(0, rawY))
  )
  return { sx, sy, sw, sh }
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
      const { sx, sy, sw, sh } = computeCropSourceRect(img.width, img.height, rect)
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

function decodeBase64Png(base64Png: string): Uint8Array | null {
  const cleaned = base64Png.trim()
  if (!cleaned) return null
  try {
    const bin = atob(cleaned)
    const out = new Uint8Array(bin.length)
    for (let i = 0; i < bin.length; i += 1) {
      out[i] = bin.charCodeAt(i)
    }
    return out.length > 0 ? out : null
  } catch {
    return null
  }
}

/**
 * Copy a base64-encoded PNG screenshot directly to the system clipboard.
 * Uses Tauri's native image clipboard on desktop and falls back to the Async
 * Clipboard API (`navigator.clipboard.write`) on web or if native write fails.
 */
export async function copyBase64PngToClipboard(
  base64Png: string
): Promise<{ ok: true } | { ok: false; error: string }> {
  const bytes = decodeBase64Png(base64Png)
  if (!bytes) {
    const error = 'Screenshot image data is invalid.'
    void logFrontendError({ level: 'warn', source: 'browser-screenshot', message: error })
    return { ok: false, error }
  }

  let nativeErr: string | null = null
  if (isTauriContext()) {
    try {
      await writeClipboardImagePng(bytes)
      return { ok: true }
    } catch (e) {
      nativeErr = e instanceof Error ? e.message : String(e)
      void logFrontendError({
        level: 'warn',
        source: 'browser-screenshot',
        message: `Native clipboard image write failed, trying web fallback: ${nativeErr}`
      })
    }
  }

  if (
    typeof ClipboardItem === 'function' &&
    (typeof ClipboardItem.supports !== 'function' || ClipboardItem.supports('image/png')) &&
    navigator.clipboard?.write
  ) {
    try {
      const blob = new Blob([bytes as BlobPart], { type: 'image/png' })
      await navigator.clipboard.write([new ClipboardItem({ 'image/png': blob })])
      return { ok: true }
    } catch (e) {
      const webErr = e instanceof Error ? e.message : String(e)
      void logFrontendError({
        level: 'warn',
        source: 'browser-screenshot',
        message: `navigator.clipboard.write failed: ${webErr}`
      })
      return { ok: false, error: 'Could not copy screenshot to clipboard.' }
    }
  }

  const error = nativeErr
    ? 'Could not copy screenshot to clipboard.'
    : 'Image clipboard is unavailable in this environment.'
  void logFrontendError({ level: 'warn', source: 'browser-screenshot', message: error })
  return { ok: false, error }
}

/**
 * Format pending batch annotations (including ruler guides, measurements, and
 * persisted crop image paths) into concise, structured markdown ready for chat.
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
          ? item.gapPx !== undefined
            ? `[Ruler ${item.axis ? `${item.axis} ` : ''}${item.gapPx}px]`
            : '[Ruler]'
          : item.kind === 'rectangle'
            ? '[Area]'
            : '[Element]'
    const deltaNote =
      typeof item.deltaPx === 'number' && item.deltaPx !== 0
        ? ` (${item.deltaPx > 0 ? `+${item.deltaPx}` : item.deltaPx}px off)`
        : ''
    const cropNote = item.cropPath ? ` (crop: \`${item.cropPath}\`)` : ''
    lines.push(
      `${item.num}. ${kindTag} \`${item.elementPath || item.element}\`${deltaNote}${cropNote}`
    )
    lines.push(`   ${item.comment}`)
  }
  return lines.join('\n')
}
