/**
 * The R6f switch lane: in a chat started from the launcher, switch the agent mid-chat to a
 * second agent (the scripted agent under another config), send so the switch runs, then
 * reload and open the chat again. On a phone the switch goes through the model sheet. Both
 * clients run against one seeded world.
 *
 *   bun tools/parity/lanes/switch.ts --port 8196 --root /tmp/rune-r6f/switch \
 *     --server-bin <troupe-server> --dist <dist-web> [--phone]
 *
 * Writes `<client>-<step>.png` and `pair-<step>.png` under the root; exits 1 when a client
 * misses a step.
 */

import path from 'node:path'
import { parseArgs } from 'node:util'
import { type Browser, chromium } from 'playwright'
import { OPTIONS_ENV } from '../agent/matrix-agent'
import { SCRIPT_PROMPTS } from '../agent/script'
import { REFERENCE_ARGS } from '../drivers/chromium'
import { mirrorPoint, runeWebArgs } from '../drivers/rune-web'
import { REPO_DIR } from '../server'
import { createWorld, SECOND_AGENT, SECOND_AGENT_ENV } from '../world'
import { type Client, inReact, inRune, type LaneOptions, shot, until, writePairs } from './kit'

const { values } = parseArgs({
  args: Bun.argv.slice(2),
  options: {
    port: { type: 'string', default: '8196' },
    root: { type: 'string', default: '/tmp/troupe-parity/switch' },
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
  device: 'switch'
}
process.env[OPTIONS_ENV] = '1'
process.env[SECOND_AGENT_ENV] = '1'
const STEPS = ['chat', 'picker', 'armed', 'switched', 'reopened']
const SECOND = SECOND_AGENT.name
const NEXT = 'Carry on with the next step.'

/** Opens the chat's agent and model control. */
async function openPicker(client: Client): Promise<void> {
  if (client.name === 'react') {
    await client.page.getByTestId('agent-model-selector-trigger').first().click({ timeout: 20_000 })
  } else {
    await client.click('Matrix Large', false)
  }
}

/** Arms the switch to the second agent, through the menu or the phone sheet. */
async function arm(
  client: Client,
  step: (name: string, marker: string) => Promise<void>
): Promise<void> {
  await openPicker(client)
  if (options.phone) {
    await client.click('Provider', false)
    await step('picker', SECOND)
    await client.click(SECOND)
    if (client.name === 'react') await client.click('Matrix Fast')
  } else if (client.name === 'react') {
    await client.page.getByRole('tab', { name: SECOND }).first().click({ timeout: 20_000 })
    await client.sees('Matrix Fast', 30_000)
    await step('picker', SECOND)
    await client.click('Matrix Fast')
  } else {
    await step('picker', SECOND)
    await client.click(SECOND)
  }
  await client.page.keyboard.press('Escape')
  await client.page.waitForTimeout(400)
}

const HANDOFF = `Handoff: Chat Scenario Agent to ${SECOND}`

/** The switch separator shows (its accessible name); brings it into view. */
async function seesHandoff(client: Client, ms = 30_000): Promise<boolean> {
  if (client.name === 'react') {
    const separator = client.page.getByRole('button', { name: HANDOFF }).first()
    const ok = await until(async () => (await separator.count()) > 0, ms)
    if (ok) await separator.scrollIntoViewIfNeeded().catch(() => {})
    return ok
  }
  const ok = await client.sees(HANDOFF, ms)
  if (ok) {
    // The Rune transcript sticks to the bottom: scroll up until the separator is in view.
    const { width, height } = client.page.viewportSize() ?? { width: 1280, height: 800 }
    for (let n = 0; n < 12; n += 1) {
      const point = await mirrorPoint(client.page, HANDOFF, false)
      if (point && point.y > 80 && point.y < height - 260) break
      await client.page.mouse.move(width / 2, height / 3)
      await client.page.mouse.wheel(0, -250)
      await client.page.waitForTimeout(150)
    }
  }
  return ok
}

async function run(client: Client): Promise<string[]> {
  const seen: string[] = []
  const step = async (name: string, marker: string | (() => Promise<boolean>)) => {
    const ok = typeof marker === 'string' ? await client.sees(marker, 30_000) : await marker()
    if (ok) seen.push(name)
    else console.log(`${client.name}: ${name} never showed its marker`)
    await shot(client, options.root, `${STEPS.indexOf(name) + 1}-${name}`)
  }
  const tryStep = async (what: string, act: () => Promise<void>) => {
    try {
      await act()
    } catch (error) {
      console.log(`${client.name}: ${what} failed: ${error}`)
    }
  }
  await client.page.waitForTimeout(1500)
  await client.send(SCRIPT_PROMPTS[0])
  await step('chat', 'Worked for')
  await tryStep('arming', () => arm(client, step))
  await step('armed', '→')
  await client.send(NEXT)
  await client.sees('Worked for 2', 30_000)
  await step('switched', () => seesHandoff(client))
  await client.page.reload()
  await client.sees('What should we do in alpha?', 30_000)
  await tryStep('reopening', () => client.click(NEXT.slice(0, 20), false))
  await client.page.waitForTimeout(3000)
  await step('reopened', () => seesHandoff(client))
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
