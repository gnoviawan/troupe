/**
 * Lane 4 of plan section R6: send the same composer draft from the launcher of
 * the React client and of the Rune web client against one seeded world, and
 * compare the prompts the scripted agent received (`PARITY_PROMPT_LOG`).
 *
 *   bun tools/parity/lanes/composer-send.ts --port 8197 --root /tmp/rune-r6c/lane4
 *
 * Each client sends twice: from the launcher (a new chat), then in that chat.
 * Writes `react.png`, `rune.png`, `react-chat.png`, `rune-chat.png` and
 * `prompts.log` under the root and exits 1 when a pair of prompts differs.
 */

import { readFileSync, rmSync } from 'node:fs'
import path from 'node:path'
import { parseArgs } from 'node:util'
import { chromium, type Page } from 'playwright'
import { REFERENCE_ARGS } from '../drivers/chromium'
import { runeWebDist, serveRuneClient } from '../drivers/rune-apps'
import { mirrorPoint, openRuneWeb, runeWebArgs } from '../drivers/rune-web'
import { REPO_DIR } from '../server'
import { createWorld, prepareDevice, TOKEN } from '../world'

const { values } = parseArgs({
  args: Bun.argv.slice(2),
  options: {
    port: { type: 'string', default: '8197' },
    root: { type: 'string', default: '/tmp/troupe-parity/lane4' },
    'server-bin': { type: 'string' },
    dist: { type: 'string', default: path.join(REPO_DIR, 'dist-web') }
  }
})
const root = path.resolve(values.root as string)
const log = path.join(root, 'prompts.log')
rmSync(log, { force: true })
process.env.PARITY_PROMPT_LOG = log

const DRAFT_BEFORE = 'Use /matrix'
const DRAFT_AFTER = 'on the router, please'
const VIEWPORT = { width: 1280, height: 800 }

function prompts(): string[] {
  try {
    return readFileSync(log, 'utf8').trim().split('\n').filter(Boolean)
  } catch {
    return []
  }
}

async function waitForPrompt(count: number): Promise<void> {
  for (let i = 0; i < 100 && prompts().length < count; i++) await Bun.sleep(200)
}

/** Types the draft with a skill pick in the middle, then sends it. */
async function compose(page: Page, field: { x: number; y: number }): Promise<void> {
  await page.mouse.click(field.x, field.y)
  await page.keyboard.type(DRAFT_BEFORE, { delay: 20 })
  await page.waitForTimeout(800)
  await page.keyboard.press('Enter')
  await page.keyboard.type(DRAFT_AFTER, { delay: 20 })
  await page.waitForTimeout(300)
  await page.keyboard.press('Enter')
}

const world = await createWorld({ port: Number(values.port), root, binary: values['server-bin'] })
const browser = await chromium.launch({ args: runeWebArgs(REFERENCE_ARGS) })
try {
  const seeded = prompts().length
  const dist = path.resolve(values.dist as string)
  await prepareDevice(world, 'lane4-react')
  const context = await browser.newContext({
    viewport: VIEWPORT,
    colorScheme: 'dark',
    reducedMotion: 'reduce',
    serviceWorkers: 'block'
  })
  await context.route(`${world.server.baseUrl}/**`, async (route) => {
    const url = new URL(route.request().url())
    const file = path.join(dist, url.pathname === '/' ? 'index.html' : url.pathname)
    if (
      route.request().method() === 'GET' &&
      file.startsWith(dist) &&
      (await Bun.file(file).exists())
    ) {
      await route.fulfill({ path: file })
    } else await route.continue()
  })
  await context.addInitScript(
    ({ token }) => {
      localStorage.setItem('termul.webAuthToken', token)
      localStorage.setItem('termul.deviceId', 'lane4-react')
      localStorage.setItem('termul.renameNotice', 'dismissed')
    },
    { token: TOKEN }
  )
  const react = await context.newPage()
  await react.goto(`${world.server.baseUrl}/`)
  const editor = react.locator('[contenteditable="true"]').first()
  await editor.waitFor({ state: 'visible', timeout: 60_000 })
  const box = await editor.boundingBox()
  if (!box) throw new Error('the React composer has no box')
  await react.waitForTimeout(1500)
  await compose(react, { x: box.x + 20, y: box.y + box.height / 2 })
  await waitForPrompt(seeded + 1)
  await react.waitForTimeout(3000)
  await react.screenshot({ path: path.join(root, 'react.png') })
  const chatBox = await react.locator('[contenteditable="true"]').first().boundingBox()
  if (!chatBox) throw new Error('the React chat has no composer')
  await compose(react, { x: chatBox.x + 20, y: chatBox.y + chatBox.height / 2 })
  await waitForPrompt(seeded + 2)
  await react.waitForTimeout(3000)
  await react.screenshot({ path: path.join(root, 'react-chat.png') })
  await context.close()

  await prepareDevice(world, 'lane4-rune')
  const served = await serveRuneClient(runeWebDist('troupe'), world.server.baseUrl)
  const rune = await openRuneWeb({
    browser,
    url: `${served.url}/`,
    ready: 'What should we do in alpha?',
    viewport: VIEWPORT,
    storage: { 'termul.webAuthToken': TOKEN, 'termul.deviceId': 'lane4-rune' }
  })
  const field = await mirrorPoint(rune.page, 'Message', false)
  await rune.page.waitForTimeout(1500)
  await compose(rune.page, field ?? { x: box.x + 20, y: box.y + box.height / 2 })
  await waitForPrompt(seeded + 3)
  await rune.page.waitForTimeout(3000)
  await rune.page.screenshot({ path: path.join(root, 'rune.png') })
  const chatField = await mirrorPoint(rune.page, 'Message', false)
  if (!chatField) throw new Error('the Rune chat has no composer')
  await compose(rune.page, chatField)
  await waitForPrompt(seeded + 4)
  await rune.page.waitForTimeout(3000)
  await rune.page.screenshot({ path: path.join(root, 'rune-chat.png') })
  await rune.close()
  await served.close()

  const sent = prompts().slice(seeded)
  const pairs = [
    ['launcher', sent[0], sent[2]],
    ['chat', sent[1], sent[3]]
  ] as const
  for (const [where, a, b] of pairs)
    console.log(`${where}\n  react: ${a ?? '(none)'}\n  rune:  ${b ?? '(none)'}`)
  const same = sent.length === 4 && pairs.every(([, a, b]) => a === b)
  console.log(same ? 'PASS: the prompts are byte for byte equal' : 'FAIL: the prompts differ')
  process.exitCode = same ? 0 : 1
} finally {
  await browser.close()
  world.ws.close()
  await world.server.stop()
}
