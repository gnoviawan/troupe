/**
 * The R9g settings lane: install a server from the MCP catalog through its consent review,
 * edit it as JSON, raise the sub-agent cap, and open Add account on the Accounts page. Each
 * client starts from the same world, and the lane fails unless both leave the host with the
 * same installed entry (its id aside), the same sub-agent cap and the same account list.
 *
 *   bun tools/parity/lanes/settings-r9g.ts --port 8196 --root /tmp/rune-r9g/lane \
 *     --server-bin <troupe-server> --dist <dist-web> [--only react|rune] [--phone]
 *
 * Writes `<client>-<step>.png` and `pair-<step>.png` under the root.
 */

import path from 'node:path'
import { parseArgs } from 'node:util'
import { type Browser, chromium } from 'playwright'
import { REFERENCE_ARGS } from '../drivers/chromium'
import { runeWebArgs } from '../drivers/rune-web'
import { REPO_DIR, serverFetch } from '../server'
import { createWorld, ensureProject, type World } from '../world'
import { type Client, inReact, inRune, type LaneOptions, shot, writePairs } from './kit'

const { values } = parseArgs({
  args: Bun.argv.slice(2),
  options: {
    port: { type: 'string', default: '8196' },
    root: { type: 'string', default: '/tmp/troupe-parity/settings-r9g' },
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
  device: 'settings-r9g'
}
const MCP_KEY = 'shared/lww/acp/mcp-servers'

interface Row {
  name: string
  title: string | null
  state: string
  version: string
  defaultOption: unknown
}

/** Waits for the catalog sync to settle, so both clients search the same catalog. */
async function catalogSettled(world: World): Promise<void> {
  for (let i = 0; i < 120; i++) {
    const body = (await (await serverFetch(world.server, '/mcp-catalog/status')).json()) as {
      data?: { sample?: boolean; sync?: { state?: string } }
    }
    const status = body.data
    if (status && !status.sample && status.sync?.state === 'idle') return
    await Bun.sleep(2000)
  }
  throw new Error('the catalog never finished syncing')
}

/** A catalog row whose install form asks for nothing required, so both clients fill the same. */
async function pickRow(world: World): Promise<Row> {
  const page = (await (await serverFetch(world.server, '/mcp-catalog/search?limit=50')).json()) as {
    data?: { items: Row[] }
    items?: Row[]
  }
  const items = page.data?.items ?? page.items ?? []
  for (const row of items.filter((r) => r.state === 'installable' && r.defaultOption)) {
    const reply = (await (
      await serverFetch(world.server, '/mcp-catalog/install/form', {
        method: 'POST',
        body: JSON.stringify({
          source: { kind: 'catalog', name: row.name, version: row.version },
          choice: row.defaultOption
        })
      })
    ).json()) as { data?: { outcome: string; form?: { fields: { isRequired: boolean }[] } } }
    const form = reply.data
    if (form?.outcome === 'form' && form.form?.fields.every((f) => !f.isRequired)) return row
  }
  throw new Error('no installable catalog row without required fields')
}

async function hostState(world: World): Promise<string> {
  const mcp = await world.ws.request<{ value?: { data?: Record<string, unknown>[] } }>(
    'store_read',
    {
      key: MCP_KEY
    }
  )
  const sub = await world.ws.request<{ maxRunning: number }>('subagent_settings_get', {})
  const accounts = await world.ws.request<{ accounts: { name: string }[] }>(
    'list_claude_accounts',
    {}
  )
  return JSON.stringify({
    mcp: (mcp.value?.data ?? []).map(({ id: _id, ...rest }) => rest),
    maxRunning: sub.maxRunning,
    accounts: accounts.accounts.map((a) => a.name)
  })
}

async function reset(world: World): Promise<void> {
  await ensureProject(world, 'alpha')
  const current = await world.ws.request<{ revision?: number }>('store_read', { key: MCP_KEY })
  await world.ws.request('store_write', {
    key: MCP_KEY,
    value: { _version: 1, data: [] },
    baseRevision: current.revision ?? 0
  })
  await world.ws.request('subagent_settings_set', { maxRunning: 3 })
}

/** Presses the Rune mirror button named `name`, the last one, or the one inside `group`. */
async function runeButton(client: Client, name: string, group?: string): Promise<void> {
  const pressed = await client.page.evaluate(
    ([want, inside]) => {
      const root = document.querySelector('[data-rune-a11y]')
      const scope = inside
        ? [...(root?.querySelectorAll('[role="group"]') ?? [])].find(
            (el) => (el.getAttribute('aria-label') ?? '').trim() === inside
          )
        : root
      const named = [
        ...(scope?.querySelectorAll('button, [role="button"], [role="switch"]') ?? [])
      ].filter((el) => (el.getAttribute('aria-label') ?? el.textContent ?? '').trim() === want)
      const last = named.at(-1) as HTMLElement | undefined
      last?.click()
      return Boolean(last)
    },
    [name, group ?? ''] as const
  )
  if (!pressed) throw new Error(`no Rune button named "${name}"`)
}

/** Types into the catalog search, which sits just under the "From the catalog" heading. */
async function runeSearch(client: Client, query: string): Promise<void> {
  const box = await client.page.evaluate(() => {
    const root = document.querySelector('[data-rune-a11y]')
    for (const el of root?.querySelectorAll('*') ?? []) {
      if ((el.textContent ?? '').trim() !== 'From the catalog' || el.children.length > 0) continue
      const r = el.getBoundingClientRect()
      if (r.width > 0) return { x: r.x + 200, y: r.y + r.height / 2 + 33 }
    }
    return null
  })
  if (!box) throw new Error('no "From the catalog" heading in the Rune mirror')
  await client.page.mouse.click(box.x, box.y)
  await client.page.keyboard.type(query, { delay: 15 })
}

async function run(
  client: Client,
  world: World,
  row: Row
): Promise<{ seen: string[]; state: string }> {
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
  const button = (name: string, group?: string) =>
    react
      ? (group ? client.page.getByRole('group', { name: group, exact: true }) : client.page)
          .getByRole('button', { name, exact: true })
          .or(client.page.getByRole('switch', { name, exact: true }))
          .last()
          .click({ timeout: 20_000 })
      : runeButton(client, name, group)
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
  await act('mcp page', () => nav('MCP servers'))
  await settle(1500)
  await act('search the catalog', () =>
    react ? client.fill('Search the MCP catalog', row.name) : runeSearch(client, row.name)
  )
  await settle(2000)
  await step('catalog', () => client.sees(row.name, 20_000))
  await act('install', () =>
    react
      ? client.page
          .getByRole('button', { name: 'Install', exact: true })
          .first()
          .click({ timeout: 20_000 })
      : runeButton(client, 'Install')
  )
  await settle(1500)
  if (await client.sees('Fill in what this server needs.', 2_000)) {
    await act('review', () => button('Review'))
  }
  await settle(1500)
  await step('consent', () => client.sees('Review exactly what will run.', 15_000))
  await act('approve', () => button('Approve'))
  await settle(3000)
  await step(
    'installed',
    async () =>
      (await client.sees(`Installed`, 15_000)) &&
      JSON.parse(await hostState(world)).mcp.length === 1
  )
  await act('done', () => button('Done'))
  await settle(600)
  await act('add server', () => button('Add server'))
  await settle(600)
  await act('import json', () => button('Import JSON instead'))
  await step('json-view', () =>
    client.sees('Paste a Claude Desktop "mcpServers" config or a single server object.', 10_000)
  )
  await act('close the dialog', () => button('Cancel'))
  await settle(800)
  await act('subagents page', () => nav('Sub-agents'))
  await step('subagents', () => client.sees('Let agents start other agents', 10_000))
  await act('more running', () => button('More running sub-agents'))
  await settle(1500)
  await step('subagents-raised', async () => JSON.parse(await hostState(world)).maxRunning === 4)
  await act('accounts page', () => nav('Accounts'))
  await step('accounts', () => client.sees('Claude Code', 10_000))
  await act('add account', () =>
    react
      ? client.page
          .getByRole('button', { name: 'Add account', exact: true })
          .first()
          .click({ timeout: 20_000 })
      : runeButton(client, 'Add account', 'Claude Code')
  )
  await settle(800)
  await step('add-account', () =>
    client.sees('Give it a short name. Troupe shows this name, never the email.', 10_000)
  )
  await act('cancel add', () => button('Cancel'))
  await settle(600)
  return { seen, state: await hostState(world) }
}

const browser: Browser = await chromium.launch({ args: runeWebArgs(REFERENCE_ARGS) })
const world = await createWorld({
  port: Number(values.port),
  root: path.join(options.root, 'world'),
  binary: values['server-bin'] as string | undefined
})
const results: Record<string, { seen: string[]; state: string }> = {}
let row: Row | null = null
try {
  await catalogSettled(world)
  row = await pickRow(world)
  console.log(`catalog row: ${row.name} ${row.version}`)
  for (const name of ['react', 'rune'] as const) {
    if (values.only && values.only !== name) continue
    await reset(world)
    const drive = name === 'react' ? inReact : inRune
    const picked = row
    results[name] = await drive(browser, world, options, (client) => run(client, world, picked))
    console.log(`${name}: ${JSON.stringify(results[name])}`)
  }
} finally {
  await browser.close()
  await world.server.stop()
}
writePairs(options.root)
const states = new Set(Object.values(results).map((r) => r.state))
const short = Object.entries(results).filter(([, r]) => r.seen.length < 9)
const pass = states.size === 1 && short.length === 0
console.log(
  pass
    ? 'PASS: every client left the same settings'
    : `FAIL: ${short.map(([n]) => n)} states=${states.size}`
)
process.exit(pass ? 0 : 1)
