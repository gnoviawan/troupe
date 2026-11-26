/**
 * The R9e automations lane: each client gets its own automation, created on the host as a
 * chat agent would (`POST /api/automations`). The lane opens the Automations tab, opens the
 * editor, adds a Wait step after the trigger and a Fail step after it (so the two are
 * connected), saves, closes the editor and runs it now. It fails unless both clients saved
 * the same graph and the run view shows step status.
 *
 *   bun tools/parity/lanes/automations.ts --port 8194 --root /tmp/rune-r9e/lane \
 *     --server-bin <troupe-server> --dist <dist-web> [--only react|rune] [--phone]
 *
 * Writes `<client>-<step>.png` and `pair-<step>.png` under the root, and `<client>-graph.json`.
 */

import { writeFileSync } from 'node:fs'
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
    root: { type: 'string', default: '/tmp/troupe-parity/automations' },
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
  device: 'automations'
}
const PROJECT = 'matrix-alpha'

async function api<T>(world: World, method: string, route: string, body?: unknown): Promise<T> {
  const reply = await fetch(`${world.server.baseUrl}${route}`, {
    method,
    headers: { Authorization: `Bearer ${TOKEN}`, 'content-type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body)
  })
  const json = (await reply.json()) as { success: boolean; data: T; error?: string }
  if (!json.success) throw new Error(`${method} ${route}: ${json.error}`)
  return json.data
}

type Saved = { id: string; revision: number; graph: unknown; layout: Record<string, unknown> }

async function run(client: Client, world: World, id: string, name: string) {
  const seen: string[] = []
  let index = 0
  const step = async (label: string, check: () => Promise<boolean>) => {
    const ok = await check().catch(() => false)
    if (ok) seen.push(label)
    else console.log(`${client.name}: ${label} failed its check`)
    index += 1
    await shot(client, options.root, `${String(index).padStart(2, '0')}-${label}`)
  }
  const act = async (what: string, fn: () => Promise<void>) => {
    try {
      await fn()
    } catch (error) {
      console.log(`${client.name}: ${what} failed: ${error}`)
    }
  }
  const settle = (ms = 800) => client.page.waitForTimeout(ms)

  await act('open automations', () => client.click('Automations'))
  await settle()
  await act('pick the automation', () => client.click(name))
  await step('detail', () => client.sees('Run policy', 15_000))
  await act('edit', () => client.click('Edit'))
  await step('editor', () => client.sees('No unsaved changes', 10_000))
  if (client.name === 'react') {
    await act('add after the trigger', () => client.click('Add a step after the trigger'))
  } else {
    await act('add a step', () => client.click('Add a step'))
  }
  await settle(500)
  await act('pick Wait', () => client.click('Wait'))
  await settle()
  if (client.name === 'react') {
    await act('add after Wait', () => client.click('Add a step after Wait'))
  } else {
    await act('add a step', () => client.click('Add a step'))
  }
  await settle(500)
  await act('pick Fail', () => client.click('Fail'))
  await settle()
  await step('two-steps', () => client.sees('2 unsaved changes', 10_000))
  await act('save', () => client.click('Save'))
  await settle(1500)
  await step('saved', () => client.sees('No unsaved changes', 10_000))
  await act('close the editor', () => client.click('Close the editor'))
  await settle()
  await act('run now', () => client.click('Run now'))
  await settle(2500)
  await step('run', () => client.sees('Steps', 15_000))
  const saved = await api<{ automation: Saved }>(world, 'GET', `/api/automations/${id}`)
  writeFileSync(
    path.join(options.root, `${client.name}-graph.json`),
    `${JSON.stringify(saved.automation, null, 2)}\n`
  )
  return {
    seen,
    graph: JSON.stringify(saved.automation.graph),
    revision: saved.automation.revision
  }
}

const browser: Browser = await chromium.launch({ args: runeWebArgs(REFERENCE_ARGS) })
const world = await createWorld({
  port: Number(values.port),
  root: path.join(options.root, 'world'),
  binary: values['server-bin'] as string | undefined
})
const results: Record<string, { seen: string[]; graph: string; revision: number }> = {}
try {
  for (const name of ['react', 'rune'] as const) {
    if (values.only && values.only !== name) continue
    const title = `${name === 'react' ? 'React' : 'Rune'} lane digest`
    const created = await api<Saved>(world, 'POST', '/api/automations', {
      project_id: PROJECT,
      name: title
    })
    const drive = name === 'react' ? inReact : inRune
    results[name] = await drive(browser, world, options, (client) =>
      run(client, world, created.id, title)
    )
    console.log(`${name}: ${JSON.stringify(results[name])}`)
  }
} finally {
  await browser.close()
  await world.server.stop()
}
writePairs(options.root)
const graphs = new Set(Object.values(results).map((r) => r.graph))
const complete = Object.values(results).every((r) => r.seen.length === 6)
console.log(
  graphs.size === 1 && complete
    ? 'PASS: every client saved the same graph and showed the run'
    : `FAIL: ${graphs.size} graphs, steps ${JSON.stringify(Object.fromEntries(Object.entries(results).map(([n, r]) => [n, r.seen])))}`
)
process.exit(graphs.size === 1 && complete ? 0 : 1)
