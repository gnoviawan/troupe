// Opens overlays in the wasm gallery in Chromium and reads Chrome's accessibility tree (CDP
// Accessibility.getFullAXTree) for each, to check what Rune's web mirror says about the
// overlay roles: dialog, alertdialog, menu, menuitem, combobox, listbox, option and tooltip.
//   bun apps/rune/tools/web-a11y.ts --web <web-build dir> [--port 8191] [--out DIR, default a fresh temp dir]
// Writes <out>/<case>.json (role, name and properties of every node) and <out>/<case>.png.
import { mkdirSync, mkdtempSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { chromium, type Page } from 'playwright'

function arg(name: string, fallback: string): string {
  const i = process.argv.indexOf(`--${name}`)
  return i === -1 ? fallback : process.argv[i + 1]
}

const web = arg('web', '')
const port = Number(arg('port', '8191'))
const out = arg('out', '') || mkdtempSync(join(tmpdir(), 'web-a11y-'))
if (!web) throw new Error('--web <dir that web-build.sh wrote> is required')
mkdirSync(out, { recursive: true })

const TYPES: Record<string, string> = {
  '.js': 'text/javascript',
  '.wasm': 'application/wasm',
  '.html': 'text/html',
  '.ttf': 'font/ttf',
  '.otf': 'font/otf'
}
const started = Date.now()
const requests: string[] = []
const server = Bun.serve({
  hostname: '127.0.0.1',
  port,
  fetch(req) {
    const pathname = new URL(req.url).pathname
    requests.push(`${Date.now() - started} ms ${pathname}`)
    const path = pathname === '/' ? '/index.html' : pathname
    const file = Bun.file(join(web, path))
    const ext = path.slice(path.lastIndexOf('.'))
    return file.exists().then((ok) =>
      ok
        ? new Response(file, {
            headers: { 'content-type': TYPES[ext] ?? 'application/octet-stream' }
          })
        : new Response('not found', { status: 404 })
    )
  }
})

interface Step {
  /** The mirror element's accessible name, or the text to type for `type`. */
  name: string
  action: 'click' | 'hover' | 'type'
}

interface Case {
  id: string
  scene: string
  steps: Step[]
}

// Each case picks a scene in the gallery's list by its registry name, then drives its triggers by accessible name.
const CASES: Case[] = [
  {
    id: 'dialog',
    scene: 'Dialog and AlertDialog',
    steps: [{ name: 'Rename session', action: 'click' }]
  },
  {
    id: 'alert-dialog',
    scene: 'Dialog and AlertDialog',
    steps: [{ name: 'Delete worktree', action: 'click' }]
  },
  {
    id: 'menu',
    scene: 'DropdownMenu and ContextMenu',
    steps: [{ name: 'View options', action: 'click' }]
  },
  { id: 'select-closed', scene: 'Select', steps: [] },
  { id: 'select', scene: 'Select', steps: [{ name: 'Branch', action: 'click' }] },
  { id: 'command', scene: 'Command', steps: [{ name: 'Open CommandDialog', action: 'click' }] },
  {
    id: 'tooltip',
    scene: 'Tooltip, HoverCard, Popover, Lightbox',
    steps: [{ name: 'Split terminal', action: 'hover' }]
  },
  // CJK and emoji in a field: boxes until the late fonts arrive (fetch-web-fonts.mjs).
  {
    id: 'cjk-emoji',
    scene: 'Dialog and AlertDialog',
    steps: [
      { name: 'Rename session', action: 'click' },
      { name: ' 日本語のセッション 🚀🎉', action: 'type' }
    ]
  }
]

type AXNode = {
  role?: { value?: string }
  name?: { value?: string }
  ignored?: boolean
  properties?: { name: string; value: { value?: unknown } }[]
}

async function axTree(page: Page) {
  const cdp = await page.context().newCDPSession(page)
  const { nodes } = (await cdp.send('Accessibility.getFullAXTree')) as { nodes: AXNode[] }
  return nodes
    .filter((n) => !n.ignored)
    .map((n) => ({
      role: n.role?.value ?? '',
      name: n.name?.value ?? '',
      props: Object.fromEntries((n.properties ?? []).map((p) => [p.name, p.value.value]))
    }))
    .filter(
      (n) => !['generic', 'none', 'InlineTextBox', 'StaticText', 'LineBreak'].includes(n.role)
    )
}

/** Presses the mirror element named `name` as a screen reader does (a click event on it), or
 * hovers the canvas where it sits. */
async function drive(page: Page, name: string, action: Step['action']) {
  if (action === 'type') {
    await page.keyboard.insertText(name)
    await page.waitForTimeout(2500)
    return
  }
  const target = page
    .getByRole('button', { name, exact: true })
    .or(page.getByRole('combobox', { name, exact: true }))
    .first()
  await target.waitFor({ state: 'attached', timeout: 20_000 })
  const box = await target.boundingBox()
  if (!box) throw new Error(`${name} has no box`)
  const [x, y] = [box.x + box.width / 2, box.y + box.height / 2]
  if (action === 'hover') await page.mouse.move(x, y)
  else await target.dispatchEvent('click')
  await page.waitForTimeout(action === 'hover' ? 900 : 400)
}

// The full Chromium build in new headless mode: the headless shell paints nothing for the
// canvas. SwiftShader's Vulkan gives WebGPU an adapter on a host without a GPU.
const browser = await chromium.launch({
  channel: 'chromium',
  args: [
    '--no-sandbox',
    '--enable-unsafe-webgpu',
    '--enable-features=Vulkan',
    '--use-angle=vulkan',
    '--use-vulkan=swiftshader'
  ]
})
const results: Record<string, unknown> = {}
try {
  for (const c of CASES) {
    const page = await browser.newPage({
      viewport: { width: 1280, height: 800 },
      reducedMotion: 'reduce'
    })
    const logs: string[] = []
    page.on('console', (m) => logs.push(m.text()))
    await page.goto(`http://127.0.0.1:${port}/`)
    await page.waitForSelector('canvas', { timeout: 60_000 })
    await drive(page, c.scene, 'click')
    for (const step of c.steps) await drive(page, step.name, step.action)
    const tree = await axTree(page)
    await page.screenshot({ path: join(out, `${c.id}.png`) })
    writeFileSync(join(out, `${c.id}.json`), `${JSON.stringify({ tree, logs }, null, 1)}\n`)
    results[c.id] = tree.map((n) => `${n.role}:${n.name}`)
    console.log(`== ${c.id}`)
    for (const n of tree) console.log(`  ${n.role} "${n.name}" ${JSON.stringify(n.props)}`)
    await page.close()
  }
} finally {
  await browser.close()
  server.stop()
}
writeFileSync(join(out, 'summary.json'), `${JSON.stringify({ results, requests }, null, 1)}\n`)
