/**
 * The R9f first-run lane (plan R9, lane 9): from an empty state dir, finish the server setup
 * wizard, pass the access token gate and the first-run onboarding, and reach the dashboard,
 * in the React client and in the Rune web client, each against its own fresh server.
 *
 * A server in setup mode always binds port 8080 (`--port` would count as configuration), so
 * run the lane in a private network namespace, where 8080 is not the host's:
 *
 *   unshare -n sh -c 'ip link set lo up && PATH=/tmp/rune-r9f/fakebin:$PATH \
 *     bun tools/parity/lanes/first-run.ts --root /tmp/rune-r9f/first-run \
 *     --server-bin <troupe-server> --dist <dist-web> [--only react|rune] [--phone]'
 *
 * The wizard moves the server to 0.0.0.0:8195, which issues an access token. A fresh browser
 * then opens 8195 without it, enters a wrong token, then the one the server saved.
 * Writes `<client>-<step>.png` and `pair-<step>.png` under the root.
 */

import { readdirSync, readFileSync, statSync } from 'node:fs'
import { mkdir, rm } from 'node:fs/promises'
import path from 'node:path'
import { parseArgs } from 'node:util'
import { type Browser, type BrowserContext, chromium, type Page } from 'playwright'
import { REFERENCE_ARGS } from '../drivers/chromium'
import { PHONE_VIEWPORT, VIEWPORT } from '../drivers/react'
import { runeWebDist, serveRuneClient } from '../drivers/rune-apps'
import { mirrorPoint, runeWebArgs } from '../drivers/rune-web'
import { defaultServerBinary, REPO_DIR } from '../server'
import { connectWs } from '../ws'
import { until, writePairs } from './kit'

const { values } = parseArgs({
  args: Bun.argv.slice(2),
  options: {
    root: { type: 'string', default: '/tmp/troupe-parity/first-run' },
    'server-bin': { type: 'string' },
    dist: { type: 'string', default: path.join(REPO_DIR, 'dist-web') },
    only: { type: 'string' },
    phone: { type: 'boolean', default: false }
  }
})
const root = path.resolve(values.root as string)
const dist = path.resolve(values.dist as string)
const phone = values.phone as boolean
const SETUP_ORIGIN = 'http://127.0.0.1:8080'
const FINAL_ORIGIN = 'http://127.0.0.1:8195'

interface Run {
  page: Page
  name: 'react' | 'rune'
}

async function healthy(origin: string): Promise<boolean> {
  try {
    return (await fetch(`${origin}/health`)).ok
  } catch {
    return false
  }
}

/** A server with an empty state dir: it boots in setup mode and prints its setup link. */
async function setupServer(dir: string) {
  await rm(dir, { recursive: true, force: true })
  const home = path.join(dir, 'home')
  await mkdir(home, { recursive: true })
  const log = path.join(dir, 'server.log')
  const child = Bun.spawn([(values['server-bin'] as string | undefined) ?? defaultServerBinary()], {
    env: {
      ...process.env,
      HOME: home,
      XDG_CONFIG_HOME: path.join(home, '.config'),
      XDG_DATA_HOME: path.join(home, '.local/share'),
      XDG_STATE_HOME: path.join(home, '.local/state'),
      XDG_CACHE_HOME: path.join(home, '.cache'),
      TROUPE_WEB_AUTH_TOKEN: '',
      RUST_LOG: 'termul_manager_lib=info'
    },
    stdout: Bun.file(log),
    stderr: Bun.file(path.join(dir, 'server.err'))
  })
  if (!(await until(() => healthy(SETUP_ORIGIN), 30_000)))
    throw new Error('setup server never answered')
  let token = ''
  await until(async () => {
    const text = [log, path.join(dir, 'server.err')].map((f) => readFileSync(f, 'utf8')).join('\n')
    token = /[?&]setup=([A-Za-z0-9_-]+)/.exec(text)?.[1] ?? ''
    return token !== ''
  }, 10_000)
  if (!token) throw new Error('no setup link in the server log')
  const health = (await (await fetch(`${SETUP_ORIGIN}/health`)).json()) as {
    setupRequired?: boolean
  }
  console.log(`setup server pid ${child.pid}, setupRequired=${health.setupRequired}`)
  return { child, home, token }
}

/** The access token the configured server saved. */
function savedToken(home: string): string {
  const found: string[] = []
  const walk = (dir: string) => {
    for (const name of readdirSync(dir)) {
      const full = path.join(dir, name)
      if (statSync(full).isDirectory()) walk(full)
      else if (name === 'web-auth-token') found.push(full)
    }
  }
  walk(home)
  if (found.length === 0) throw new Error('the server saved no web-auth-token')
  return readFileSync(found[0], 'utf8').trim()
}

async function hostState(token: string): Promise<string> {
  const ws = await connectWs({
    baseUrl: FINAL_ORIGIN,
    token,
    deviceId: 'first-run-check',
    clientInstanceId: 'first-run-check'
  })
  try {
    const reply = await ws.request<{ value?: { data?: { onboardingCompletedAt?: string } } }>(
      'store_read',
      { key: 'shared/lww/settings/app' }
    )
    const health = (await (await fetch(`${FINAL_ORIGIN}/health`)).json()) as {
      setupRequired?: boolean
    }
    return JSON.stringify({
      setupRequired: health.setupRequired ?? false,
      onboarded: Boolean(reply.value?.data?.onboardingCompletedAt)
    })
  } finally {
    ws.close()
  }
}

/** A fresh browser; the React client's files come from `--dist`, as the other lanes serve it. */
async function context(browser: Browser, name: 'react' | 'rune'): Promise<BrowserContext> {
  const made = await browser.newContext({
    viewport: phone ? PHONE_VIEWPORT : VIEWPORT,
    isMobile: phone,
    hasTouch: phone,
    colorScheme: 'dark',
    reducedMotion: 'reduce',
    serviceWorkers: 'block'
  })
  if (name === 'react') {
    await made.route(/^http:\/\/127\.0\.0\.1:(8080|8195)\//, async (route) => {
      const url = new URL(route.request().url())
      const file = path.join(dist, url.pathname === '/' ? 'index.html' : url.pathname)
      const local = file.startsWith(dist) && (await Bun.file(file).exists())
      if (route.request().method() === 'GET' && local) await route.fulfill({ path: file })
      else await route.continue()
    })
  }
  return made
}

async function served(origin: string, name: 'react' | 'rune') {
  if (name === 'react') return { url: origin, close: async () => {} }
  return serveRuneClient(runeWebDist('troupe'), origin)
}

async function sees(run: Run, text: string, ms = 20_000): Promise<boolean> {
  if (run.name === 'react')
    return until(async () => (await run.page.getByText(text).count()) > 0, ms)
  return until(async () => (await mirrorPoint(run.page, text, false)) !== null, ms)
}

async function click(run: Run, text: string, nth = -1): Promise<void> {
  if (run.name === 'react') {
    const all = run.page.getByRole('button', { name: text, exact: true })
    const target = (await all.count()) > 0 ? all : run.page.getByText(text, { exact: true })
    await (nth === -1 ? target.last() : target.nth(nth)).click({ timeout: 20_000 })
    return
  }
  let point: { x: number; y: number } | null = null
  await until(async () => {
    point = await mirrorPoint(run.page, text, true)
    return point !== null
  }, 20_000)
  if (!point) throw new Error(`the Rune mirror never showed "${text}"`)
  const { x, y } = point as { x: number; y: number }
  await run.page.mouse.click(x, y)
}

/** Replaces what the field named `label` holds with `text`. */
async function type(run: Run, label: string, text: string): Promise<void> {
  if (run.name === 'react') {
    await run.page.getByLabel(label, { exact: true }).first().fill(text, { timeout: 20_000 })
    return
  }
  await click(run, label)
  await run.page.keyboard.press('Control+a')
  await run.page.keyboard.press('Backspace')
  await run.page.keyboard.type(text, { delay: 15 })
}

async function lane(browser: Browser, name: 'react' | 'rune') {
  const dir = path.join(root, name)
  const server = await setupServer(dir)
  const seen: string[] = []
  let index = 0
  let page: Page | null = null
  const step = async (label: string, check: () => Promise<boolean>) => {
    const ok = await check().catch(() => false)
    if (ok) seen.push(label)
    else console.log(`${name}: ${label} failed its check`)
    index += 1
    if (page) {
      await page.waitForTimeout(600)
      await page.screenshot({
        path: path.join(root, `${name}-${String(index).padStart(2, '0')}-${label}.png`)
      })
    }
  }
  const act = async (what: string, fn: () => Promise<void>) => {
    try {
      await fn()
    } catch (error) {
      console.log(`${name}: ${what} failed: ${error}`)
    }
  }
  const first = await served(SETUP_ORIGIN, name)
  const one = await context(browser, name)
  try {
    page = await one.newPage()
    const run: Run = { page, name }
    await page.goto(`${first.url}/?setup=${server.token}`)
    await step('wizard', () => sees(run, 'Set up Troupe', 45_000))
    await act('reachability', () => click(run, 'Only this computer'))
    await page.waitForTimeout(400)
    await act('all devices', () => click(run, 'Other devices on the network'))
    await page.waitForTimeout(400)
    await act('port', () => type(run, 'Port', '8195'))
    await step('wizard-filled', () => sees(run, 'Let other devices make changes', 5_000))
    await act('start', () => click(run, 'Start Troupe'))
    await step('restarting', () => sees(run, 'Restarting Troupe', 10_000))
    await step('landed', () =>
      until(async () => page?.url().startsWith(FINAL_ORIGIN) ?? false, 30_000)
    )
  } finally {
    await one.close()
    await first.close()
  }
  const token = savedToken(server.home)
  const second = await served(FINAL_ORIGIN, name)
  const two = await context(browser, name)
  try {
    page = await two.newPage()
    const run: Run = { page, name }
    await page.goto(`${second.url}/`)
    await step('token-gate', () => sees(run, 'Server access token required', 45_000))
    await act('wrong token', () => type(run, 'Access token', 'not-the-token'))
    await act('continue', () => click(run, 'Continue'))
    await step('token-invalid', () => sees(run, 'Invalid token — check and try again.', 10_000))
    await act('right token', () => type(run, 'Access token', token))
    await act('continue', () => click(run, 'Continue'))
    await step('welcome', () => sees(run, 'Welcome to Troupe', 45_000))
    await act('get started', () => click(run, 'Get started'))
    await step('agents', () => sees(run, 'Check your agents', 10_000))
    await act('set up later', () => click(run, 'Set up later'))
    await step('project', () => sees(run, 'Where should agents work?', 10_000))
    await act('skip', () => click(run, 'Skip for now'))
    await step('dashboard', async () =>
      until(async () => JSON.parse(await hostState(token)).onboarded === true, 15_000)
    )
    return { seen, state: await hostState(token) }
  } finally {
    await two.close()
    await second.close()
    server.child.kill('SIGTERM')
    await Promise.race([server.child.exited, Bun.sleep(3000)])
    if (server.child.exitCode === null) server.child.kill('SIGKILL')
  }
}

await mkdir(root, { recursive: true })
const browser: Browser = await chromium.launch({ args: runeWebArgs(REFERENCE_ARGS) })
const results: Record<string, { seen: string[]; state: string }> = {}
try {
  for (const name of ['react', 'rune'] as const) {
    if (values.only && values.only !== name) continue
    results[name] = await lane(browser, name)
    console.log(`${name}: ${JSON.stringify(results[name])}`)
  }
} finally {
  await browser.close()
}
writePairs(root)
const want = JSON.stringify({ setupRequired: false, onboarded: true })
const failed = Object.entries(results).filter(([, r]) => r.state !== want || r.seen.length < 10)
console.log(
  failed.length === 0
    ? 'PASS: every client reached the dashboard'
    : `FAIL: ${failed.map(([n]) => n)}`
)
process.exit(failed.length === 0 ? 0 : 1)
