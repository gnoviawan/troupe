// Dumps every design-system registry entry as the browser draws it: each
// element's rect inside the preview frame and its computed padding, border,
// radius, colours, font and shadow, plus a PNG of the frame. Rune components
// are matched against these numbers.
//   bun apps/rune/tools/dump-registry.ts [--port 8197] [--out /tmp/registry-dump]
//     [--scale 2] [--ids button.variants,badges] [--theme termul] [--server URL]
//     [--registry apps/rune/crates/gallery/registry.json]
// Without --server it starts the design-system Vite dev server on --port.
// --registry writes the entry list (id, name, frame size) the Rune gallery checks its scenes
// against.
import { mkdirSync, mkdtempSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { chromium } from 'playwright'

const ROOT = join(import.meta.dir, '..', '..', '..')

function arg(name: string, fallback: string): string {
  const i = process.argv.indexOf(`--${name}`)
  return i === -1 ? fallback : process.argv[i + 1]
}

const port = Number(arg('port', '8197'))
const out = arg('out', '') || mkdtempSync(join(tmpdir(), 'rune-registry-'))
const scale = Number(arg('scale', '2'))
const theme = arg('theme', 'termul')
const only = arg('ids', '')
  .split(',')
  .filter((id) => id !== '')
const external = arg('server', '')
const registry = arg('registry', '')

/** Room around a frame so its shadow and focus ring land in the shot. */
const MARGIN = 16

const vite = external
  ? null
  : Bun.spawn(
      [
        'bun',
        'x',
        'vite',
        '--config',
        'apps/rune/tools/design-system.vite.config.ts',
        '--host',
        '127.0.0.1',
        '--port',
        String(port),
        '--strictPort'
      ],
      { cwd: ROOT, stdout: 'ignore', stderr: 'inherit' }
    )
const base = external || `http://127.0.0.1:${port}/`
const browser = await chromium.launch({ args: ['--font-render-hinting=none'] })
try {
  for (let i = 0; ; i++) {
    if (i > 240) throw new Error(`design-system server at ${base} did not answer`)
    if (
      await fetch(base).then(
        (r) => r.ok,
        () => false
      )
    )
      break
    await Bun.sleep(500)
  }
  const page = await browser.newPage({
    viewport: { width: 1600, height: 1400 },
    deviceScaleFactor: scale,
    reducedMotion: 'reduce',
    colorScheme: 'dark'
  })
  await page.addInitScript((themeId) => {
    localStorage.setItem(
      'termul.design-system-canvas.layout',
      JSON.stringify({ v: 1, camera: null, themeId, positions: {} })
    )
  }, theme)
  await page.goto(base, { waitUntil: 'commit' })
  await page.waitForSelector('[data-canvas-item] [data-preview-frame]', { timeout: 300_000 })
  await page.evaluate(() => document.fonts.ready)
  await page.waitForTimeout(500)

  const entries = await page.evaluate(() =>
    [...document.querySelectorAll<HTMLElement>('[data-canvas-item]')].map((item) => {
      const frame = item.querySelector<HTMLElement>('[data-preview-frame]')
      return {
        id: item.dataset.canvasItem ?? '',
        name: item.querySelector('[aria-label$="Copy name"] .truncate')?.textContent?.trim() ?? '',
        width: frame?.offsetWidth ?? 0,
        height: frame?.offsetHeight ?? 0
      }
    })
  )
  mkdirSync(out, { recursive: true })
  writeFileSync(
    join(out, 'index.json'),
    `${JSON.stringify({ base, theme, scale, entries }, null, 2)}\n`
  )
  console.log(`${entries.length} entries`)
  if (registry) {
    writeFileSync(registry, `${JSON.stringify(entries, null, 2)}\n`)
    console.log(`wrote ${registry}`)
  }

  for (const entry of entries) {
    if (only.length > 0 && !only.includes(entry.id)) continue
    const dump = await page.evaluate(
      ({ id, margin }) => {
        const item = document.querySelector<HTMLElement>(`[data-canvas-item="${CSS.escape(id)}"]`)
        const frame = item?.querySelector<HTMLElement>('[data-preview-frame]')
        const world = item?.parentElement
        if (!item || !frame || !world) throw new Error(`no frame for ${id}`)
        // Only the world stays visible: the page header and toolbar float over the canvas.
        for (let node: Element | null = world; node?.parentElement; node = node.parentElement) {
          for (const sibling of node.parentElement.children) {
            if (sibling !== node) (sibling as HTMLElement).style.visibility = 'hidden'
          }
        }
        for (const other of world.children) {
          ;(other as HTMLElement).style.visibility = other === item ? '' : 'hidden'
        }
        // Put the frame at (margin, margin) at zoom 1: camera writes only on camera changes.
        world.style.transform = 'none'
        const placed = frame.getBoundingClientRect()
        world.style.transform = `translate(${margin - placed.left}px, ${margin - placed.top}px)`
        const origin = frame.getBoundingClientRect()
        const px = (v: string) => Number.parseFloat(v) || 0
        const elements: unknown[] = []
        const walk = (el: Element, depth: number) => {
          const r = el.getBoundingClientRect()
          const s = getComputedStyle(el)
          const own = [...el.childNodes]
            .filter((n) => n.nodeType === Node.TEXT_NODE)
            .map((n) => n.textContent ?? '')
            .join('')
            .trim()
          elements.push({
            depth,
            tag: el.tagName.toLowerCase(),
            role: el.getAttribute('role'),
            label: el.getAttribute('aria-label'),
            text: own || null,
            rect: [r.left - origin.left, r.top - origin.top, r.width, r.height].map(
              (v) => Math.round(v * 100) / 100
            ),
            padding: [s.paddingTop, s.paddingRight, s.paddingBottom, s.paddingLeft].map(px),
            border: [
              s.borderTopWidth,
              s.borderRightWidth,
              s.borderBottomWidth,
              s.borderLeftWidth
            ].map(px),
            borderColor: s.borderTopColor,
            radius: s.borderRadius,
            background: s.backgroundColor,
            backgroundImage: s.backgroundImage === 'none' ? null : s.backgroundImage,
            color: s.color,
            font: `${s.fontWeight} ${s.fontSize}/${s.lineHeight} ${s.fontFamily}`,
            letterSpacing: s.letterSpacing,
            shadow: s.boxShadow === 'none' ? null : s.boxShadow,
            opacity: Number(s.opacity),
            gap: s.gap === 'normal' ? null : s.gap
          })
          // An icon is one box; its paths are noise.
          if (el.tagName.toLowerCase() === 'svg') return
          for (const child of el.children) walk(child, depth + 1)
        }
        walk(frame, 0)
        return { width: origin.width, height: origin.height, elements }
      },
      { id: entry.id, margin: MARGIN }
    )
    await page.waitForTimeout(50)
    await page.screenshot({
      path: join(out, `${entry.id}.png`),
      clip: { x: MARGIN, y: MARGIN, width: dump.width, height: dump.height }
    })
    writeFileSync(
      join(out, `${entry.id}.json`),
      `${JSON.stringify({ id: entry.id, ...dump }, null, 1)}\n`
    )
  }
  console.log(`wrote ${out}`)
} finally {
  await browser.close()
  vite?.kill()
}
