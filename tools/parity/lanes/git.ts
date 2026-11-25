/**
 * The R9a git lane: in alpha's Changes panel, stage README.md, commit it, open the History
 * tab and the new commit's diff, then switch to feature/login carrying the changes. Each
 * client starts from the seeded working tree, and the lane fails unless both clients leave
 * the repository in the same state (`git status` and the last subjects).
 *
 *   bun tools/parity/lanes/git.ts --port 8195 --root /tmp/rune-r9a/lane \
 *     --server-bin <troupe-server> --dist <dist-web>
 *
 * Writes `<client>-<step>.png` and `pair-<step>.png` under the root.
 */

import path from 'node:path'
import { parseArgs } from 'node:util'
import { type Browser, chromium } from 'playwright'
import { REFERENCE_ARGS } from '../drivers/chromium'
import { mirrorPoint, runeWebArgs } from '../drivers/rune-web'
import { REPO_DIR } from '../server'
import { createWorld, resetAlpha, type World } from '../world'
import { type Client, inReact, inRune, type LaneOptions, shot, writePairs } from './kit'

const { values } = parseArgs({
  args: Bun.argv.slice(2),
  options: {
    port: { type: 'string', default: '8195' },
    root: { type: 'string', default: '/tmp/troupe-parity/git' },
    'server-bin': { type: 'string' },
    dist: { type: 'string', default: path.join(REPO_DIR, 'dist-web') },
    only: { type: 'string' }
  }
})
const options: LaneOptions = {
  root: path.resolve(values.root as string),
  phone: false,
  dist: path.resolve(values.dist as string),
  device: 'git'
}
const STEPS = ['changes', 'staged', 'committed', 'history', 'commit-diff', 'switched']
const SUBJECT = 'lane: update the readme'

function repo(world: World, ...args: string[]): string {
  const proc = Bun.spawnSync(['git', ...args], { cwd: world.dirs.alpha, stdout: 'pipe' })
  return proc.stdout.toString().trim()
}

function repoState(world: World): string {
  return [
    repo(world, 'status', '--porcelain=v1', '-b'),
    repo(world, 'log', '-2', '--format=%s')
  ].join('\n')
}

async function run(client: Client, world: World): Promise<{ seen: string[]; state: string }> {
  const seen: string[] = []
  const step = async (name: string, check: () => Promise<boolean>) => {
    const ok = await check().catch(() => false)
    if (ok) seen.push(name)
    else console.log(`${client.name}: ${name} failed its check`)
    await shot(client, options.root, `${STEPS.indexOf(name) + 1}-${name}`)
  }
  const act = async (what: string, fn: () => Promise<void>) => {
    try {
      await fn()
    } catch (error) {
      console.log(`${client.name}: ${what} failed: ${error}`)
    }
  }
  const settle = (ms = 1500) => client.page.waitForTimeout(ms)
  await client.sees('What should we do in alpha?', 30_000)
  await act('open Changes', () => client.click('Changes'))
  await step('changes', () => client.sees('README.md', 20_000))
  await act('stage', () => client.click('Stage changes'))
  await settle()
  await step('staged', async () =>
    repo(world, 'diff', '--cached', '--name-only').includes('README.md')
  )
  await act('summary', async () => {
    if (client.name === 'react') await client.fill('Commit summary', SUBJECT)
    else {
      // An unfocused Rune text field has no mirror node: the summary sits 104 points above
      // the amend checkbox's row.
      const amend = await mirrorPoint(client.page, 'Amend last commit', true)
      if (!amend) throw new Error('no amend row')
      await client.page.mouse.click(amend.x, amend.y - 104)
      await client.page.keyboard.type(SUBJECT, { delay: 15 })
    }
  })
  await act('commit', () => client.click('Commit to main'))
  await settle(2500)
  await step(
    'committed',
    async () =>
      repo(world, 'log', '-1', '--format=%s') === SUBJECT && (await client.sees(SUBJECT, 15_000))
  )
  await act('view all', () =>
    client.click(client.name === 'react' ? 'View all' : 'Open History in the main panel')
  )
  await step('history', () => client.sees('initial commit', 20_000))
  await act('select commit', () => client.click(SUBJECT, false))
  await settle()
  await act('open diff', () => client.click('Open diff'))
  await step('commit-diff', () => client.sees('An unstaged line.', 20_000))
  await act('branch menu', async () => {
    if (client.name === 'react')
      await client.page
        .locator('[data-testid="side-panel-header"] button:has-text("main")')
        .first()
        .click({ timeout: 20_000 })
    else await client.click('Switch branch')
  })
  await settle(800)
  await act('pick branch', async () => {
    if (client.name === 'react')
      await client.page
        .getByRole('menuitem', { name: 'feature/login' })
        .first()
        .click({ timeout: 20_000 })
    else await client.click('feature/login')
  })
  await settle(800)
  await act('bring changes', () => client.click('Bring Changes'))
  await settle(2500)
  await step(
    'switched',
    async () => repo(world, 'rev-parse', '--abbrev-ref', 'HEAD') === 'feature/login'
  )
  return { seen, state: repoState(world) }
}

const world = await createWorld({
  port: Number(values.port),
  root: options.root,
  binary: values['server-bin']
})
let browser: Browser | undefined
try {
  browser = await chromium.launch({ args: runeWebArgs(REFERENCE_ARGS) })
  const results: Array<[string, { seen: string[]; state: string }]> = []
  for (const name of ['react', 'rune'] as const) {
    if (values.only && values.only !== name) continue
    await resetAlpha(world)
    const runner = name === 'react' ? inReact : inRune
    results.push([name, await runner(browser, world, options, (client) => run(client, world))])
  }
  let pass = true
  for (const [name, { seen, state }] of results) {
    const ok = STEPS.every((s) => seen.includes(s))
    pass &&= ok
    console.log(`${name}: ${ok ? 'PASS' : 'FAIL'} ${JSON.stringify(seen)}`)
    console.log(`${name} repository:\n${state}`)
  }
  if (results.length === 2) {
    const same = results[0][1].state === results[1][1].state
    pass &&= same
    console.log(
      same
        ? 'PASS: both clients left the same repository state'
        : 'FAIL: the repository states differ'
    )
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
