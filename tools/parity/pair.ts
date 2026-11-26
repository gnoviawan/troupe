/**
 * `bun run parity [surface-id...] [--out DIR] [--scale 0.5] [--crop x,y,w,h]`
 *
 * Review aid: writes `pair.png` next to each step's frames, the reference on
 * the left and the candidate on the right, box-downscaled, so one image shows
 * a whole pair. Without ids it pairs every step of every surface in
 * `<out>/results.json`. Prints the path of each image it writes.
 */

import { existsSync, readFileSync } from 'node:fs'
import path from 'node:path'
import { parseArgs } from 'node:util'
import { blankImage, blit, type Image, readPng, writePng } from './png'
import type { SavedResults } from './types'

const GAP = 8

/** `file` (or its `crop` part) box-downscaled by `scale`. */
function scaled(file: string, scale: number, crop?: number[]): Image {
  const src = readPng(file)
  const [left = 0, top = 0, w = src.width, h = src.height] = crop ?? []
  const cx = Math.min(Math.max(0, left), src.width - 1)
  const cy = Math.min(Math.max(0, top), src.height - 1)
  const cw = Math.min(w, src.width - cx)
  const ch = Math.min(h, src.height - cy)
  const width = Math.max(1, Math.round(cw * scale))
  const height = Math.max(1, Math.round(ch * scale))
  const out = blankImage(width, height)
  const step = 1 / scale
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const sum = [0, 0, 0, 0]
      let n = 0
      // At least one source pixel per output pixel, so a scale above 1 enlarges.
      const y0 = Math.floor(cy + y * step)
      const x0 = Math.floor(cx + x * step)
      const yEnd = Math.min(cy + ch, Math.max(y0 + 1, Math.floor(cy + (y + 1) * step)))
      const xEnd = Math.min(cx + cw, Math.max(x0 + 1, Math.floor(cx + (x + 1) * step)))
      for (let yy = y0; yy < yEnd; yy++) {
        for (let xx = x0; xx < xEnd; xx++) {
          const i = (yy * src.width + xx) * 4
          for (let c = 0; c < 4; c++) sum[c] += src.data[i + c]
          n++
        }
      }
      for (let c = 0; c < 4; c++) out.data[(y * width + x) * 4 + c] = n ? Math.round(sum[c] / n) : 0
    }
  }
  return out
}

export function writePair(dir: string, scale: number, crop?: number[]): string | null {
  const reference = path.join(dir, 'reference.png')
  const candidate = path.join(dir, 'candidate.png')
  if (!existsSync(reference) || !existsSync(candidate)) return null
  const a = scaled(reference, scale, crop)
  const b = scaled(candidate, scale, crop)
  const out = blankImage(a.width + GAP + b.width, Math.max(a.height, b.height), 255)
  blit(a, out, 0, 0, a.width, a.height, 0, 0)
  blit(b, out, 0, 0, b.width, b.height, a.width + GAP, 0)
  const file = path.join(dir, crop ? 'pair-crop.png' : 'pair.png')
  writePng(file, out)
  return file
}

if (import.meta.main) {
  const { values, positionals } = parseArgs({
    args: Bun.argv.slice(2),
    allowPositionals: true,
    options: {
      out: {
        type: 'string',
        default: path.join(process.env.TROUPE_PARITY_ROOT ?? '/tmp/troupe-parity', 'out')
      },
      scale: { type: 'string', default: '0.5' },
      crop: { type: 'string' }
    }
  })
  const results = JSON.parse(
    readFileSync(path.join(path.resolve(values.out as string), 'results.json'), 'utf8')
  ) as SavedResults
  const crop = values.crop?.split(',').map(Number)
  for (const [id, result] of Object.entries(results.surfaces)) {
    if (positionals.length && !positionals.includes(id)) continue
    for (const [i, frame] of (result.reference?.frames ?? []).entries()) {
      // A step the candidate never reached can hold an earlier run's candidate.png.
      if (!result.candidate?.frames[i]) continue
      const file = writePair(path.dirname(frame), Number(values.scale), crop)
      if (file) console.log(file)
    }
  }
}
