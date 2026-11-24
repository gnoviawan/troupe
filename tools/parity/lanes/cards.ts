/**
 * The R6e card lane: in a chat started from the launcher, the matrix agent runs one tool call
 * of each shape (`matrix tools`), shares a plan (`matrix plan`) and fails a turn
 * (`matrix error`). Each client opens the turn's steps, a command's output and the full diff,
 * folds the plan, and opens the error's details. Both clients run against one seeded world.
 *
 *   bun tools/parity/lanes/cards.ts --port 8199 --root /tmp/rune-r6e/cards \
 *     --server-bin <troupe-server> --dist <dist-web> [--phone]
 *
 * Writes `<client>-<step>.png` and `pair-<step>.png` under the root; exits 1 when a client
 * misses a step.
 */

import path from 'node:path'
import { parseArgs } from 'node:util'
import { type Browser, chromium } from 'playwright'
import { MATRIX_PROMPTS, PLAN_ENTRIES, TOOLS_REPLY } from '../agent/matrix-agent'
import { REFERENCE_ARGS } from '../drivers/chromium'
import { runeWebArgs } from '../drivers/rune-web'
import { REPO_DIR } from '../server'
import { createWorld } from '../world'
import { type Client, inReact, inRune, type LaneOptions, shot, writePairs } from './kit'

const { values } = parseArgs({
  args: Bun.argv.slice(2),
  options: {
    port: { type: 'string', default: '8199' },
    root: { type: 'string', default: '/tmp/troupe-parity/cards' },
    'server-bin': { type: 'string' },
    dist: { type: 'string', default: path.join(REPO_DIR, 'dist-web') },
    phone: { type: 'boolean', default: false },
    only: { type: 'string' }
  }
})
const options: LaneOptions = {
  root: path.resolve(values.root as string),
  phone: values.phone === true,
  dist: path.resolve(values.dist as string),
  device: 'cards'
}
const STEPS = ['tools', 'output', 'diff', 'plan', 'plan-folded', 'error', 'details']

/** Runs the card steps; returns the ones whose marker showed. */
async function run(client: Client): Promise<string[]> {
  const seen: string[] = []
  const step = async (name: string, marker: string) => {
    if (await client.sees(marker, 30_000)) seen.push(name)
    else console.log(`${client.name}: ${name} never showed "${marker}"`)
    await shot(client, options.root, `${STEPS.indexOf(name) + 1}-${name}`)
  }
  const tryClick = async (text: string, exact = true) => {
    try {
      await client.click(text, exact)
    } catch (error) {
      console.log(`${client.name}: could not press "${text}": ${error}`)
    }
  }
  await client.page.waitForTimeout(1500)
  await client.send(MATRIX_PROMPTS.tools)
  await client.sees(TOOLS_REPLY)
  await tryClick('Worked for', false)
  await tryClick('Read 1 file, edited 1 file', false)
  await step('tools', 'bun test router')
  await tryClick('Ran bun test router')
  await step('output', '6 pass')
  await tryClick('Show full diff of routes.ts')
  await step('diff', "route12 = '/r12'")

  await client.send(MATRIX_PROMPTS.plan)
  await step('plan', PLAN_ENTRIES[2])
  await tryClick('Plan, 1 of 3 tasks', false)
  await client.page.waitForTimeout(400)
  const folded = !(await client.sees(PLAN_ENTRIES[2], 1500))
  if (folded) seen.push('plan-folded')
  await shot(client, options.root, '5-plan-folded')

  await client.send(MATRIX_PROMPTS.error)
  await step('error', 'The last turn did not finish.')
  await tryClick('Details')
  await step('details', 'Code -32603')
  await tryClick('Dismiss error')
  return seen
}

const world = await createWorld({
  port: Number(values.port),
  root: options.root,
  binary: values['server-bin']
})
let browser: Browser | undefined
try {
  browser = await chromium.launch({ args: runeWebArgs(REFERENCE_ARGS) })
  const results: Array<[string, string[]]> = []
  if (values.only !== 'rune') results.push(['react', await inReact(browser, world, options, run)])
  if (values.only !== 'react') results.push(['rune', await inRune(browser, world, options, run)])
  let pass = true
  for (const [name, seen] of results) {
    const ok = STEPS.every((s) => seen.includes(s))
    pass &&= ok
    console.log(`${name}: ${ok ? 'PASS' : 'FAIL'} ${JSON.stringify(seen)}`)
  }
  process.exitCode = pass ? 0 : 1
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
