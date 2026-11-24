// Opens each overlay of the design-system gallery the way a user does (a click, a right-click or
// a hover on its trigger) and dumps the open surface: every element's rect inside it with its
// computed padding, border, radius, colours, font and shadow, plus a PNG of the surface. The
// Rune overlay layer is matched against these numbers, and parity-overlays.ts diffs its crops
// against these PNGs.
//   bun apps/rune/tools/dump-overlays.ts [--port 8196] [--out /tmp/overlay-dump] [--scale 2]
//     [--ids dialog,menu.dropdown] [--theme termul] [--server URL]
import { mkdirSync, mkdtempSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { chromium, type Page } from 'playwright'
import { OVERLAY_CASES, type OverlayCase, VIEWPORT } from './overlay-cases'

const ROOT = join(import.meta.dir, '..', '..', '..')

function arg(name: string, fallback: string): string {
  const i = process.argv.indexOf(`--${name}`)
  return i === -1 ? fallback : process.argv[i + 1]
}

const port = Number(arg('port', '8196'))
const out = arg('out', '') || mkdtempSync(join(tmpdir(), 'rune-overlays-'))
const scale = Number(arg('scale', '2'))
const theme = arg('theme', 'termul')
const only = arg('ids', '')
  .split(',')
  .filter((id) => id !== '')
const external = arg('server', '')

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

/** Shows one registry entry alone with its frame at (16, 16), as dump-registry.ts does. */
async function isolate(page: Page, entry: string): Promise<void> {
  await page.evaluate((id) => {
    const item = document.querySelector<HTMLElement>(`[data-canvas-item="${CSS.escape(id)}"]`)
    const frame = item?.querySelector<HTMLElement>('[data-preview-frame]')
    const world = item?.parentElement
    if (!item || !frame || !world) throw new Error(`no frame for ${id}`)
    for (let node: Element | null = world; node?.parentElement; node = node.parentElement) {
      for (const sibling of node.parentElement.children) {
        if (sibling !== node) (sibling as HTMLElement).style.visibility = 'hidden'
      }
    }
    for (const other of world.children) {
      ;(other as HTMLElement).style.visibility = other === item ? '' : 'hidden'
    }
    world.style.transform = 'none'
    const placed = frame.getBoundingClientRect()
    world.style.transform = `translate(${16 - placed.left}px, ${16 - placed.top}px)`
  }, entry)
}

async function open(page: Page, c: OverlayCase): Promise<void> {
  const item = page.locator(`[data-canvas-item="${c.entry}"] [data-preview-frame]`)
  for (const step of c.open) {
    const target = (
      step.role === 'text'
        ? item.getByText(step.name, { exact: true })
        : item.getByRole(step.role, { name: step.name, exact: true })
    ).first()
    if (step.action === 'hover') await target.hover()
    else if (step.action === 'context') await target.click({ button: 'right' })
    else await target.click()
  }
  // Poppers mount in the canvas's own portal, which isolate() hid with the other entries.
  await page.waitForSelector(c.surface, { state: 'attached', timeout: 5000 })
  await page.evaluate(() => {
    for (const el of document.querySelectorAll<HTMLElement>(
      '[data-radix-popper-content-wrapper]'
    )) {
      el.style.visibility = 'visible'
    }
  })
  await page.locator(c.surface).last().waitFor({ state: 'visible', timeout: 5000 })
  // Past the tooltip delay and every entrance (reduced motion shortens most of them).
  await page.waitForTimeout(c.settle ?? 400)
}

async function dumpSurface(page: Page, selector: string) {
  return page.evaluate((sel) => {
    const all = document.querySelectorAll<HTMLElement>(sel)
    const surface = all[all.length - 1]
    if (!surface) throw new Error(`no surface ${sel}`)
    const origin = surface.getBoundingClientRect()
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
        border: [s.borderTopWidth, s.borderRightWidth, s.borderBottomWidth, s.borderLeftWidth].map(
          px
        ),
        borderColor: s.borderTopColor,
        outline: s.outlineStyle === 'none' ? null : `${s.outlineWidth} ${s.outlineColor}`,
        radius: s.borderRadius,
        background: s.backgroundColor,
        color: s.color,
        font: `${s.fontWeight} ${s.fontSize}/${s.lineHeight} ${s.fontFamily}`,
        letterSpacing: s.letterSpacing,
        shadow: s.boxShadow === 'none' ? null : s.boxShadow,
        opacity: Number(s.opacity),
        gap: s.gap === 'normal' ? null : s.gap
      })
      if (el.tagName.toLowerCase() === 'svg') return
      for (const child of el.children) walk(child, depth + 1)
    }
    walk(surface, 0)
    return {
      rect: [origin.left, origin.top, origin.width, origin.height],
      elements,
      focused:
        document.activeElement?.getAttribute('aria-label') ??
        document.activeElement?.textContent?.trim().slice(0, 40) ??
        null
    }
  }, selector)
}

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
  mkdirSync(out, { recursive: true })
  const index: unknown[] = []
  for (const c of OVERLAY_CASES) {
    if (only.length > 0 && !only.includes(c.id)) continue
    const page = await browser.newPage({
      viewport: { width: VIEWPORT[0], height: VIEWPORT[1] },
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
    await isolate(page, c.entry)
    await open(page, c)
    const dump = await dumpSurface(page, c.surface)
    const [x, y, width, height] = dump.rect
    await page.screenshot({ path: join(out, `${c.id}.png`), clip: { x, y, width, height } })
    await page.screenshot({ path: join(out, `${c.id}.page.png`) })
    writeFileSync(join(out, `${c.id}.json`), `${JSON.stringify({ id: c.id, ...dump }, null, 1)}\n`)
    index.push({ id: c.id, rect: dump.rect, focused: dump.focused })
    console.log(`${c.id.padEnd(18)} ${width}x${height} at ${x},${y} focus ${dump.focused}`)
    await page.close()
  }
  writeFileSync(
    join(out, 'index.json'),
    `${JSON.stringify({ base, theme, scale, cases: index }, null, 2)}\n`
  )
  console.log(`wrote ${out}`)
} finally {
  await browser.close()
  vite?.kill()
}
