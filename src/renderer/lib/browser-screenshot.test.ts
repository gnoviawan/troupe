import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

let mockIsTauri = true
const mockWriteClipboardImagePng = vi.fn()

vi.mock('@/lib/tauri-runtime', () => ({
  isTauriContext: () => mockIsTauri
}))

vi.mock('@/lib/composer-attachments-io', () => ({
  writeClipboardImagePng: (...args: unknown[]) => mockWriteClipboardImagePng(...args)
}))

import {
  computeCropSourceRect,
  copyBase64PngToClipboard,
  formatBatchAnnotationsForChat
} from './browser-screenshot'
import { normalizeBrowserInputUrl } from './browser-url'

describe('computeCropSourceRect', () => {
  it('derives pixel scale from imgWidth / viewportWidth across zoom and DPR', () => {
    // 1000px CSS viewport at 125% zoom on 2x display -> 2500px wide screenshot (scale = 2.5)
    const rect = computeCropSourceRect(2500, 1500, {
      x: 100,
      y: 40,
      width: 200,
      height: 80,
      viewportWidth: 1000,
      dpr: 2
    })
    expect(rect).toEqual({ sx: 250, sy: 100, sw: 500, sh: 200 })
  })

  it('clamps negative origins to zero and shrinks width/height to only the visible area', () => {
    const rect = computeCropSourceRect(800, 600, {
      x: -20,
      y: -10,
      width: 100,
      height: 50,
      dpr: 2
    })
    // rawX = -40, rawY = -20, width*2 = 200 -> visible width = 160; height*2 = 100 -> visible height = 80
    expect(rect).toEqual({ sx: 0, sy: 0, sw: 160, sh: 80 })
  })
})

describe('formatBatchAnnotationsForChat', () => {
  it('includes guide metadata, delta, and persisted cropPath when available', () => {
    const md = formatBatchAnnotationsForChat('http://localhost:3000', [
      {
        id: 'ann-1',
        num: 1,
        kind: 'guide',
        element: 'h1',
        elementPath: 'main > h1',
        comment: 'Align left edge',
        axis: 'vertical',
        positionPx: 32,
        deltaPx: 8,
        cropPath: '/tmp/annotation-crops/ann-1.png'
      }
    ])
    expect(md).toContain('[Guide vertical 32px]')
    expect(md).toContain('(+8px off)')
    expect(md).toContain('(crop: `/tmp/annotation-crops/ann-1.png`)')
    expect(md).toContain('Align left edge')
  })

  it('renders gapPx for ruler measurement annotations', () => {
    const md = formatBatchAnnotationsForChat('http://localhost:3000', [
      {
        id: 'ann-2',
        num: 1,
        kind: 'measurement',
        element: 'header',
        elementPath: 'header',
        comment: 'Keep 24px top spacing',
        axis: 'vertical',
        gapPx: 24
      }
    ])
    expect(md).toContain('[Ruler vertical 24px]')
  })
})

describe('normalizeBrowserInputUrl', () => {
  it('keeps query-only and fragment-only localhost inputs on http while preserving host boundaries', () => {
    expect(normalizeBrowserInputUrl('localhost?foo=1')).toBe('http://localhost?foo=1')
    expect(normalizeBrowserInputUrl('localhost#top')).toBe('http://localhost#top')
    expect(normalizeBrowserInputUrl('127.0.0.1:3000?a=1')).toBe('http://127.0.0.1:3000?a=1')
    expect(normalizeBrowserInputUrl(':3000#section')).toBe('http://localhost:3000#section')
    expect(normalizeBrowserInputUrl('localhost.example.com')).toBe('https://localhost.example.com')
  })
})

describe('copyBase64PngToClipboard', () => {
  beforeEach(() => {
    mockIsTauri = true
    mockWriteClipboardImagePng.mockReset()
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('writes decoded PNG bytes through native Tauri clipboard on desktop', async () => {
    mockWriteClipboardImagePng.mockResolvedValueOnce(undefined)
    const res = await copyBase64PngToClipboard('iVBORw==')
    expect(res).toEqual({ ok: true })
    expect(mockWriteClipboardImagePng).toHaveBeenCalledTimes(1)
    expect(mockWriteClipboardImagePng.mock.calls[0][0]).toBeInstanceOf(Uint8Array)
  })

  it('uses navigator.clipboard.write on web and falls back when native clipboard fails', async () => {
    const write = vi.fn().mockResolvedValue(undefined)
    class FakeClipboardItem {
      constructor(public items: Record<string, Blob>) {}
    }
    vi.stubGlobal('ClipboardItem', FakeClipboardItem)
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: { write }
    })

    // Desktop native error -> falls back to web Clipboard API
    mockWriteClipboardImagePng.mockRejectedValueOnce(new Error('plugin failed'))
    const desktopFallback = await copyBase64PngToClipboard('iVBORw==')
    expect(desktopFallback).toEqual({ ok: true })
    expect(write).toHaveBeenCalledTimes(1)

    // Web mode -> uses navigator.clipboard.write directly
    mockIsTauri = false
    const webRes = await copyBase64PngToClipboard('iVBORw==')
    expect(webRes).toEqual({ ok: true })
    expect(write).toHaveBeenCalledTimes(2)
  })

  it('returns clear error on empty/invalid base64 payload', async () => {
    const res = await copyBase64PngToClipboard('')
    expect(res.ok).toBe(false)
    expect(mockWriteClipboardImagePng).not.toHaveBeenCalled()
  })
})
