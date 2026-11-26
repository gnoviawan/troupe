/**
 * pixelmatch's perceptual pixel test (https://github.com/mapbox/pixelmatch,
 * ISC licence), ported because the repo has no pixelmatch dependency. Two
 * pixels differ when their YIQ distance is above `threshold`, unless the
 * pixel is an anti-aliased edge in either image.
 */

import type { Image } from './png'

const rgb2y = (r: number, g: number, b: number) => r * 0.29889531 + g * 0.58662247 + b * 0.11448223
const rgb2i = (r: number, g: number, b: number) => r * 0.59597799 - g * 0.2741761 - b * 0.32180189
const rgb2q = (r: number, g: number, b: number) => r * 0.21147017 - g * 0.52261711 + b * 0.31114694
const blend = (c: number, a: number) => 255 + (c - 255) * a

/** Squared YIQ distance between pixel `k` of `a` and pixel `m` of `b`, negative when `a` is brighter. */
function colorDelta(a: Uint8Array, b: Uint8Array, k: number, m: number, yOnly = false): number {
  let r1 = a[k]
  let g1 = a[k + 1]
  let b1 = a[k + 2]
  const a1 = a[k + 3]
  let r2 = b[m]
  let g2 = b[m + 1]
  let b2 = b[m + 2]
  const a2 = b[m + 3]
  if (a1 === a2 && r1 === r2 && g1 === g2 && b1 === b2) return 0
  if (a1 < 255) {
    r1 = blend(r1, a1 / 255)
    g1 = blend(g1, a1 / 255)
    b1 = blend(b1, a1 / 255)
  }
  if (a2 < 255) {
    r2 = blend(r2, a2 / 255)
    g2 = blend(g2, a2 / 255)
    b2 = blend(b2, a2 / 255)
  }
  const y1 = rgb2y(r1, g1, b1)
  const y2 = rgb2y(r2, g2, b2)
  const y = y1 - y2
  if (yOnly) return y
  const i = rgb2i(r1, g1, b1) - rgb2i(r2, g2, b2)
  const q = rgb2q(r1, g1, b1) - rgb2q(r2, g2, b2)
  const delta = 0.5053 * y * y + 0.299 * i * i + 0.1957 * q * q
  return y1 > y2 ? -delta : delta
}

function window3(x: number, y: number, width: number, height: number) {
  return {
    x0: Math.max(x - 1, 0),
    y0: Math.max(y - 1, 0),
    x2: Math.min(x + 1, width - 1),
    y2: Math.min(y + 1, height - 1)
  }
}

/** More than two of the pixel's neighbours have exactly its colour. */
function hasManySiblings(img: Uint8Array, x1: number, y1: number, width: number, height: number) {
  const { x0, y0, x2, y2 } = window3(x1, y1, width, height)
  const pos = (y1 * width + x1) * 4
  let zeroes = x1 === x0 || x1 === x2 || y1 === y0 || y1 === y2 ? 1 : 0
  for (let x = x0; x <= x2; x++) {
    for (let y = y0; y <= y2; y++) {
      if (x === x1 && y === y1) continue
      const pos2 = (y * width + x) * 4
      if (
        img[pos] === img[pos2] &&
        img[pos + 1] === img[pos2 + 1] &&
        img[pos + 2] === img[pos2 + 2] &&
        img[pos + 3] === img[pos2 + 3]
      )
        zeroes++
      if (zeroes > 2) return true
    }
  }
  return false
}

/** The pixel at `x1`,`y1` of `img` sits on an anti-aliased edge that `other` also has. */
function antialiased(
  img: Uint8Array,
  x1: number,
  y1: number,
  width: number,
  height: number,
  other: Uint8Array
): boolean {
  const { x0, y0, x2, y2 } = window3(x1, y1, width, height)
  const pos = (y1 * width + x1) * 4
  let zeroes = x1 === x0 || x1 === x2 || y1 === y0 || y1 === y2 ? 1 : 0
  let min = 0
  let max = 0
  let minX = 0
  let minY = 0
  let maxX = 0
  let maxY = 0
  for (let x = x0; x <= x2; x++) {
    for (let y = y0; y <= y2; y++) {
      if (x === x1 && y === y1) continue
      const delta = colorDelta(img, img, pos, (y * width + x) * 4, true)
      if (delta === 0) {
        zeroes++
        if (zeroes > 2) return false
      } else if (delta < min) {
        min = delta
        minX = x
        minY = y
      } else if (delta > max) {
        max = delta
        maxX = x
        maxY = y
      }
    }
  }
  if (min === 0 || max === 0) return false
  return (
    (hasManySiblings(img, minX, minY, width, height) &&
      hasManySiblings(other, minX, minY, width, height)) ||
    (hasManySiblings(img, maxX, maxY, width, height) &&
      hasManySiblings(other, maxX, maxY, width, height))
  )
}

/** One byte per pixel: 1 where `a` and `b` differ perceptually. */
export function perceptualMask(a: Image, b: Image, threshold: number): Uint8Array {
  const { width, height } = a
  const mask = new Uint8Array(width * height)
  const maxDelta = 35215 * threshold * threshold
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const pos = (y * width + x) * 4
      const delta = colorDelta(a.data, b.data, pos, pos)
      if (Math.abs(delta) <= maxDelta) continue
      if (
        antialiased(a.data, x, y, width, height, b.data) ||
        antialiased(b.data, x, y, width, height, a.data)
      )
        continue
      mask[y * width + x] = 1
    }
  }
  return mask
}
