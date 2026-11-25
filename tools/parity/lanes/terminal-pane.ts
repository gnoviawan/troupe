/**
 * The R7b pane lane: each client opens a terminal and shows `depth/term-box.sh` (joined box
 * drawing), answers a mouse-reporting program's click and a bracketed-paste program's paste
 * (`depth/term-input.py` records the bytes each client sent), takes two frames 600 ms apart
 * to see the cursor blink, opens the pane's context menu, and opens the command history modal
 * and its clear confirm. The history shortcut has no web default, so the lane binds it to
 * Alt+R in the shared shortcut record both clients read.
 *
 *   bun tools/parity/lanes/terminal-pane.ts --port 8197 --root /tmp/rune-r7b/pane \
 *     --server-bin <troupe-server> --dist <dist-web> [--only react|rune]
 */

import { readFileSync, rmSync } from 'node:fs'
import path from 'node:path'
import { parseArgs } from 'node:util'
import { type Browser, chromium } from 'playwright'
import { REFERENCE_ARGS } from '../drivers/chromium'
import { runeWebArgs } from '../drivers/rune-web'
import { readPng } from '../png'
import { REPO_DIR } from '../server'
import { createWorld, seedCommandHistory, type World } from '../world'
import { type Client, inReact, inRune, type LaneOptions, shot, until, writePairs } from './kit'

const { values } = parseArgs({
  args: Bun.argv.slice(2),
  options: {
    port: { type: 'string', default: '8197' },
    root: { type: 'string', default: '/tmp/troupe-parity/terminal-pane' },
    'server-bin': { type: 'string' },
    dist: { type: 'string', default: path.join(REPO_DIR, 'dist-web') },
    only: { type: 'string' }
  }
})
const options: LaneOptions = {
  root: path.resolve(values.root as string),
  phone: false,
  dist: path.resolve(values.dist as string),
  device: 'terminal-pane'
}
const DEPTH = path.join(REPO_DIR, 'tools/parity/depth')
const CELL = { w: 8, h: 19 }
/** The SGR press and release a left click on column 10, row 5 (from 0) reports. */
const CLICK = { col: 10, row: 5, want: "b'\\x1b[<0;11;6M\\x1b[<0;11;6m'" }
const PASTED = "b'\\x1b[200~ls -1\\rpwd\\x1b[201~'"

interface Result {
  ok: boolean
  mouse: string
  paste: string
  blinkPixels: number
  notes: string[]
}

let origin = { x: 0, y: 0 }
const cell = (col: number, row: number) => ({
  x: origin.x + col * CELL.w + CELL.w / 2,
  y: origin.y + row * CELL.h + CELL.h / 2
})

/** Pixels that differ between two same-size frames. */
function changed(a: string, b: string): number {
  const [x, y] = [readPng(a), readPng(b)]
  let n = 0
  for (let i = 0; i < x.data.length; i += 4) {
    if (Math.abs(x.data[i] - y.data[i]) + Math.abs(x.data[i + 1] - y.data[i + 1]) > 24) n++
  }
  return n
}

async function probe(client: Client, mode: 'mouse' | 'paste'): Promise<string> {
  const out = path.join(options.root, `${mode}-${client.name}.txt`)
  rmSync(out, { force: true })
  await client.page.keyboard.type(`clear; python3 ${DEPTH}/term-input.py ${mode} ${out}`, {
    delay: 15
  })
  await client.page.keyboard.press('Enter')
  await client.page.waitForTimeout(1500)
  if (mode === 'mouse') {
    const at = cell(CLICK.col, CLICK.row)
    await client.page.mouse.click(at.x, at.y)
  } else {
    await client.page.evaluate(() => navigator.clipboard.writeText('ls -1\npwd'))
    if (client.name === 'react') {
      // Headless Chromium fires no paste event for a synthetic Ctrl+V; xterm.js pastes from
      // the event its textarea gets, so hand it one.
      await client.page.evaluate(() => {
        const data = new DataTransfer()
        data.setData('text/plain', 'ls -1\npwd')
        document
          .querySelector('.xterm-helper-textarea')
          ?.dispatchEvent(new ClipboardEvent('paste', { clipboardData: data, bubbles: true }))
      })
    } else {
      await client.page.keyboard.press('Control+v')
    }
  }
  await until(async () => (await Bun.file(out).exists()) && Bun.file(out).size > 0, 10_000)
  return (await Bun.file(out).exists()) ? readFileSync(out, 'utf8') : ''
}

async function run(client: Client, world: World): Promise<Result> {
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
  await page.mouse.click(origin.x + 300, origin.y + 300)
  await page.waitForTimeout(1000)
  await page.keyboard.type(`clear; bash ${DEPTH}/term-box.sh`, { delay: 15 })
  await page.keyboard.press('Enter')
  await page.waitForTimeout(2000)
  await shot(client, options.root, '4-box')

  const a = path.join(options.root, `blink-${client.name}-a.png`)
  const b = path.join(options.root, `blink-${client.name}-b.png`)
  await page.screenshot({ path: a })
  await page.waitForTimeout(600)
  await page.screenshot({ path: b })
  const blinkPixels = changed(a, b)
  const blinked = check(
    blinkPixels > 0,
    `the cursor blinked (${blinkPixels} pixels changed in 600 ms)`
  )
  if (client.name === 'rune') ok = blinked && ok

  const mouse = await probe(client, 'mouse')
  ok = check(mouse === CLICK.want, `mouse report ${mouse}`) && ok
  const paste = await probe(client, 'paste')
  ok = check(paste === PASTED, `paste ${paste}`) && ok

  const menuAt = cell(30, 8)
  await page.mouse.click(menuAt.x, menuAt.y, { button: 'right' })
  ok = check(await client.sees('Restart Terminal', 10_000), 'the context menu opened') && ok
  await shot(client, options.root, '5-menu')
  await page.keyboard.press('Escape')
  await page.waitForTimeout(500)

  await page.mouse.click(menuAt.x, menuAt.y)
  await page.keyboard.press('Alt+r')
  ok = check(await client.sees('Command History', 10_000), 'the history modal opened') && ok
  ok =
    check(
      await client.sees('bun vitest run src/renderer/lib', 10_000),
      'it lists the seeded command'
    ) && ok
  await shot(client, options.root, '6-history')
  await client.click('Clear History')
  ok = check(await client.sees('Clear Command History', 10_000), 'Clear History asks first') && ok
  await shot(client, options.root, '7-clear-confirm')
  await page.keyboard.press('Escape')
  await page.waitForTimeout(300)
  await page.keyboard.press('Escape')
  void world
  return { ok, mouse, paste, blinkPixels, notes }
}

const world = await createWorld({
  port: Number(values.port),
  root: options.root,
  binary: values['server-bin']
})
let browser: Browser | undefined
try {
  const current = await world.ws.request<{ revision?: number }>('store_read', {
    key: 'shared/lww/settings/keyboard-shortcuts'
  })
  await world.ws.request('store_write', {
    key: 'shared/lww/settings/keyboard-shortcuts',
    value: { _version: 1, data: { commandHistory: { id: 'commandHistory', customKey: 'alt+r' } } },
    baseRevision: current.revision ?? 0
  })
  for (const name of ['react', 'rune']) await seedCommandHistory(world, `${options.device}-${name}`)
  browser = await chromium.launch({ args: runeWebArgs(REFERENCE_ARGS) })
  const results: Array<[string, Result]> = []
  if (values.only !== 'rune')
    results.push(['react', await inReact(browser, world, options, (c) => run(c, world))])
  if (values.only !== 'react')
    results.push(['rune', await inRune(browser, world, options, (c) => run(c, world))])
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
