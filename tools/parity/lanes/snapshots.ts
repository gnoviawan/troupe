/**
 * The R9f snapshots lane: open a terminal in alpha, open the snapshots page, create two
 * snapshots, restore the older one, then delete the newer one. On the phone the page opens
 * from the menu drawer and the older snapshot is renamed first. Each client starts from the
 * same world, and the lane fails unless both leave the host with the same list: the restored
 * snapshot alone, with its one terminal.
 *
 *   bun tools/parity/lanes/snapshots.ts --port 8195 --root /tmp/rune-r9f/snapshots \
 *     --server-bin <troupe-server> --dist <dist-web> [--only react|rune] [--phone]
 *
 * Writes `<client>-<step>.png` and `pair-<step>.png` under the root.
 */

import path from 'node:path'
import { parseArgs } from 'node:util'
import { type Browser, chromium } from 'playwright'
import { scopedKey } from '../../../src/shared/types/persistence.types'
import { REFERENCE_ARGS } from '../drivers/chromium'
import { runeWebArgs } from '../drivers/rune-web'
import { REPO_DIR } from '../server'
import { createWorld, PROJECTS, TOKEN, type World } from '../world'
import { connectWs } from '../ws'
import { type Client, inReact, inRune, type LaneOptions, shot, until, writePairs } from './kit'

const { values } = parseArgs({
  args: Bun.argv.slice(2),
  options: {
    port: { type: 'string', default: '8195' },
    root: { type: 'string', default: '/tmp/troupe-parity/snapshots' },
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
  device: values.phone ? 'snapshots-phone' : 'snapshots'
}
const RENAMED = 'Lane snapshot renamed'
const FIRST = 'Lane snapshot'
const SECOND = 'Second snapshot'

interface Saved {
  name: string
  description?: string
  terminals: { cwd?: string }[]
}

/** The device's saved list, read as that device (a device key belongs to its device). */
async function saved(world: World, device: string): Promise<Saved[]> {
  const ws = await connectWs({
    baseUrl: world.server.baseUrl,
    token: TOKEN,
    deviceId: device,
    clientInstanceId: `${device}-lane-read`
  })
  try {
    const reply = await ws.request<{ value?: { data?: { snapshots?: Saved[] } } }>('store_read', {
      key: scopedKey(`snapshots/${PROJECTS.alpha.id}`, device)
    })
    return reply.value?.data?.snapshots ?? []
  } finally {
    ws.close()
  }
}

async function hostState(world: World, device: string): Promise<string> {
  const list = await saved(world, device)
  return JSON.stringify(
    list.map((s) => ({ name: s.name, description: s.description, panes: s.terminals.length }))
  )
}

/** Presses the `nth` Rune mirror button named `name` (-1: the last, a dialog's). */
async function runeButton(client: Client, name: string, nth = -1): Promise<void> {
  const pressed = await client.page.evaluate(
    ({ want, nth }) => {
      const root = document.querySelector('[data-rune-a11y]')
      const named = [...(root?.querySelectorAll('button, [role="button"]') ?? [])].filter(
        (el) => (el.getAttribute('aria-label') ?? el.textContent ?? '').trim() === want
      )
      const el = named.at(nth) as HTMLElement | undefined
      if (!el) return false
      const r = el.getBoundingClientRect()
      return { x: r.x + r.width / 2, y: r.y + r.height / 2 }
    },
    { want: name, nth }
  )
  if (!pressed) throw new Error(`no Rune button named "${name}"`)
  await client.page.mouse.move(pressed.x, pressed.y)
  await client.page.mouse.click(pressed.x, pressed.y)
}

async function run(client: Client, world: World): Promise<{ seen: string[]; state: string }> {
  const device = `${options.device}-${client.name}`
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
  const button = (name: string, nth = -1) =>
    react
      ? (nth === -1
          ? client.page.getByRole('button', { name, exact: true }).last()
          : client.page.getByRole('button', { name, exact: true }).nth(nth)
        ).click({ timeout: 20_000, force: true })
      : runeButton(client, name, nth)
  const count = async () => (await saved(world, device)).length
  const openPage = async () => {
    if (options.phone) {
      await act('open the menu', () => client.click('Open menu', false))
      await settle(400)
      await act('open snapshots', () => button('Snapshots'))
    } else {
      await client.page.goto(`${new URL(client.page.url()).origin}/#/snapshots`)
    }
    await settle(1500)
  }

  await client.sees('What should we do in alpha?', 30_000)
  await act('new tab menu', () => button('New tab', 0))
  await settle(400)
  await act('new terminal', () => client.click('New terminal'))
  await settle(2500)
  await openPage()
  await step('empty', () => client.sees('No snapshots yet', 15_000))
  await act('create first', () => button('Create First Snapshot'))
  await settle(400)
  await act('name', () => client.fill('Snapshot Name', FIRST))
  await act('description', () => client.fill('Description (optional)', 'one terminal'))
  await step('create-dialog', () => client.sees('Create Snapshot', 5_000))
  await act('create', () => button('Create'))
  await step('created', () => until(async () => (await count()) === 1, 10_000))
  await act('open create', () => button(options.phone ? 'New' : 'Create New Snapshot'))
  await settle(400)
  await act('name 2', () => client.fill('Snapshot Name', SECOND))
  await act('create 2', () => button('Create'))
  await step('two', () => until(async () => (await count()) === 2, 10_000))
  let first = FIRST
  if (options.phone) {
    await act('rename', () => button('Rename snapshot', 1))
    await settle(400)
    await act('rename text', async () => {
      await client.page.keyboard.press('Control+a')
      await client.page.keyboard.type(RENAMED, { delay: 15 })
      await client.page.keyboard.press('Enter')
    })
    first = RENAMED
    await step('renamed', () =>
      until(async () => (await saved(world, device)).some((s) => s.name === RENAMED), 10_000)
    )
  }
  // Newest first: the older snapshot's Restore is the second card's.
  await act('restore older', () => button('Restore', 1))
  await step('restore-dialog', () => client.sees(`"${first}"`, 5_000))
  await act('confirm restore', () => button('Restore'))
  await step('restored', () => client.sees('What should we do in alpha?', 1).then((v) => !v))
  await openPage()
  await act('delete newer', () => button(options.phone ? 'Delete snapshot' : 'Delete', 0))
  await step('delete-dialog', () => client.sees(`"${SECOND}"`, 5_000))
  await act('confirm delete', () => button('Delete'))
  await step('deleted', () =>
    until(async () => {
      const list = await saved(world, device)
      return list.length === 1 && list[0].name === first
    }, 10_000)
  )
  return { seen, state: await hostState(world, device) }
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
    const drive = name === 'react' ? inReact : inRune
    results[name] = await drive(browser, world, options, (client) => run(client, world))
    console.log(`${name}: ${JSON.stringify(results[name])}`)
  }
} finally {
  await browser.close()
  await world.server.stop()
}
writePairs(options.root)
const steps = options.phone ? 10 : 9
const want = JSON.stringify([
  { name: options.phone ? RENAMED : FIRST, description: 'one terminal', panes: 1 }
])
const failed = Object.entries(results).filter(([, r]) => r.state !== want || r.seen.length < steps)
console.log(
  failed.length === 0
    ? 'PASS: every client left the same snapshots'
    : `FAIL: ${failed.map(([n]) => n)}`
)
process.exit(failed.length === 0 ? 0 : 1)
