// Pixel check of Rune gallery scenes against the design-system entries they port. Snapshots
// each scene with the gallery binary at --scale, diffs it with tools/parity/diff.ts against the
// PNG dump-registry.ts wrote, and saves reference | Rune | diff side by side.
//   bun apps/rune/tools/parity-gallery.ts --dump /tmp/registry-dump --bin <gallery> --out <dir>
//     [--ids button.variants,badges] [--scale 2]
// Linux without a GPU needs VK_ICD_FILENAMES pointing at lavapipe.
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { diffImages, type PartRects, type Rect } from '../../../tools/parity/diff'
import { blankImage, blit, readPng, writePng } from '../../../tools/parity/png'

function arg(name: string, fallback: string): string {
  const i = process.argv.indexOf(`--${name}`)
  return i === -1 ? fallback : process.argv[i + 1]
}

const dump = arg('dump', '/tmp/registry-dump')
const bin = arg('bin', '')
const out = arg('out', '/tmp/gallery-parity')
const scale = Number(arg('scale', '2'))
if (!bin) throw new Error('--bin <path to the gallery binary> is required')

interface Entry {
  id: string
  width: number
  height: number
}
interface Element {
  tag: string
  text: string | null
  rect: [number, number, number, number]
}

const entries: Entry[] = JSON.parse(readFileSync(join(dump, 'index.json'), 'utf8')).entries
const ids = arg('ids', '')
  .split(',')
  .filter((id) => id !== '')
const picked = ids.length === 0 ? entries : entries.filter((e) => ids.includes(e.id))
mkdirSync(out, { recursive: true })

/** Where the reference painted text and icons, in device pixels. */
function parts(id: string): PartRects {
  const elements: Element[] = JSON.parse(readFileSync(join(dump, `${id}.json`), 'utf8')).elements
  const box = ([x, y, w, h]: Element['rect']): Rect => ({
    x: x * scale,
    y: y * scale,
    width: w * scale,
    height: h * scale
  })
  return {
    text: elements.filter((e) => e.text).map((e) => box(e.rect)),
    icons: elements.filter((e) => e.tag === 'svg' || e.tag === 'img').map((e) => box(e.rect))
  }
}

const rows: Record<string, unknown>[] = []
for (const entry of picked) {
  const runePng = join(out, `${entry.id}.rune.png`)
  const snap = Bun.spawnSync(
    [
      bin,
      '--scene',
      entry.id,
      '--size',
      `${entry.width}x${entry.height}`,
      '--scale',
      String(scale),
      '--snap',
      runePng
    ],
    { stderr: 'pipe', stdout: 'pipe' }
  )
  if (snap.exitCode !== 0) throw new Error(`${entry.id}: ${snap.stderr.toString()}`)
  const reference = readPng(join(dump, `${entry.id}.png`))
  const rune = readPng(runePng)
  const { result, image } = diffImages(reference, rune, parts(entry.id))
  const { width, height } = reference
  const sheet = blankImage(width * 3 + 16, height, 255)
  blit(reference, sheet, 0, 0, width, height, 0, 0)
  blit(rune, sheet, 0, 0, width, height, width + 8, 0)
  blit(image, sheet, 0, 0, width, height, width * 2 + 16, 0)
  writePng(join(out, `${entry.id}.side-by-side.png`), sheet)
  const row = {
    id: entry.id,
    perceptual: result.perceptual.percent,
    exact: result.exact.percent,
    boxes: result.parts.boxes.perceptual.percent,
    text: result.parts.text.perceptual.percent,
    icons: result.parts.icons.perceptual.percent
  }
  rows.push(row)
  console.log(
    `${entry.id.padEnd(22)} perceptual ${row.perceptual.toFixed(2)}%  exact ${row.exact.toFixed(2)}%  boxes ${row.boxes.toFixed(2)}%  text ${row.text.toFixed(2)}%  icons ${row.icons.toFixed(2)}%`
  )
}
writeFileSync(join(out, 'summary.json'), `${JSON.stringify({ scale, rows }, null, 2)}\n`)
