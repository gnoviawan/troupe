import { describe, expect, test } from 'bun:test'
import { mkdtempSync } from 'node:fs'
import { tmpdir } from 'node:os'
import path from 'node:path'
import { cropPng, cropRects, diffPngs } from '../diff'

const FIXTURES = path.join(import.meta.dir, '../fixtures')
// frame-b differs from frame-a in the 32x32 block at 16,16: 1,024 pixels.
const A = path.join(FIXTURES, 'frame-a.png')
const B = path.join(FIXTURES, 'frame-b.png')
const out = () => path.join(mkdtempSync(path.join(tmpdir(), 'parity-diff-')), 'diff.png')

describe('diffPngs', () => {
  test('counts the 1,024 changed pixels exactly and perceptually', () => {
    const result = diffPngs(A, B, out())
    expect(result.total).toBe(4096)
    expect(result.exact).toEqual({ mismatched: 1024, percent: 25 })
    expect(result.perceptual).toEqual({ mismatched: 1024, percent: 25 })
    expect(result.changed).toEqual({ x: 16, y: 16, width: 32, height: 32 })
  })

  test('reads zero for a frame against itself', () => {
    const result = diffPngs(A, A, out())
    expect(result.exact.mismatched).toBe(0)
    expect(result.perceptual.mismatched).toBe(0)
    expect(result.changed).toBeNull()
  })

  test('splits the count into text, icons and boxes', () => {
    const result = diffPngs(A, B, out(), {
      text: [{ x: 0, y: 0, width: 32, height: 64 }],
      icons: [{ x: 40, y: 40, width: 4, height: 4 }]
    })
    expect(result.parts.text.pixels).toBe(2048)
    expect(result.parts.text.exact.mismatched).toBe(512)
    expect(result.parts.icons.pixels).toBe(16)
    expect(result.parts.icons.perceptual.mismatched).toBe(16)
    expect(result.parts.boxes.pixels).toBe(4096 - 2048 - 16)
    expect(result.parts.boxes.exact).toEqual({ mismatched: 496, percent: (496 / 2032) * 100 })
  })
})

describe('cropRects', () => {
  test('moves rects into the region and drops the ones outside it', () => {
    const crop = { x: 100, y: 50, width: 200, height: 100 }
    expect(
      cropRects(
        {
          text: [
            { x: 110, y: 60, width: 10, height: 10 },
            { x: 400, y: 60, width: 10, height: 10 }
          ],
          icons: []
        },
        crop
      )
    ).toEqual({ text: [{ x: 10, y: 10, width: 10, height: 10 }], icons: [] })
  })
})

describe('cropPng', () => {
  test('crops the part of the rect inside the image', () => {
    expect(cropPng(A, { x: -5, y: 60, width: 10, height: 10 }, out())).toEqual({
      x: 0,
      y: 60,
      width: 5,
      height: 4
    })
  })
})

test('preserves rounded dimensions for fractional in-bounds crops', () => {
  expect(cropPng(A, { x: 0.4, y: 0.4, width: 10.4, height: 10.4 }, out())).toEqual({
    x: 0,
    y: 0,
    width: 10,
    height: 10
  })
})
