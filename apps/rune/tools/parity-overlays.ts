// Pixel check of the open overlay surfaces against the design-system gallery. For each case in
// overlay-cases.ts it opens the surface in the Rune gallery binary with a script (Escape closes
// the surface --scene opened, then a tap, right-click or hover on the trigger), crops the
// surface's rect from --tree out of the --snap, diffs it with tools/parity/diff.ts against the
// PNG dump-overlays.ts wrote, and saves reference | Rune | diff side by side.
//   bun apps/rune/tools/parity-overlays.ts --dump /tmp/overlay-dump --bin <gallery> --out <dir>
//     [--ids dialog,select] [--scale 2]
// Linux without a GPU needs VK_ICD_FILENAMES pointing at lavapipe.
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { diffImages, type PartRects, type Rect } from '../../../tools/parity/diff'
import { blankImage, blit, readPng, writePng } from '../../../tools/parity/png'
import { OVERLAY_CASES, type OverlayCase, VIEWPORT } from './overlay-cases'

function arg(name: string, fallback: string): string {
  const i = process.argv.indexOf(`--${name}`)
  return i === -1 ? fallback : process.argv[i + 1]
}

const dump = arg('dump', '/tmp/overlay-dump')
const bin = arg('bin', '')
const out = arg('out', '') || mkdtempSync(join(tmpdir(), 'overlay-parity-'))
const scale = Number(arg('scale', '2'))
if (!bin) throw new Error('--bin <path to the gallery binary> is required')
const ids = arg('ids', '')
  .split(',')
  .filter((id) => id !== '')
const picked = ids.length === 0 ? OVERLAY_CASES : OVERLAY_CASES.filter((c) => ids.includes(c.id))
mkdirSync(out, { recursive: true })

interface Element {
  tag: string
  text: string | null
  rect: [number, number, number, number]
}

/** Where the reference painted text and icons inside the surface, in device pixels. */
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

function script(c: OverlayCase): unknown[] {
  const open =
    c.gesture === 'context'
      ? { t: 0.3, type: 'secondary-click', key: c.trigger }
      : c.gesture === 'hover'
        ? { t: 0.3, type: 'move', key: c.trigger }
        : { t: 0.3, type: 'tap', key: c.trigger }
  return [{ t: 0.1, type: 'key', name: 'Escape' }, open]
}

type Node = { key?: string; rect?: [number, number, number, number] } & Record<string, unknown>

function findRect(tree: unknown, key: string): [number, number, number, number] | null {
  if (Array.isArray(tree)) {
    for (const v of tree) {
      const r = findRect(v, key)
      if (r) return r
    }
  } else if (tree && typeof tree === 'object') {
    const n = tree as Node
    if (n.key === key && n.rect) return n.rect
    for (const v of Object.values(n)) {
      const r = findRect(v, key)
      if (r) return r
    }
  }
  return null
}

const rows: Record<string, unknown>[] = []
for (const c of picked) {
  const scriptPath = join(out, `${c.id}.script.json`)
  const snap = join(out, `${c.id}.rune-page.png`)
  const treePath = join(out, `${c.id}.tree.json`)
  writeFileSync(scriptPath, JSON.stringify(script(c)))
  const run = Bun.spawnSync(
    [
      bin,
      '--scene',
      c.entry,
      '--size',
      `${VIEWPORT[0]}x${VIEWPORT[1]}`,
      '--scale',
      String(scale),
      '--script',
      scriptPath,
      '--at',
      String(0.3 + (c.settle ?? 400) / 1000),
      '--snap',
      snap,
      '--tree',
      treePath
    ],
    { stderr: 'pipe', stdout: 'pipe' }
  )
  if (run.exitCode !== 0) throw new Error(`${c.id}: ${run.stderr.toString()}`)
  const rect = findRect(JSON.parse(readFileSync(treePath, 'utf8')).elements, c.key)
  if (!rect) throw new Error(`${c.id}: the Rune gallery shows no ${c.key}`)
  const reference = readPng(join(dump, `${c.id}.png`))
  const page = readPng(snap)
  const { width, height } = reference
  // The surface's top-left in the Rune frame, cropped to the reference's size.
  const rune = blankImage(width, height, 0)
  blit(page, rune, Math.round(rect[0] * scale), Math.round(rect[1] * scale), width, height, 0, 0)
  writePng(join(out, `${c.id}.rune.png`), rune)
  const { result, image } = diffImages(reference, rune, parts(c.id))
  const sheet = blankImage(width * 3 + 16, height, 255)
  blit(reference, sheet, 0, 0, width, height, 0, 0)
  blit(rune, sheet, 0, 0, width, height, width + 8, 0)
  blit(image, sheet, 0, 0, width, height, width * 2 + 16, 0)
  writePng(join(out, `${c.id}.side-by-side.png`), sheet)
  const reactRect = JSON.parse(readFileSync(join(dump, `${c.id}.json`), 'utf8')).rect
  const row = {
    id: c.id,
    react: reactRect.map((v: number) => Math.round(v * 100) / 100),
    rune: rect,
    perceptual: result.perceptual.percent,
    exact: result.exact.percent,
    boxes: result.parts.boxes.perceptual.percent,
    text: result.parts.text.perceptual.percent,
    icons: result.parts.icons.perceptual.percent
  }
  rows.push(row)
  console.log(
    `${c.id.padEnd(16)} react ${row.react.join(',')} rune ${rect.join(',')}  perceptual ${row.perceptual.toFixed(2)}%  exact ${row.exact.toFixed(2)}%  boxes ${row.boxes.toFixed(2)}%  text ${row.text.toFixed(2)}%  icons ${row.icons.toFixed(2)}%`
  )
}
writeFileSync(join(out, 'summary.json'), `${JSON.stringify({ scale, rows }, null, 2)}\n`)
