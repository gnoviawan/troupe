// Records what headless Chromium paints for a sample of theme tokens in the
// real web client: starts the client's Vite dev server, applies every bundled
// theme through the app's own applyColorTheme, and reads each sampled token
// back as the 8-bit sRGB pixel a canvas fill of its computed colour paints.
// The Rust token test compares the generated theme files against this fixture.
//   bun scripts/rune/record-theme-fixture.ts [--port 8193]
import { writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { chromium } from 'playwright'

const ROOT = join(import.meta.dir, '..', '..')
const FIXTURE = join(ROOT, 'apps/rune/crates/theme/tests/fixtures/chromium-tokens.json')

/**
 * Opaque in every theme. Covers the contrast solver's outputs, the solid
 * fills, OKLCH round trips, color-mix() in oklab and oklch, and index.css
 * tokens the engine never writes.
 */
const SAMPLED_TOKENS = [
  'background',
  'foreground',
  'card',
  'popover',
  'primary',
  'primary-fill',
  'primary-fill-hover',
  'muted-foreground',
  'secondary-foreground',
  'disabled-foreground',
  'destructive',
  'success',
  'warning-foreground',
  'field-border',
  'focus-ring',
  'popover-edge',
  'control-fill-hover',
  'row-hover-solid',
  'rail-keycap',
  'status-bar-purple'
] as const

const portArg = process.argv.indexOf('--port')
const port = portArg === -1 ? 8193 : Number(process.argv[portArg + 1])

const vite = Bun.spawn(
  [
    'bun',
    'x',
    'vite',
    '--config',
    'vite.config.web.ts',
    '--host',
    '127.0.0.1',
    '--port',
    String(port),
    '--strictPort'
  ],
  { cwd: ROOT, stdout: 'inherit', stderr: 'inherit' }
)
const browser = await chromium.launch()
try {
  const base = `http://127.0.0.1:${port}/`
  for (let i = 0; ; i++) {
    if (i > 120) throw new Error('vite dev server did not start')
    const ok = await fetch(base).then(
      (r) => r.ok,
      () => false
    )
    if (ok) break
    await Bun.sleep(500)
  }
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 } })
  await page.route('**/sw.js', (route) => route.abort())
  // A cold Vite dev server optimizes dependencies on the first load.
  await page.goto(base, { waitUntil: 'commit' })
  await page.waitForFunction(
    () => getComputedStyle(document.documentElement).getPropertyValue('--radius').trim() !== '',
    undefined,
    { timeout: 300_000 }
  )
  const themes = await page.evaluate(async (tokens) => {
    const engine = await import('/src/renderer/lib/themes/apply-color-theme.ts')
    const { BUNDLED_COLOR_THEMES } = await import('/src/renderer/lib/themes/bundled-themes.ts')
    const ctx = document.createElement('canvas').getContext('2d', { willReadFrequently: true })
    if (!ctx) throw new Error('no 2D canvas')
    const probe = document.createElement('div')
    document.body.append(probe)
    const out: Record<string, Record<string, string>> = {}
    for (const id of Object.keys(BUNDLED_COLOR_THEMES)) {
      engine.applyColorTheme(id)
      const values: Record<string, string> = {}
      for (const token of tokens) {
        const raw = getComputedStyle(document.documentElement).getPropertyValue(`--${token}`).trim()
        const components = /^-?[\d.]+ -?[\d.]+ -?[\d.]+$/.test(raw)
        probe.style.backgroundColor = components ? `oklch(var(--${token}))` : `var(--${token})`
        const computed = getComputedStyle(probe).backgroundColor
        ctx.clearRect(0, 0, 1, 1)
        ctx.fillStyle = '#010203'
        ctx.fillStyle = computed
        if (ctx.fillStyle === '#010203') throw new Error(`canvas rejected ${token}: ${computed}`)
        ctx.fillRect(0, 0, 1, 1)
        const px = [...ctx.getImageData(0, 0, 1, 1).data]
        values[token] = `#${px.map((v) => v.toString(16).padStart(2, '0')).join('')}`
      }
      out[id] = values
    }
    return out
  }, SAMPLED_TOKENS)
  const fixture = {
    recordedWith: `Chromium ${browser.version()}, Playwright, ${base} (vite.config.web.ts)`,
    tokens: SAMPLED_TOKENS,
    themes
  }
  writeFileSync(FIXTURE, `${JSON.stringify(fixture, null, 2)}\n`)
  console.log(`wrote ${FIXTURE}: ${Object.keys(themes).length} themes`)
} finally {
  await browser.close()
  vite.kill()
}
