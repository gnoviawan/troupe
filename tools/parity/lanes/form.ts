/**
 * The R6e form lane: in a chat started from the launcher, the matrix agent asks a form
 * elicitation (`matrix form`). Each client submits it empty (the required field stops it),
 * types a whole number, submits, and reads the agent's echo of the answer.
 *
 *   bun tools/parity/lanes/form.ts --port 8199 --root /tmp/rune-r6e/form \
 *     --server-bin <troupe-server> --dist <dist-web> [--phone]
 */

import path from 'node:path'
import { parseArgs } from 'node:util'
import { type Browser, chromium } from 'playwright'
import { FORM_MESSAGE, MATRIX_PROMPTS } from '../agent/matrix-agent'
import { REFERENCE_ARGS } from '../drivers/chromium'
import { runeWebArgs } from '../drivers/rune-web'
import { REPO_DIR } from '../server'
import { createWorld } from '../world'
import { type Client, inReact, inRune, type LaneOptions, shot, writePairs } from './kit'

const { values } = parseArgs({
  args: Bun.argv.slice(2),
  options: {
    port: { type: 'string', default: '8199' },
    root: { type: 'string', default: '/tmp/troupe-parity/form' },
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
  device: 'form'
}
const ANSWER = 'Form answer: accept {"runs":3}'

async function run(client: Client): Promise<boolean> {
  await client.page.waitForTimeout(1500)
  await client.send(MATRIX_PROMPTS.form)
  await client.sees(FORM_MESSAGE)
  await shot(client, options.root, '1-form')
  await client.click('Submit')
  await shot(client, options.root, '2-required')
  await client.fill('Runs', '3')
  await client.click('Submit')
  const ok = await client.sees(ANSWER, 30_000)
  await shot(client, options.root, '3-answered')
  return ok
}

const world = await createWorld({
  port: Number(values.port),
  root: options.root,
  binary: values['server-bin']
})
let browser: Browser | undefined
try {
  browser = await chromium.launch({ args: runeWebArgs(REFERENCE_ARGS) })
  const results: Array<[string, boolean]> = []
  if (values.only !== 'rune') results.push(['react', await inReact(browser, world, options, run)])
  if (values.only !== 'react') results.push(['rune', await inRune(browser, world, options, run)])
  for (const [name, ok] of results) console.log(`${name}: ${ok ? 'PASS' : 'FAIL'}`)
  process.exitCode = results.every(([, ok]) => ok) ? 0 : 1
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
