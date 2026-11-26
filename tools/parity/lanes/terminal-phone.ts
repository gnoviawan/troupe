/**
 * The R7b phone lane (plan R7 lane 9): on the phone profile each client opens a terminal from
 * the header's More sheet, runs `depth/term-input.py keys`, and sends Ctrl+C, the left arrow
 * and Tab from the key bar; the probe records the bytes each client sent. The pair image shows
 * the terminal with its key bar.
 *
 *   bun tools/parity/lanes/terminal-phone.ts --port 8197 --root /tmp/rune-r7b/phone \
 *     --server-bin <troupe-server> --dist <dist-web> [--only react|rune]
 */

import { readFileSync, rmSync } from 'node:fs'
import path from 'node:path'
import { parseArgs } from 'node:util'
import { type Browser, chromium } from 'playwright'
import { REFERENCE_ARGS } from '../drivers/chromium'
import { runeWebArgs } from '../drivers/rune-web'
import { REPO_DIR } from '../server'
import { createWorld } from '../world'
import { type Client, inReact, inRune, type LaneOptions, shot, until, writePairs } from './kit'

const { values } = parseArgs({
  args: Bun.argv.slice(2),
  options: {
    port: { type: 'string', default: '8197' },
    root: { type: 'string', default: '/tmp/troupe-parity/terminal-phone' },
    'server-bin': { type: 'string' },
    dist: { type: 'string', default: path.join(REPO_DIR, 'dist-web') },
    only: { type: 'string' }
  }
})
const options: LaneOptions = {
  root: path.resolve(values.root as string),
  phone: true,
  dist: path.resolve(values.dist as string),
  device: 'terminal-phone'
}
const PROBE = path.join(REPO_DIR, 'tools/parity/depth/term-input.py')
const WANT = "b'\\x03\\x1b[D\\t'"

interface Result {
  ok: boolean
  keys: string
  notes: string[]
}

async function run(client: Client): Promise<Result> {
  const notes: string[] = []
  const check = (ok: boolean, what: string) => {
    notes.push(`${ok ? 'ok' : 'FAIL'} ${what}`)
    console.log(`${client.name}: ${ok ? 'ok' : 'FAIL'} ${what}`)
    return ok
  }
  const { page } = client
  await page.waitForTimeout(1500)
  await client.click('More')
  await client.click('New terminal')
  let ok = check(await client.sees('Ctrl+C', 30_000), 'the terminal opened with its key bar')
  await page.waitForTimeout(2500)
  const vp = page.viewportSize() ?? { width: 390, height: 844 }
  await page.mouse.click(vp.width / 2, vp.height / 3)
  await page.waitForTimeout(800)
  const out = path.join(options.root, `keys-${client.name}.txt`)
  rmSync(out, { force: true })
  await page.keyboard.type(`clear; python3 ${PROBE} keys ${out}`, { delay: 15 })
  await page.keyboard.press('Enter')
  await page.waitForTimeout(2000)
  await shot(client, options.root, '8-phone-keys')
  // Keys the Rune lane shows without scrolling it (the mirror has no scroll-into-view).
  for (const key of ['Ctrl+C, interrupt', 'Left arrow', 'Tab']) {
    await client.click(key)
    await page.waitForTimeout(400)
  }
  await until(async () => (await Bun.file(out).exists()) && Bun.file(out).size > 0, 10_000)
  const keys = (await Bun.file(out).exists()) ? readFileSync(out, 'utf8') : ''
  ok = check(keys === WANT, `key bar sent ${keys}`) && ok
  await shot(client, options.root, '9-phone-after')
  return { ok, keys, notes }
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
