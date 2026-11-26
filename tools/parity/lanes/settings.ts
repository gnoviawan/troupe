/**
 * The R9b settings lane: open the command palette and run App Preferences from it, change the
 * terminal font size and the idle-chat window, open the shortcuts page and the reset confirm,
 * apply a theme from the palette's Change Color Theme, open the shortcut menu, change the
 * interface zoom, then reload. Each client starts from the same seeded `settings/app`, and the lane
 * fails unless both leave the same values on the host and show them again after the reload.
 *
 *   bun tools/parity/lanes/settings.ts --port 8194 --root /tmp/rune-r9b/lane \
 *     --server-bin <troupe-server> --dist <dist-web> [--only react|rune]
 *
 * Writes `<client>-<step>.png` and `pair-<step>.png` under the root.
 */

import path from 'node:path'
import { parseArgs } from 'node:util'
import { type Browser, chromium } from 'playwright'
import { REFERENCE_ARGS } from '../drivers/chromium'
import { runeWebArgs } from '../drivers/rune-web'
import { REPO_DIR } from '../server'
import { createWorld, TOKEN, type World } from '../world'
import { type Client, inReact, inRune, type LaneOptions, shot, writePairs } from './kit'

const { values } = parseArgs({
  args: Bun.argv.slice(2),
  options: {
    port: { type: 'string', default: '8194' },
    root: { type: 'string', default: '/tmp/troupe-parity/settings' },
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
  device: 'settings'
}
const KEY = 'shared/lww/settings/app'
const WANT = {
  colorTheme: 'nord',
  appearanceMode: 'dark',
  uiZoomLevel: 1.1,
  terminalFontSize: 15,
  agentIdleWindowSecs: 3600
}

async function seedSettings(world: World): Promise<void> {
  const current = await world.ws.request<{ revision?: number }>('store_read', { key: KEY })
  await world.ws.request('store_write', {
    key: KEY,
    value: { _version: 1, data: { colorTheme: 'termul', appearanceMode: 'dark' } },
    baseRevision: current.revision ?? 0
  })
  // The idle window lives on the host too: put it back to the default 30 minutes.
  const reply = await fetch(`${world.server.baseUrl}/host/agent-idle-window`, {
    method: 'POST',
    headers: { Authorization: `Bearer ${TOKEN}`, 'content-type': 'application/json' },
    body: JSON.stringify({ secs: 1800 })
  })
  if (!reply.ok) throw new Error(`resetting the idle window: HTTP ${reply.status}`)
}

async function stored(world: World): Promise<Record<string, unknown>> {
  const read = await world.ws.request<{ value?: { data?: Record<string, unknown> } }>(
    'store_read',
    { key: KEY }
  )
  const data = read.value?.data ?? {}
  return Object.fromEntries(Object.keys(WANT).map((key) => [key, data[key]]))
}

/**
 * Presses the Rune mirror's button named `name` (the rail page, not the group heading above it)
 * through the mirror itself, as a screen reader does. At a zoom other than 100% the mirror
 * keeps its unzoomed boxes, so a pointer press at a box's centre would miss.
 */
async function runeButton(client: Client, name: string): Promise<void> {
  const pressed = await client.page.evaluate((want) => {
    const root = document.querySelector('[data-rune-a11y]')
    for (const el of root?.querySelectorAll('button') ?? []) {
      const named = (el.getAttribute('aria-label') ?? el.textContent ?? '').trim()
      if (named === want) {
        ;(el as HTMLButtonElement).click()
        return true
      }
    }
    return false
  }, name)
  if (!pressed) throw new Error(`no Rune button named "${name}"`)
}

async function run(client: Client, world: World): Promise<{ seen: string[]; state: string }> {
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
  // A rail group heading shows the same word as its first page ("Terminal", "Agents").
  const nav = (name: string) =>
    client.name === 'react'
      ? client.page
          .locator(`nav[aria-label="Settings pages"] >> role=button[name="${name}"s]`)
          .click({ timeout: 20_000 })
      : runeButton(client, name)
  const palette = async (query: string) => {
    await act('open the palette', () => client.click('Search'))
    await settle(400)
    await client.page.keyboard.type(query, { delay: 20 })
    await settle(300)
  }

  await client.sees('What should we do in alpha?', 30_000)
  await act('open the palette', () => client.click('Search'))
  await step('palette', () => client.sees('Open a new shell in the active pane', 10_000))
  await client.page.keyboard.type('App Pref', { delay: 20 })
  await settle(300)
  await step('palette-search', () => client.sees('Open global application preferences', 5_000))
  // cmdk ranks Open Canvas first for this query (it does not reorder groups), so click the row.
  // Click the row by its description: the label also names the row's pin button.
  await act('run App Preferences', () => client.click('Open global application preferences', false))
  await step('preferences', () => client.sees('Interface zoom', 10_000))
  await act('terminal page', () => nav('Terminal'))
  await act('font size', () => client.click('Increase font size'))
  await step('terminal', () => client.sees('15 px', 5_000))
  await act('agents page', () => nav('Agents'))
  await settle()
  await step('agents', () => client.sees('Close idle chats after', 5_000))
  // The Rune trigger is named by its row, as React's is; its value is not in the mirror.
  await act('idle window select', () =>
    client.name === 'react'
      ? client.click('30 minutes')
      : runeButton(client, 'Close idle chats after')
  )
  await settle(400)
  await act('idle window 1 hour', () => client.click('1 hour'))
  await settle(800)
  await step('agents-picked', async () => (await stored(world)).agentIdleWindowSecs === 3600)
  await act('shortcuts page', () => nav('Keyboard shortcuts'))
  await step('shortcuts', () => client.sees('Reset all', 5_000))
  await act('about page', () => nav('About'))
  await act('reset', () => client.click('Reset settings'))
  await step('reset-confirm', () => client.sees('Reset all settings?', 5_000))
  await act('cancel reset', () => client.click('Cancel'))
  await settle(400)
  await client.page.keyboard.press('Escape')
  await settle(400)
  await palette('Color Theme')
  await act('run Change Color Theme', () =>
    client.click('Preview and apply a UI color theme', false)
  )
  await step('theme-picker', () => client.sees('Catppuccin Light', 5_000))
  await client.page.keyboard.type('nord', { delay: 20 })
  await settle(300)
  await client.page.keyboard.press('Enter')
  await settle()
  await step('themed', async () => (await stored(world)).colorTheme === 'nord')
  // The theme picker hands focus back to its rail button, whose tooltip covers the next one.
  await client.page.mouse.move(640, 400)
  await client.page.keyboard.press('Escape')
  await settle(400)
  await act('shortcut menu', () => client.click('Open keyboard shortcuts menu'))
  await step('shortcut-menu', () => client.sees('Shortcut Menu', 5_000))
  await client.page.keyboard.press('Escape')
  await settle(400)
  // Zoom last: the Rune mirror keeps its unzoomed boxes, so a press after a zoom misses.
  await act('open settings to zoom', () => client.click('Settings'))
  await act('zoom in', () => client.click('Increase interface zoom'))
  await step('zoomed', () => client.sees('110%', 5_000))
  await client.page.keyboard.press('Escape')
  await settle(1500)
  const before = JSON.stringify(await stored(world))
  await client.page.reload()
  await client.sees('What should we do in alpha?', 30_000)
  await act('open settings', () =>
    client.name === 'react' ? client.click('Settings') : runeButton(client, 'Settings')
  )
  await step(
    'reloaded',
    async () => (await client.sees('110%', 10_000)) && before === JSON.stringify(WANT)
  )
  return { seen, state: JSON.stringify(await stored(world)) }
}

const browser: Browser = await chromium.launch({ args: runeWebArgs(REFERENCE_ARGS) })
const world = await createWorld({
  port: Number(values.port),
  root: path.join(options.root, 'world'),
  binary: values['server-bin'] as string | undefined
})
const results: Record<string, { seen: string[]; state: string }> = {}
try {
  for (const name of ['react', 'rune'] as const) {
    if (values.only && values.only !== name) continue
    await seedSettings(world)
    const drive = name === 'react' ? inReact : inRune
    results[name] = await drive(browser, world, options, (client) => run(client, world))
    console.log(`${name}: ${JSON.stringify(results[name])}`)
  }
} finally {
  await browser.close()
  await world.server.stop()
}
writePairs(options.root)
const want = JSON.stringify(WANT)
const failed = Object.entries(results).filter(([, r]) => r.state !== want || r.seen.length < 13)
console.log(
  failed.length === 0
    ? 'PASS: every client left the same settings'
    : `FAIL: ${failed.map(([n]) => n)}`
)
process.exit(failed.length === 0 ? 0 : 1)
