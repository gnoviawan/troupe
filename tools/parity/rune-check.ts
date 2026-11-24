/**
 * `bun tools/parity/rune-check.ts [--out DIR] [--only web|desktop]`
 *
 * Proves both Rune drivers on the `hello` app before the matrix trusts them.
 * Each driver finds the button by its text and presses it, reads "Pressed 1",
 * presses it again by another route (the web driver at the point it
 * recorded, the desktop driver by the button's key), reads "Pressed 2", and
 * saves one frame per press under `--out`. Exits 1 when any step fails.
 */

import { mkdirSync, rmSync } from 'node:fs'
import path from 'node:path'
import { parseArgs } from 'node:util'
import { chromium } from 'playwright'
import { REFERENCE_ARGS } from './drivers/chromium'
import type { Driver, Points } from './drivers/driver'
import { RUNE_APPS, runeBinary, runeWebDist, serveStatic } from './drivers/rune-apps'
import { openRuneDesktop } from './drivers/rune-desktop'
import { openRuneWeb, runeWebArgs } from './drivers/rune-web'
import type { Action, Check } from './types'

const { values } = parseArgs({
  args: Bun.argv.slice(2),
  options: {
    out: { type: 'string', default: '/tmp/troupe-parity/rune-check' },
    only: { type: 'string' }
  }
})
const OUT = path.resolve(values.out as string)

interface Press {
  step: string
  action: Action
  expect: Check
}

const press = (step: string, action: Action, text: string): Press => ({
  step,
  action,
  expect: { text }
})

/** Runs each press through `driver`, printing one line per action and check. */
async function drive(
  name: string,
  driver: Driver,
  presses: Press[],
  points: Points
): Promise<boolean> {
  let ok = true
  for (const { step, action, expect } of presses) {
    const log = await driver.act(step, 0, action)
    const check = await driver.check?.(step, expect)
    const frame = path.join(OUT, `${name}-${step}.png`)
    await driver.grab(frame)
    const at = points.get(`${step}/0`)
    console.log(
      `[rune-check] ${name} ${log.action}: ${log.ok ? 'ok' : `FAIL ${log.error}`} via ${log.via}${at ? ` at ${Math.round(at.x)},${Math.round(at.y)}` : ''}; reads "${expect.text}": ${check?.pass ? 'pass' : 'FAIL'}; frame ${frame}`
    )
    ok &&= log.ok && !!check?.pass
  }
  return ok
}

async function web(): Promise<boolean> {
  const server = await serveStatic(runeWebDist('hello'))
  const browser = await chromium.launch({ args: runeWebArgs(REFERENCE_ARGS) })
  try {
    const points: Points = new Map()
    const session = await openRuneWeb({
      browser,
      url: `${server.url}/`,
      ready: RUNE_APPS.hello.ready,
      points
    })
    console.log(`[rune-check] web drew with ${session.renderer}`)
    try {
      const first = press('by-text', { do: 'click', target: { text: 'Press me' } }, 'Pressed 1')
      const byText = await drive('web', session.driver, [first], points)
      const recorded = points.get('by-text/0')
      if (!recorded) return false
      const second = press('by-point', { do: 'click', target: { point: recorded } }, 'Pressed 2')
      return byText && (await drive('web', session.driver, [second], points))
    } finally {
      await session.close()
    }
  } finally {
    await browser.close()
    await server.close()
  }
}

async function desktop(): Promise<boolean> {
  const stateDir = path.join(OUT, 'desktop-state')
  rmSync(stateDir, { recursive: true, force: true })
  mkdirSync(stateDir, { recursive: true })
  const points: Points = new Map()
  const session = await openRuneDesktop({
    bin: runeBinary('hello'),
    stateDir,
    ready: RUNE_APPS.hello.ready,
    points
  })
  try {
    return await drive(
      'desktop',
      session.driver,
      [
        press('by-text', { do: 'click', target: { text: 'Press me' } }, 'Pressed 1'),
        press('by-key', { do: 'click', target: { testId: 'press' } }, 'Pressed 2')
      ],
      points
    )
  } finally {
    await session.close()
  }
}

mkdirSync(OUT, { recursive: true })
const results: Record<string, boolean> = {}
for (const [name, run] of [
  ['web', web],
  ['desktop', desktop]
] as const) {
  if (values.only && values.only !== name) continue
  try {
    results[name] = await run()
  } catch (error) {
    console.log(`[rune-check] ${name} FAIL ${error instanceof Error ? error.message : error}`)
    results[name] = false
  }
}
console.log(
  `[rune-check] ${Object.entries(results)
    .map(([n, ok]) => `${n} ${ok ? 'pass' : 'FAIL'}`)
    .join(', ')}`
)
process.exit(Object.values(results).every(Boolean) ? 0 : 1)
