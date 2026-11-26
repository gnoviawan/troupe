/**
 * The R8c command editor lane: each client opens a terminal and waits for the command editor
 * the shell's trusted prompt mounts, types a two-line command, asks for a completion with Tab
 * and accepts it, sends it, then recalls it with Arrow Up and sends it again. The page's
 * `/terminal/ws` frames are read over CDP, so the lane compares the `command_editor_submit`
 * drafts the clients sent: the host writes exactly that draft to the PTY. The command writes
 * a file, so the lane also checks what the shell ran.
 *
 *   bun tools/parity/lanes/command-editor.ts --port 8193 --root /tmp/rune-r8c/editor \
 *     --server-bin <troupe-server> --dist <dist-web> [--only react|rune]
 */

import { mkdirSync, readFileSync, rmSync } from 'node:fs'
import path from 'node:path'
import { parseArgs } from 'node:util'
import { type Browser, chromium, type Page } from 'playwright'
import { REFERENCE_ARGS } from '../drivers/chromium'
import { mirrorPoint, runeWebArgs } from '../drivers/rune-web'
import { REPO_DIR } from '../server'
import { createWorld, type World } from '../world'
import { type Client, inReact, inRune, type LaneOptions, shot, until, writePairs } from './kit'

const { values } = parseArgs({
  args: Bun.argv.slice(2),
  options: {
    port: { type: 'string', default: '8193' },
    root: { type: 'string', default: '/tmp/troupe-parity/command-editor' },
    'server-bin': { type: 'string' },
    dist: { type: 'string', default: path.join(REPO_DIR, 'dist-web') },
    only: { type: 'string' }
  }
})
const options: LaneOptions = {
  root: path.resolve(values.root as string),
  phone: false,
  dist: path.resolve(values.dist as string),
  device: 'command-editor'
}
const TARGET = 'completion-target-dir'
const OUT = 'r8c-out.txt'
const WANT_DRAFT = `printf '%s\\n' "multi\nline" > ${OUT} && ls -d ${TARGET}`

interface Result {
  ok: boolean
  drafts: string[]
  ran: string
  notes: string[]
}

/** The `command_editor_submit` drafts the page sends from now on. */
async function watchSubmits(page: Page): Promise<string[]> {
  const drafts: string[] = []
  const cdp = await page.context().newCDPSession(page)
  await cdp.send('Network.enable')
  cdp.on('Network.webSocketFrameSent', (e: { response: { payloadData: string } }) => {
    try {
      const frame = JSON.parse(e.response.payloadData)
      if (frame?.type === 'command_editor_submit') drafts.push(frame.payload.draft)
    } catch {
      // Not a JSON frame.
    }
  })
  return drafts
}

async function editorShown(client: Client): Promise<boolean> {
  if (client.name === 'react')
    return (await client.page.locator('[data-slot="command-editor-block"]').count()) > 0
  return (await mirrorPoint(client.page, 'Command editor', true)) !== null
}

async function run(client: Client, world: World): Promise<Result> {
  const notes: string[] = []
  const check = (ok: boolean, what: string) => {
    notes.push(`${ok ? 'ok' : 'FAIL'} ${what}`)
    console.log(`${client.name}: ${ok ? 'ok' : 'FAIL'} ${what}`)
    return ok
  }
  const { page } = client
  const out = path.join(world.dirs.alpha, OUT)
  rmSync(out, { force: true })
  const drafts = await watchSubmits(page)
  await page.waitForTimeout(1500)
  await client.click('New terminal')
  let ok = check(await client.sees('Terminal 1', 30_000), 'tab "Terminal 1" opened')
  const mounted = await until(() => editorShown(client), 30_000)
  ok = check(mounted, 'the trusted prompt mounted the command editor') && ok
  await shot(client, options.root, '1-open')
  const box =
    client.name === 'react'
      ? await page
          .locator('[data-slot="command-editor-block"] .cm-content')
          .first()
          .boundingBox({ timeout: 5000 })
          .catch(() => null)
      : null
  const at =
    client.name === 'react'
      ? box && { x: box.x + 40, y: box.y + box.height / 2 }
      : await mirrorPoint(page, 'Command editor', true)
  if (at) await page.mouse.click(at.x, at.y)
  await page.waitForTimeout(500)
  await page.keyboard.type(`printf '%s\\n' "multi`, { delay: 20 })
  await page.keyboard.press('Shift+Enter')
  await page.keyboard.type(`line" > ${OUT} && ls -d compl`, { delay: 20 })
  await shot(client, options.root, '2-typed')
  await page.keyboard.press('Tab')
  await page.waitForTimeout(1500)
  await shot(client, options.root, '3-completion')
  await page.keyboard.press('Tab')
  await page.waitForTimeout(500)
  await shot(client, options.root, '4-accepted')
  await page.keyboard.press('Enter')
  const sent = await until(async () => drafts.length > 0, 10_000)
  ok =
    check(sent && drafts[0] === WANT_DRAFT, `the submit carried ${JSON.stringify(drafts[0])}`) && ok
  await until(async () => (await Bun.file(out).exists()) && Bun.file(out).size > 0, 10_000)
  const ran = (await Bun.file(out).exists()) ? readFileSync(out, 'utf8') : ''
  ok = check(ran === 'multi\nline\n', `the shell ran it (${JSON.stringify(ran)})`) && ok
  await until(() => editorShown(client), 15_000)
  await page.waitForTimeout(1000)
  await shot(client, options.root, '5-sent')
  await page.keyboard.press('ArrowUp')
  await page.waitForTimeout(800)
  await shot(client, options.root, '6-recalled')
  await page.keyboard.press('Enter')
  const again = await until(async () => drafts.length > 1, 10_000)
  ok =
    check(
      again && drafts[1] === WANT_DRAFT,
      `Arrow Up recalled and resent ${JSON.stringify(drafts[1])}`
    ) && ok
  await page.waitForTimeout(1500)
  await shot(client, options.root, '7-resent')
  return { ok, drafts, ran, notes }
}

const world = await createWorld({
  port: Number(values.port),
  root: options.root,
  binary: values['server-bin']
})
let browser: Browser | undefined
try {
  mkdirSync(path.join(world.dirs.alpha, TARGET), { recursive: true })
  browser = await chromium.launch({ args: runeWebArgs(REFERENCE_ARGS) })
  const results: Array<[string, Result]> = []
  if (values.only !== 'rune')
    results.push(['react', await inReact(browser, world, options, (c) => run(c, world))])
  if (values.only !== 'react')
    results.push(['rune', await inRune(browser, world, options, (c) => run(c, world))])
  for (const [name, r] of results) console.log(`${name}: ${r.ok ? 'PASS' : 'FAIL'}`)
  const same =
    results.length === 2 &&
    JSON.stringify(results[0][1].drafts) === JSON.stringify(results[1][1].drafts)
  if (results.length === 2)
    console.log(`the two clients sent ${same ? 'the same' : 'DIFFERENT'} drafts`)
  await Bun.write(
    path.join(options.root, `results-${values.only ?? 'both'}.json`),
    JSON.stringify(results, null, 2)
  )
  process.exitCode = results.every(([, r]) => r.ok) && (results.length < 2 || same) ? 0 : 1
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
