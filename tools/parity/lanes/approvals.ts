/**
 * Lane 3 of plan section R6: in a chat started from the launcher, answer a permission prompt
 * with allow, then one with deny, answer an ask-user question, then stop a
 * subagent from its panel, in the React client and in the Rune web client,
 * against one seeded world. The matrix agent echoes each answer into the
 * transcript ("Permission answer: allow-once"), so both clients' answers are
 * read back from the agent's own replies.
 *
 *   bun tools/parity/lanes/approvals.ts --port 8198 --root /tmp/rune-r6d/lane3 \
 *     --server-bin <troupe-server> [--phone]
 *
 * Writes `<client>-<step>.png` per step under the root and exits 1 when a
 * client's replies differ from the expected ones.
 */

import { existsSync, readdirSync } from 'node:fs'
import path from 'node:path'
import { parseArgs } from 'node:util'
import { type Browser, chromium, type Page } from 'playwright'
import { MATRIX_PROMPTS } from '../agent/matrix-agent'
import { REFERENCE_ARGS } from '../drivers/chromium'
import { PHONE_VIEWPORT, VIEWPORT } from '../drivers/react'
import { runeWebDist, serveRuneClient } from '../drivers/rune-apps'
import { mirrorPoint, openRuneWeb, runeWebArgs } from '../drivers/rune-web'
import { blankImage, blit, readPng, writePng } from '../png'
import { REPO_DIR } from '../server'
import { createWorld, prepareDevice, TOKEN, type World } from '../world'

const { values } = parseArgs({
  args: Bun.argv.slice(2),
  options: {
    port: { type: 'string', default: '8198' },
    root: { type: 'string', default: '/tmp/troupe-parity/lane3' },
    'server-bin': { type: 'string' },
    dist: { type: 'string', default: path.join(REPO_DIR, 'dist-web') },
    phone: { type: 'boolean', default: false },
    only: { type: 'string' }
  }
})
const root = path.resolve(values.root as string)
const PHONE = values.phone === true
const EXPECTED = [
  'Permission answer: allow-once',
  'Permission answer: reject-once',
  'Question answer: feature',
  'Stopped child-explore.'
]

/** What a client offers the lane: find text, click it, type into the composer. */
interface Client {
  name: string
  page: Page
  /** Clicks the smallest element that shows `text` (or names it), waiting up to 20 s. */
  click(text: string, exact?: boolean): Promise<void>
  /** True once `text` shows, waiting up to `ms`. */
  sees(text: string, ms?: number): Promise<boolean>
  send(prompt: string): Promise<void>
}

async function shot(client: Client, step: string): Promise<void> {
  await client.page.waitForTimeout(600)
  await client.page.evaluate(
    () => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))
  )
  await client.page.screenshot({ path: path.join(root, `${client.name}-${step}.png`) })
}

async function until(check: () => Promise<boolean>, ms: number): Promise<boolean> {
  for (const end = Date.now() + ms; Date.now() < end; await Bun.sleep(200)) {
    if (await check()) return true
  }
  return false
}

function reactClient(page: Page): Client {
  return {
    name: 'react',
    page,
    async click(text, exact = true) {
      const target = page
        .getByRole('button', { name: text, exact })
        .or(page.getByText(text, { exact }))
        .first()
      await target.click({ timeout: 20_000 })
    },
    sees: (text, ms = 45_000) => until(async () => (await page.getByText(text).count()) > 0, ms),
    async send(prompt) {
      const editor = page.locator('[contenteditable="true"]').first()
      await editor.click({ timeout: 20_000 })
      await page.keyboard.type(prompt, { delay: 15 })
      await page.waitForTimeout(300)
      // A phone's Return breaks the line; its Send button sends.
      if (PHONE)
        await page
          .getByRole('button', { name: /^(Send message|Start agent chat)$/ })
          .first()
          .click({ timeout: 20_000 })
      else await page.keyboard.press('Enter')
    }
  }
}

function runeClient(page: Page): Client {
  const at = async (text: string, exact: boolean) => {
    let point = null
    await until(async () => {
      point = await mirrorPoint(page, text, exact)
      return point !== null
    }, 20_000)
    if (!point) throw new Error(`the Rune mirror never showed "${text}"`)
    return point as { x: number; y: number }
  }
  return {
    name: 'rune',
    page,
    async click(text, exact = true) {
      const { x, y } = await at(text, exact)
      await page.mouse.click(x, y)
    },
    sees: (text, ms = 45_000) =>
      until(async () => (await mirrorPoint(page, text, false)) !== null, ms),
    async send(prompt) {
      const { x, y } = await at('Message', false)
      await page.mouse.click(x, y)
      await page.keyboard.type(prompt, { delay: 15 })
      await page.waitForTimeout(300)
      // A phone's Return breaks the line; its Send button sends.
      if (PHONE) await this.click('Send message')
      else await page.keyboard.press('Enter')
    }
  }
}

/** Runs the four answers in `client`; returns the agent replies it saw. */
async function run(client: Client): Promise<string[]> {
  const seen: string[] = []
  const expect = async (reply: string) => {
    if (await client.sees(reply)) seen.push(reply)
    else console.log(`${client.name}: never saw "${reply}"`)
  }
  await client.page.waitForTimeout(1500)
  await client.send(MATRIX_PROMPTS.permission)
  await client.sees('Approval needed')
  await shot(client, '1-permission')
  await client.click('Allow')
  await expect(EXPECTED[0])
  await shot(client, '2-allowed')

  await client.send(MATRIX_PROMPTS.permission)
  await client.sees('Approval needed')
  // The phone ignores taps in the first 400 ms of a prompt (APPROVAL_ACTIVATION_GUARD_MS).
  await client.page.waitForTimeout(600)
  await client.click('Reject')
  await expect(EXPECTED[1])
  await shot(client, '3-rejected')

  await client.send(MATRIX_PROMPTS.question)
  await client.sees('Which branch should the router tests run on?')
  await shot(client, '4-question')
  await client.click('feature/router', false)
  await shot(client, '5-question-picked')
  await client.click('Submit', false)
  await expect(EXPECTED[2])

  await client.send(MATRIX_PROMPTS.subagent)
  await client.sees('Explore the router')
  await shot(client, '6-subagents')
  await client.click('Explore the router')
  await client.sees('Subagent of this chat')
  await shot(client, '7-panel')
  await client.click('Stop')
  await client.sees('Stopped')
  await client.page.waitForTimeout(500)
  if (PHONE) await client.click('Back to chat')
  await expect(EXPECTED[3])
  await shot(client, '8-stopped')
  return seen
}

async function react(browser: Browser, world: World): Promise<string[]> {
  const dist = path.resolve(values.dist as string)
  await prepareDevice(world, 'lane3-react')
  const context = await browser.newContext({
    viewport: PHONE ? PHONE_VIEWPORT : VIEWPORT,
    isMobile: PHONE,
    hasTouch: PHONE,
    colorScheme: 'dark',
    reducedMotion: 'reduce',
    serviceWorkers: 'block'
  })
  await context.route(`${world.server.baseUrl}/**`, async (route) => {
    const url = new URL(route.request().url())
    const file = path.join(dist, url.pathname === '/' ? 'index.html' : url.pathname)
    const local = file.startsWith(dist) && (await Bun.file(file).exists())
    if (route.request().method() === 'GET' && local) await route.fulfill({ path: file })
    else await route.continue()
  })
  await context.addInitScript(
    ({ token }) => {
      localStorage.setItem('termul.webAuthToken', token)
      localStorage.setItem('termul.deviceId', 'lane3-react')
      localStorage.setItem('termul.renameNotice', 'dismissed')
    },
    { token: TOKEN }
  )
  const page = await context.newPage()
  await page.goto(`${world.server.baseUrl}/`)
  await page.getByText('What should we do in alpha?').first().waitFor({ timeout: 60_000 })
  try {
    return await run(reactClient(page))
  } finally {
    await context.close()
  }
}

async function rune(browser: Browser, world: World): Promise<string[]> {
  await prepareDevice(world, 'lane3-rune')
  const served = await serveRuneClient(runeWebDist('troupe'), world.server.baseUrl)
  try {
    const session = await openRuneWeb({
      browser,
      url: `${served.url}/`,
      ready: 'What should we do in alpha?',
      viewport: PHONE ? PHONE_VIEWPORT : VIEWPORT,
      touch: PHONE,
      storage: { 'termul.webAuthToken': TOKEN, 'termul.deviceId': 'lane3-rune' }
    })
    const lines: string[] = []
    session.page.on('console', (m) => lines.push(`${m.type()} ${m.text()}`))
    try {
      return await run(runeClient(session.page))
    } finally {
      await Bun.write(path.join(root, 'rune-console.log'), lines.join('\n'))
      await session.close()
    }
  } finally {
    await served.close()
  }
}

/** `pair-<step>.png`: React's frame on the left, Rune's on the right, for each step both saved. */
function writePairs(): void {
  for (const file of readdirSync(root).filter((f) => f.startsWith('react-'))) {
    const other = path.join(root, file.replace(/^react-/, 'rune-'))
    if (!existsSync(other)) continue
    const [a, b] = [readPng(path.join(root, file)), readPng(other)]
    const out = blankImage(a.width + 16 + b.width, Math.max(a.height, b.height), 255)
    blit(a, out, 0, 0, a.width, a.height, 0, 0)
    blit(b, out, 0, 0, b.width, b.height, a.width + 16, 0)
    writePng(path.join(root, file.replace(/^react-/, 'pair-')), out)
  }
}

const world = await createWorld({ port: Number(values.port), root, binary: values['server-bin'] })
let browser: Browser | undefined
try {
  browser = await chromium.launch({ args: runeWebArgs(REFERENCE_ARGS) })
  const results: Array<[string, string[]]> = []
  if (values.only !== 'rune') results.push(['react', await react(browser, world)])
  if (values.only !== 'react') results.push(['rune', await rune(browser, world)])
  let pass = true
  for (const [name, seen] of results) {
    const ok = EXPECTED.every((reply) => seen.includes(reply))
    pass &&= ok
    console.log(`${name}: ${ok ? 'PASS' : 'FAIL'} ${JSON.stringify(seen)}`)
  }
  process.exitCode = pass ? 0 : 1
} finally {
  try {
    writePairs()
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
