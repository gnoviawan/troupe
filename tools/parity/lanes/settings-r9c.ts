/**
 * The R9c settings lane: add a terminal profile, open Skills, add a local MCP server from the
 * form, open its detail page and remove it, then rename the active project from the project
 * settings window and put the name back. Each client starts from the same world, and the lane
 * fails unless both leave the host with the same profile, an empty MCP list and the project's
 * own name.
 *
 *   bun tools/parity/lanes/settings-r9c.ts --port 8192 --root /tmp/rune-r9c/lane \
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
import { createWorld, ensureProject, PROJECTS, type World } from '../world'
import { type Client, inReact, inRune, type LaneOptions, shot, writePairs } from './kit'

const { values } = parseArgs({
  args: Bun.argv.slice(2),
  options: {
    port: { type: 'string', default: '8192' },
    root: { type: 'string', default: '/tmp/troupe-parity/settings-r9c' },
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
  device: 'settings-r9c'
}
const STEPS = options.phone ? 9 : 12

async function read<T>(world: World, key: string): Promise<T | undefined> {
  const reply = await world.ws.request<{ value?: { data?: T } }>('store_read', { key })
  return reply.value?.data
}

async function hostState(world: World): Promise<string> {
  const app =
    (await read<{ terminalProfiles?: { name: string }[] }>(world, 'shared/lww/settings/app')) ?? {}
  const mcp = (await read<{ name: string }[]>(world, 'shared/lww/acp/mcp-servers')) ?? []
  const body = (await (await serverFetch(world.server, '/projects')).json()) as { data?: unknown }
  const list = (body && typeof body === 'object' && 'data' in body ? body.data : body) as unknown
  const rows = (
    Array.isArray(list) ? list : ((list as { projects?: unknown[] }).projects ?? [])
  ) as { id: string; name: string }[]
  return JSON.stringify({
    profiles: (app.terminalProfiles ?? []).map((p) => p.name),
    mcp: mcp.map((s) => s.name),
    alpha: rows.find((p) => p.id === PROJECTS.alpha.id)?.name
  })
}

async function reset(world: World): Promise<void> {
  await ensureProject(world, 'alpha')
  for (const key of ['shared/lww/settings/app', 'shared/lww/acp/mcp-servers']) {
    const current = await world.ws.request<{ revision?: number; value?: { data?: unknown } }>(
      'store_read',
      { key }
    )
    const data = key.endsWith('settings/app')
      ? { ...((current.value?.data as object) ?? {}), terminalProfiles: [] }
      : []
    await world.ws.request('store_write', {
      key,
      value: { _version: 1, data },
      baseRevision: current.revision ?? 0
    })
  }
}

/** Presses the last Rune mirror button named `name`: a dialog's, over the page's button of the same name. */
async function runeButton(client: Client, name: string): Promise<void> {
  const pressed = await client.page.evaluate((want) => {
    const root = document.querySelector('[data-rune-a11y]')
    const named = [
      ...(root?.querySelectorAll('button, [role="button"], [role="radio"]') ?? [])
    ].filter((el) => (el.getAttribute('aria-label') ?? el.textContent ?? '').trim() === want)
    const last = named.at(-1) as HTMLElement | undefined
    last?.click()
    return Boolean(last)
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
  const react = client.name === 'react'
  const button = (name: string) =>
    react
      ? client.page
          .getByRole('button', { name, exact: true })
          .or(client.page.getByRole('radio', { name, exact: true }))
          .last()
          .click({ timeout: 20_000 })
      : runeButton(client, name)
  const nav = (name: string) =>
    react
      ? client.page
          .locator(`nav[aria-label="Settings pages"] >> role=button[name="${name}"s]`)
          .click({ timeout: 20_000 })
      : runeButton(client, name)
  /** Replaces the text of the field named `label`. */
  const replace = async (label: string, text: string) => {
    if (react) return client.fill(label, text)
    // The mirror at the pin has no node for an unfocused text field, and the row is named like
    // its field: press the row's control, 240 px wide at its right edge inside 16 px of padding.
    const box = await client.page.evaluate((want) => {
      const root = document.querySelector('[data-rune-a11y]')
      for (const el of root?.querySelectorAll('[role="group"]') ?? []) {
        if ((el.getAttribute('aria-label') ?? '').trim() !== want) continue
        const r = el.getBoundingClientRect()
        if (r.width > 0) return { x: r.right - 16 - 120, y: r.y + r.height / 2 }
      }
      return null
    }, label)
    if (!box) throw new Error(`no Rune row named "${label}"`)
    await client.page.mouse.click(box.x, box.y)
    await client.page.keyboard.press('Control+a')
    await client.page.keyboard.press('Backspace')
    await client.page.keyboard.type(text, { delay: 15 })
  }

  await client.sees('What should we do in alpha?', 30_000)
  // A phone opens Settings from the menu drawer's footer.
  if (options.phone) await act('open the menu', () => client.click('Open menu', false))
  await settle(400)
  await act('open settings', () =>
    options.phone && !react ? client.click('Settings') : button('Settings')
  )
  await settle()
  await act('profiles page', () => nav('Profiles'))
  await step('profiles', () => client.sees('No terminal profiles', 10_000))
  await act('add profile', () => button('Add profile'))
  await settle(400)
  await act('profile name', () => client.fill('Name', 'Node dev'))
  await step('profile-editor', () => client.sees('New profile', 5_000))
  await act('save profile', () => button('Save profile'))
  await settle()
  await step('profile-saved', async () =>
    JSON.parse(await hostState(world)).profiles.includes('Node dev')
  )
  await act('skills page', () => nav('Skills'))
  await step('skills', () =>
    client.sees('Skills in your home folder and in the open project.', 10_000)
  )
  await act('mcp page', () => nav('MCP servers'))
  await step('mcp', () => client.sees('No MCP servers yet', 10_000))
  await act('add server', () => button('Add server'))
  await settle(400)
  await act('local kind', () => client.click('On this computer'))
  await settle(300)
  await act('server name', () => client.fill('Name', 'docs'))
  await act('command', () => client.fill('Command', 'npx -y @mcp/docs'))
  await step('mcp-dialog', () => client.sees('Add MCP server', 5_000))
  await act('add it', () => button('Add server'))
  await settle(1500)
  await step('mcp-added', async () => JSON.parse(await hostState(world)).mcp.includes('docs'))
  await act('open docs', () => client.click('docs'))
  await step('mcp-detail', () => client.sees('Remove server', 10_000))
  await act('remove', () => button('Remove'))
  await settle()
  await step(
    'mcp-removed',
    async () =>
      (await client.sees('Deleted docs', 5_000)) &&
      JSON.parse(await hostState(world)).mcp.length === 0
  )
  // The phone run covers the App Preferences pages; the project window opens from other
  // menus there.
  if (options.phone) return { seen, state: await hostState(world) }
  // The Undo toast stays until it is closed, over the project window's Save.
  await act('close the undo toast', () => button('Close toast'))
  await client.page.keyboard.press('Escape')
  await settle(400)
  await act('open the palette', () => client.click('Search'))
  await settle(400)
  await client.page.keyboard.type('Project Settings', { delay: 20 })
  await settle(300)
  await act('run Project Settings', () =>
    client.click('Configure the active project workspace', false)
  )
  await settle()
  await step('project', () => client.sees('Basic project identification and location.', 10_000))
  await act('rename', () => replace('Name', 'alpha renamed'))
  await step('project-dirty', () => client.sees('Unsaved changes', 5_000))
  await act('save', () => button('Save changes'))
  await settle(1500)
  const renamed = JSON.parse(await hostState(world)).alpha === 'alpha renamed'
  await act('rename back', () => replace('Name', 'alpha'))
  await act('save back', () => button('Save changes'))
  await settle(1500)
  await step(
    'project-saved',
    async () => renamed && JSON.parse(await hostState(world)).alpha === 'alpha'
  )
  return { seen, state: await hostState(world) }
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
    await reset(world)
    const drive = name === 'react' ? inReact : inRune
    results[name] = await drive(browser, world, options, (client) => run(client, world))
    console.log(`${name}: ${JSON.stringify(results[name])}`)
  }
} finally {
  await browser.close()
  await world.server.stop()
}
writePairs(options.root)
const want = JSON.stringify({ profiles: ['Node dev'], mcp: [], alpha: 'alpha' })
const failed = Object.entries(results).filter(([, r]) => r.state !== want || r.seen.length < STEPS)
console.log(
  failed.length === 0
    ? 'PASS: every client left the same settings'
    : `FAIL: ${failed.map(([n]) => n)}`
)
process.exit(failed.length === 0 ? 0 : 1)
