import { describe, expect, it } from 'vitest'
import { computeCropSourceRect, formatBatchAnnotationsForChat } from './browser-screenshot'

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
})
