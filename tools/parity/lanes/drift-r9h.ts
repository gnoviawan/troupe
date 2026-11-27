/**
 * The R9h drift lane: the settings pages in the 1080 px split layout (#2169), the advanced
 * disclosure open, the MCP and About pages, and a terminal with a passing and a failing
 * command, which shows a success and a failure dot (#1961). The phone run shows Preferences
 * down to the colour swatches (#1968) and the phone terminal's dots.
 *
 * The dots need a `troupe-server` from #1961 on (0.6.0 or later), whose bash prints the finish
 * mark.
 *
 *   bun tools/parity/lanes/drift-r9h.ts --port 8191 --root /tmp/rune-r9h/lane \
 *     --server-bin <troupe-server> --dist <dist-web> [--only react|rune] [--phone]
 *
 * Writes `<client>-<step>.png` and `pair-<step>.png` under the root.
 */

import path from 'node:path'
import { parseArgs } from 'node:util'
import { type Browser, chromium } from 'playwright'
import { REFERENCE_ARGS } from '../drivers/chromium'
import { runeWebArgs } from '../drivers/rune-web'
import { REPO_DIR } from '../server'
import { createWorld, ensureProject } from '../world'
import { type Client, inReact, inRune, type LaneOptions, shot, until, writePairs } from './kit'

const { values } = parseArgs({
  args: Bun.argv.slice(2),
  options: {
    port: { type: 'string', default: '8191' },
    root: { type: 'string', default: '/tmp/troupe-parity/drift-r9h' },
    'server-bin': { type: 'string' },
    dist: { type: 'string', default: path.join(REPO_DIR, 'dist-web') },
    only: { type: 'string' },
    phone: { type: 'boolean', default: false }
  }
})
const options: LaneOptions = {
  root: path.resolve(values.root as string),
  phone: values.phone as boolean,
  dist: path.resolve(values.dist as string),
  device: 'drift-r9h'
}

/** Presses the last Rune mirror button named `name`. */
async function runeButton(client: Client, name: string): Promise<void> {
  const pressed = await client.page.evaluate((want) => {
    const root = document.querySelector('[data-rune-a11y]')
    const named = [...(root?.querySelectorAll('button, [role="button"]') ?? [])].filter(
      (el) => (el.getAttribute('aria-label') ?? el.textContent ?? '').trim() === want
    )
    const last = named.at(-1) as HTMLElement | undefined
    last?.click()
    return Boolean(last)
  }, name)
  if (!pressed) throw new Error(`no Rune button named "${name}"`)
}

/** Both a success dot and a failure dot show: xterm decorations in React, mirror images in Rune. */
async function hasDots(client: Client): Promise<boolean> {
  return client.page.evaluate((react) => {
    if (react) {
      return (
        document.querySelector('.termul-command-status--success') !== null &&
        document.querySelector('.termul-command-status--failure') !== null
      )
    }
    const labels = [
      ...(document.querySelector('[data-rune-a11y]')?.querySelectorAll('[aria-label]') ?? [])
    ].map((el) => el.getAttribute('aria-label') ?? '')
    return (
      labels.includes('Command succeeded (exit code 0)') &&
      labels.includes('Command failed (exit code 2)')
    )
  }, client.name === 'react')
}

async function run(client: Client): Promise<string[]> {
  const seen: string[] = []
  let index = 0
  const step = async (name: string, check: () => Promise<boolean>) => {
    const ok = await check().catch(() => false)
    if (ok) seen.push(name)
    else console.log(`${client.name}: ${name} failed its check`)
    index += 1
    await shot(client, options.root, `${String(index).padStart(2, '0')}-${name}`)
  }
  const act = async (what: string, fn: () => Promise<void>) => {
    try {
      await fn()
    } catch (error) {
      console.log(`${client.name}: ${what} failed: ${error}`)
    }
  }
  const settle = (ms = 800) => client.page.waitForTimeout(ms)
  const react = client.name === 'react'
  const button = (name: string) =>
    react
      ? client.page.getByRole('button', { name, exact: true }).last().click({ timeout: 20_000 })
      : runeButton(client, name)
  const nav = (name: string) =>
    react
      ? client.page
          .locator(`nav[aria-label="Settings pages"] >> role=button[name="${name}"s]`)
          .click({ timeout: 20_000 })
      : runeButton(client, name)

  await client.sees('What should we do in alpha?', 30_000)
  if (options.phone) await act('open the menu', () => client.click('Open menu', false))
  await settle(400)
  await act('open settings', () =>
    options.phone && !react ? client.click('Settings') : button('Settings')
  )
  await settle()
  await step('preferences', () => client.sees('Theme, interface zoom, and window chrome.', 10_000))
  if (options.phone) {
    await client.page.mouse.move(195, 500)
    for (let i = 0; i < 12; i++) await client.page.mouse.wheel(0, 400)
    await settle()
    await step('preferences-swatches', () => client.sees('Default color', 5_000))
  } else {
    await act('terminal page', () => nav('Terminal'))
    await settle()
    await step('terminal-page', () =>
      client.sees('Default shell and accessibility behavior.', 10_000)
    )
    await act('open advanced', () =>
      react
        ? client.page
            .getByRole('button', { name: /^Advanced/ })
            .first()
            .click({ timeout: 20_000 })
        : runeButton(client, 'Advanced')
    )
    await settle()
    await step('terminal-advanced', () => client.sees('Scrollback', 5_000))
    await act('mcp page', () => nav('MCP servers'))
    await settle()
    await step('mcp', () => client.sees('No MCP servers yet', 10_000))
    await act('about page', () => nav('About'))
    await settle()
    await step('about', () =>
      client.sees('Restore all preferences and shortcuts to defaults.', 10_000)
    )
    await client.page.keyboard.press('Escape')
    await settle()
    await act('new terminal', () => client.click('New terminal'))
    await settle(4000)
    if (react) await client.page.locator('.xterm-screen').first().click()
    else {
      // The first press closes a menu the New terminal press may have left open.
      await client.page.mouse.click(560, 300)
      await settle(400)
      await client.page.mouse.click(560, 300)
    }
    for (const command of ['echo passes', 'ls /nope']) {
      await client.page.keyboard.type(command, { delay: 5 })
      await client.page.keyboard.press('Enter')
      await settle(2000)
    }
    await step('terminal-dots', () => until(() => hasDots(client), 5_000))
  }
  return seen
}

const browser: Browser = await chromium.launch({ args: runeWebArgs(REFERENCE_ARGS) })
const world = await createWorld({
  port: Number(values.port),
  root: path.join(options.root, 'world'),
  binary: values['server-bin'] as string | undefined
})
const results: Record<string, string[]> = {}
try {
  for (const name of ['react', 'rune'] as const) {
    if (values.only && values.only !== name) continue
    await ensureProject(world, 'alpha')
    const drive = name === 'react' ? inReact : inRune
    results[name] = await drive(browser, world, options, (client) => run(client))
    console.log(`${name}: ${JSON.stringify(results[name])}`)
  }
} finally {
  await browser.close()
  await world.server.stop()
}
writePairs(options.root)
const want = options.phone ? 2 : 6
const counts = Object.values(results).map((r) => r.length)
const ok = counts.length > 0 && counts.every((n) => n === want)
console.log(
  ok ? `PASS: both clients passed ${counts[0]} steps` : `FAIL: ${JSON.stringify(results)}`
)
process.exit(ok ? 0 : 1)
