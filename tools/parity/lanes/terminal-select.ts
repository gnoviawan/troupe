/**
 * The R7b selection, search and link lane: each client opens a terminal, prints
 * `depth/term-select.sh`, drags across the wrapped line into the next one and copies, opens
 * the Terminal Search shortcut on "needle", and clicks the URL. React has no terminal find bar
 * (no caller hands `ConnectedTerminal` its search handle), so the search step is Rune's alone.
 *
 *   bun tools/parity/lanes/terminal-select.ts --port 8197 --root /tmp/rune-r7b/select \
 *     --server-bin <troupe-server> --dist <dist-web> [--only react|rune]
 */

import path from 'node:path'
import { parseArgs } from 'node:util'
import { type Browser, chromium } from 'playwright'
import { REFERENCE_ARGS } from '../drivers/chromium'
import { runeWebArgs } from '../drivers/rune-web'
import { REPO_DIR } from '../server'
import { createWorld } from '../world'
import { type Client, inReact, inRune, type LaneOptions, shot, writePairs } from './kit'

const { values } = parseArgs({
  args: Bun.argv.slice(2),
  options: {
    port: { type: 'string', default: '8197' },
    root: { type: 'string', default: '/tmp/troupe-parity/terminal-select' },
    'server-bin': { type: 'string' },
    dist: { type: 'string', default: path.join(REPO_DIR, 'dist-web') },
    only: { type: 'string' }
  }
})
const options: LaneOptions = {
  root: path.resolve(values.root as string),
  phone: false,
  dist: path.resolve(values.dist as string),
  device: 'terminal-select'
}
const SCREEN = path.join(REPO_DIR, 'tools/parity/depth/term-select.sh')
/** The terminal's cell box at device scale 1 in both clients (R7a). */
const CELL = { w: 8, h: 19 }

interface Result {
  ok: boolean
  clipboard: string
  link: string
  notes: string[]
}

/** The top-left of the terminal's cells. React's is `.xterm-screen`; Rune draws the same pane. */
let origin = { x: 0, y: 0 }

const cell = (col: number, row: number) => ({
  x: origin.x + col * CELL.w + 1,
  y: origin.y + row * CELL.h + CELL.h / 2
})

async function run(client: Client): Promise<Result> {
  const notes: string[] = []
  const check = (ok: boolean, what: string) => {
    notes.push(`${ok ? 'ok' : 'FAIL'} ${what}`)
    console.log(`${client.name}: ${ok ? 'ok' : 'FAIL'} ${what}`)
    return ok
  }
  const { page } = client
  await page.context().grantPermissions(['clipboard-read', 'clipboard-write'])
  await page.waitForTimeout(1500)
  await client.click('New terminal')
  let ok = check(await client.sees('Terminal 1', 30_000), 'tab "Terminal 1" opened')
  await page.waitForTimeout(2000)
  if (client.name === 'react') {
    const box = await page.locator('.xterm-screen').first().boundingBox()
    if (box) origin = { x: box.x, y: box.y }
  }
  console.log(`${client.name}: cells start at ${origin.x},${origin.y}`)
  await page.mouse.click(origin.x + 200, origin.y + 200)
  await page.keyboard.type(`clear; bash ${SCREEN}`, { delay: 5 })
  await page.keyboard.press('Enter')
  await page.waitForTimeout(2500)
  await page.evaluate(() => navigator.clipboard.writeText('empty'))
  const from = cell(0, 1)
  const to = cell(6, 3)
  await page.mouse.move(from.x, from.y)
  await page.mouse.down()
  await page.mouse.move((from.x + to.x) / 2, (from.y + to.y) / 2, { steps: 5 })
  await page.mouse.move(to.x, to.y, { steps: 5 })
  await page.mouse.up()
  await page.keyboard.press('Control+c')
  await page.waitForTimeout(500)
  const clipboard = await page.evaluate(() => navigator.clipboard.readText())
  ok =
    check(
      clipboard.startsWith('word word') && clipboard.endsWith('\nneedle'),
      `copied ${JSON.stringify(clipboard)}`
    ) && ok
  await shot(client, options.root, '1-selection')
  await page.mouse.click(cell(20, 10).x, cell(20, 10).y)
  if (client.name === 'rune') {
    await page.keyboard.press('Control+f')
    await page.waitForTimeout(400)
    await page.keyboard.type('needle', { delay: 20 })
    ok =
      check(
        (await client.sees('1 of 3', 10_000)) || (await client.sees('3 of 3', 1000)),
        'find counts three matches'
      ) && ok
  }
  await shot(client, options.root, '2-search')
  if (client.name === 'rune') await page.keyboard.press('Escape')
  const url = cell(8, 6)
  await page.mouse.move(url.x, url.y)
  await page.waitForTimeout(400)
  let link = ''
  if (client.name === 'react') {
    const popup = page.waitForEvent('popup', { timeout: 10_000 }).catch(() => null)
    await page.mouse.click(url.x, url.y)
    const opened = await popup
    await opened?.waitForLoadState('commit').catch(() => {})
    link = opened ? opened.url() : ''
    await opened?.close()
    await shot(client, options.root, '3-link')
  } else {
    await page.mouse.click(url.x, url.y)
    ok =
      check(await client.sees('Open external link?', 10_000), 'the external-link confirm opened') &&
      ok
    link = (await client.sees('https://example.com/troupe?x=1', 3000))
      ? 'https://example.com/troupe?x=1'
      : ''
    await shot(client, options.root, '3-link')
    await page.keyboard.press('Escape')
  }
  ok = check(link.startsWith('https://example.com/troupe?x=1'), `link ${link}`) && ok
  return { ok, clipboard, link, notes }
}

const world = await createWorld({
  port: Number(values.port),
  root: options.root,
  binary: values['server-bin']
})
let browser: Browser | undefined
try {
  browser = await chromium.launch({ args: runeWebArgs(REFERENCE_ARGS) })
  const results: Array<[string, Result]> = []
  if (values.only !== 'rune') results.push(['react', await inReact(browser, world, options, run)])
  if (values.only !== 'react') results.push(['rune', await inRune(browser, world, options, run)])
  for (const [name, r] of results) console.log(`${name}: ${r.ok ? 'PASS' : 'FAIL'}`)
  await Bun.write(
    path.join(options.root, `results-${values.only ?? 'both'}.json`),
    JSON.stringify(results, null, 2)
  )
  process.exitCode = results.every(([, r]) => r.ok) ? 0 : 1
} finally {
  try {
    writePairs(options.root)
  } finally {
    try {
      await browser?.close()
    } finally {
      try {
        world.ws.close()
      } finally {
        await world.server.stop()
      }
    }
  }
}
