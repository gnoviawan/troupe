/**
 * The R8b Markdown lane: each client opens the same Markdown file in its document view
 * (seeded as an editor tab in preview mode), adds a heading in front of the first block and
 * saves; the file on disk is diffed against the original. Then the page reloads, the file is
 * saved again with no edit, and its bytes must not change.
 *
 *   bun tools/parity/lanes/markdown.ts --port 8193 --root /tmp/rune-r8b/markdown \
 *     --server-bin <troupe-server> --dist <dist-web> [--only react|rune]
 */

import { readFileSync, writeFileSync } from 'node:fs'
import path from 'node:path'
import { parseArgs } from 'node:util'
import { chromium } from 'playwright'
import { REFERENCE_ARGS } from '../drivers/chromium'
import { runeWebArgs } from '../drivers/rune-web'
import { REPO_DIR } from '../server'
import { createWorld, seedEditorTab } from '../world'
import { type Client, inReact, inRune, type LaneOptions, shot, until, writePairs } from './kit'

const { values } = parseArgs({
  args: Bun.argv.slice(2),
  options: {
    port: { type: 'string', default: '8193' },
    root: { type: 'string', default: '/tmp/troupe-parity/markdown' },
    'server-bin': { type: 'string' },
    dist: { type: 'string', default: path.join(REPO_DIR, 'dist-web') },
    only: { type: 'string' }
  }
})
const options: LaneOptions = {
  root: path.resolve(values.root as string),
  phone: false,
  dist: path.resolve(values.dist as string),
  device: 'markdown-lane'
}

const SOURCE = [
  '# Release notes',
  '',
  'Text with *emphasis*, `code` and a [link](https://example.com).  ',
  '',
  '* first item',
  '* second item',
  '',
  '| Column | Value |',
  '|:--|--:|',
  '| a | 1 |',
  '',
  '```mermaid',
  'graph TD',
  '  A[Open] --> B[Save]',
  '```',
  ''
].join('\n')
const HEADING = 'Added heading'

interface Result {
  client: string
  edited: string
  diff: string[]
  resavedEqual: boolean
  notes: string[]
}

/** Lines of `after` that are not in `before`, and lines of `before` gone from `after`. */
function lineDiff(before: string, after: string): string[] {
  const a = before.split('\n')
  const b = after.split('\n')
  return [
    ...b.filter((l) => !a.includes(l)).map((l) => `+${l}`),
    ...a.filter((l) => !b.includes(l)).map((l) => `-${l}`)
  ]
}

async function edit(client: Client, file: string): Promise<string> {
  const page = client.page
  if (!(await client.sees('Release notes', 30_000))) throw new Error('the document never showed')
  await shot(client, options.root, '1-open')
  await client.click('Release notes', false)
  await page.keyboard.press('Home')
  await page.keyboard.type(`# ${HEADING}`, { delay: 30 })
  await page.keyboard.press('Enter')
  await page.waitForTimeout(400)
  await shot(client, options.root, '2-typed')
  await page.keyboard.press('Control+s')
  await until(async () => readFileSync(file, 'utf8').includes(HEADING), 10_000)
  await shot(client, options.root, '3-saved')
  return readFileSync(file, 'utf8')
}

async function resave(client: Client, file: string): Promise<boolean> {
  const page = client.page
  const before = readFileSync(file)
  if (!(await client.sees('Release notes', 30_000)))
    throw new Error('the document never showed again')
  await client.click('Release notes', false)
  await page.keyboard.press('Control+s')
  await page.waitForTimeout(1500)
  await shot(client, options.root, '4-resaved')
  return Buffer.compare(before, readFileSync(file)) === 0
}

const world = await createWorld({
  port: Number(values.port),
  root: options.root,
  binary: values['server-bin'] as string | undefined
})
const file = path.join(world.dirs.alpha, 'NOTES.md')
const browser = await chromium.launch({ args: runeWebArgs(REFERENCE_ARGS) })
const results: Result[] = []
try {
  for (const which of ['react', 'rune'] as const) {
    if (values.only && values.only !== which) continue
    writeFileSync(file, SOURCE)
    const device = `${options.device}-${which}`
    await seedEditorTab(world, device, file, 'preview')
    const inClient = which === 'react' ? inReact : inRune
    const edited = await inClient(browser, world, options, (c) => edit(c, file))
    await seedEditorTab(world, device, file, 'preview')
    const resavedEqual = await inClient(browser, world, options, (c) => resave(c, file))
    results.push({ client: which, edited, diff: lineDiff(SOURCE, edited), resavedEqual, notes: [] })
  }
} finally {
  await browser.close()
  await world.server.stop()
}
writePairs(options.root)
console.log(
  JSON.stringify(
    results.map(({ edited: _, ...r }) => r),
    null,
    2
  )
)
writeFileSync(path.join(options.root, 'markdown-results.json'), JSON.stringify(results, null, 2))
