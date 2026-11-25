/**
 * The R8a editor lane: each client opens the same Rust file in an editor tab (seeded in the
 * device's workspace record), types at its end, saves with Ctrl+S, and the file on disk is
 * compared byte for byte; then find counts a word and replace all rewrites it; then each client
 * types 200 characters into a 1 MB file and the time from each key to the second animation
 * frame after it is recorded.
 *
 *   bun tools/parity/lanes/editor.ts --port 8198 --root /tmp/rune-r8a/editor \
 *     --server-bin <troupe-server> --dist <dist-web> [--only react|rune] [--skip-large]
 */

import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import path from 'node:path'
import { parseArgs } from 'node:util'
import { chromium, type Page } from 'playwright'
import { REFERENCE_ARGS } from '../drivers/chromium'
import { runeWebArgs } from '../drivers/rune-web'
import { REPO_DIR } from '../server'
import { createWorld, seedEditorTab } from '../world'
import { type Client, inReact, inRune, type LaneOptions, shot, until, writePairs } from './kit'

const { values } = parseArgs({
  args: Bun.argv.slice(2),
  options: {
    port: { type: 'string', default: '8198' },
    root: { type: 'string', default: '/tmp/troupe-parity/editor' },
    'server-bin': { type: 'string' },
    dist: { type: 'string', default: path.join(REPO_DIR, 'dist-web') },
    only: { type: 'string' },
    'skip-large': { type: 'boolean', default: false }
  }
})
const options: LaneOptions = {
  root: path.resolve(values.root as string),
  phone: false,
  dist: path.resolve(values.dist as string),
  device: 'editor-lane'
}

const SOURCE = `pub fn total(lines: &[u64]) -> u64 {
    lines.iter().sum()
}

pub fn totals_match(lines: &[u64], expected: u64) -> bool {
    total(lines) == expected
}
`
const TYPED = '// saved from the editor'
const KEYS = 200

interface Result {
  client: string
  saved: string
  matchCount: string
  replaced: string
  keyMs: number[]
  notes: string[]
}

const frames = (page: Page) =>
  page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))))

function percentile(xs: number[], p: number): number {
  const sorted = [...xs].sort((a, b) => a - b)
  return sorted[Math.min(sorted.length - 1, Math.floor((p / 100) * sorted.length))] ?? Number.NaN
}

async function run(client: Client, file: string, large: string | null): Promise<Result> {
  const notes: string[] = []
  const page = client.page
  const r: Result = {
    client: client.name,
    saved: '',
    matchCount: '',
    replaced: '',
    keyMs: [],
    notes
  }
  if (!(await client.sees('totals_match', 30_000))) notes.push('the file never showed')
  await shot(client, options.root, '1-open')
  await client.click('totals_match', false)
  await page.keyboard.press('Control+End')
  await page.keyboard.type(TYPED, { delay: 20 })
  await page.keyboard.press('Control+s')
  await until(async () => readFileSync(file, 'utf8').includes(TYPED), 10_000)
  r.saved = readFileSync(file, 'utf8')
  await shot(client, options.root, '2-saved')
  await page.keyboard.press('Control+f')
  await page.waitForTimeout(400)
  await page.keyboard.type('total', { delay: 20 })
  await page.waitForTimeout(600)
  await shot(client, options.root, '3-find')
  const bodyText = await page.evaluate(() => document.body.innerText)
  r.matchCount = /\d+ of \d+|\d+ results?|No results/.exec(bodyText)?.[0] ?? ''
  await page.keyboard.press('Control+h')
  await page.waitForTimeout(300)
  await page.keyboard.type('sum_all', { delay: 20 })
  await client.click('Replace all', true).catch((e) => notes.push(`replace all: ${e}`))
  await page.keyboard.press('Escape')
  await page.keyboard.press('Control+s')
  await until(async () => readFileSync(file, 'utf8').includes('sum_all'), 10_000)
  r.replaced = readFileSync(file, 'utf8')
  await shot(client, options.root, '4-replaced')
  if (large) {
    notes.push('large file: see keyMs')
  }
  return r
}

async function typeLarge(client: Client): Promise<number[]> {
  const page = client.page
  if (!(await client.sees('large_marker_0', 60_000))) throw new Error('the 1 MB file never showed')
  await client.click('large_marker_0', false)
  await page.keyboard.press('End')
  const out: number[] = []
  for (let i = 0; i < KEYS; i++) {
    const t0 = performance.now()
    await page.keyboard.press('x')
    await frames(page)
    out.push(performance.now() - t0)
  }
  await shot(client, options.root, '5-large')
  return out
}

const world = await createWorld({
  port: Number(values.port),
  root: options.root,
  binary: values['server-bin'] as string | undefined
})
const file = path.join(world.dirs.alpha, 'src', 'editor_lane.rs')
const largeFile = path.join(world.dirs.alpha, 'src', 'large.rs')
mkdirSync(path.dirname(file), { recursive: true })
const largeLines: string[] = []
for (let i = 0; largeLines.join('\n').length < 1_048_576; i++) {
  largeLines.push(
    `fn large_marker_${i}(value: u64) -> u64 { value.wrapping_mul(${i}) + 1 } // line ${i}`
  )
}
const browser = await chromium.launch({ args: runeWebArgs(REFERENCE_ARGS) })
const results: Result[] = []
try {
  for (const which of ['react', 'rune'] as const) {
    if (values.only && values.only !== which) continue
    writeFileSync(file, SOURCE)
    const device = `${options.device}-${which}`
    await seedEditorTab(world, device, file)
    const steps = (client: Client) => run(client, file, null)
    const result =
      which === 'react'
        ? await inReact(browser, world, options, steps)
        : await inRune(browser, world, options, steps)
    if (!values['skip-large']) {
      writeFileSync(largeFile, `${largeLines.join('\n')}\n`)
      await seedEditorTab(world, device, largeFile)
      const large = (client: Client) => typeLarge(client)
      result.keyMs =
        which === 'react'
          ? await inReact(browser, world, options, large)
          : await inRune(browser, world, options, large)
    }
    results.push(result)
  }
} finally {
  await browser.close()
  await world.server.stop()
}
writePairs(options.root)
const summary = results.map((r) => ({
  client: r.client,
  savedEndsWithTyped: r.saved.endsWith(TYPED),
  matchCount: r.matchCount,
  replacedCount: (r.replaced.match(/sum_all/g) ?? []).length,
  keyP50: percentile(r.keyMs, 50),
  keyP95: percentile(r.keyMs, 95),
  notes: r.notes
}))
const [a, b] = results
if (a && b) {
  console.log(`saved byte-equal: ${a.saved === b.saved}`)
  console.log(`replaced byte-equal: ${a.replaced === b.replaced}`)
}
console.log(JSON.stringify(summary, null, 2))
writeFileSync(
  path.join(options.root, 'editor-results.json'),
  JSON.stringify({ summary, results }, null, 2)
)
