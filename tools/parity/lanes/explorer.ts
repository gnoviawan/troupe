/**
 * The R8a explorer lane: each client opens the Explorer panel, opens `src`, renames
 * `src/utils.ts` from its row menu, creates a file from the header's New File, deletes it from
 * its row menu through the confirm, and opens `README.md` in an editor tab. After each step
 * the tree and the disk are compared. With `--phone` the same steps run in the phone's Files
 * sheet, through each row's actions sheet.
 *
 *   bun tools/parity/lanes/explorer.ts --port 8198 --root /tmp/rune-r8a/explorer \
 *     --server-bin <troupe-server> --dist <dist-web> [--only react|rune]
 */

import { existsSync } from 'node:fs'
import path from 'node:path'
import { parseArgs } from 'node:util'
import { chromium } from 'playwright'
import { REFERENCE_ARGS } from '../drivers/chromium'
import { mirrorPoint, runeWebArgs } from '../drivers/rune-web'
import { REPO_DIR } from '../server'
import { createWorld } from '../world'
import { type Client, inReact, inRune, type LaneOptions, shot, until, writePairs } from './kit'

const { values } = parseArgs({
  args: Bun.argv.slice(2),
  options: {
    port: { type: 'string', default: '8198' },
    root: { type: 'string', default: '/tmp/troupe-parity/explorer' },
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
  device: 'explorer-lane'
}

interface Step {
  step: string
  ok: boolean
}

async function rightClick(client: Client, text: string): Promise<void> {
  const page = client.page
  if (client.name === 'react') {
    await page.getByText(text, { exact: true }).first().click({ button: 'right' })
    return
  }
  let point: { x: number; y: number } | null = null
  await until(async () => {
    point = await mirrorPoint(page, text, true)
    return point !== null
  }, 20_000)
  if (!point) throw new Error(`the Rune mirror never showed "${text}"`)
  const { x, y } = point as { x: number; y: number }
  await page.mouse.click(x, y, { button: 'right' })
}

async function run(client: Client, alpha: string): Promise<Step[]> {
  const out: Step[] = []
  try {
    await (options.phone ? phoneSteps : steps)(client, alpha, out)
  } catch (e) {
    out.push({ step: String(e).split('\n')[0] ?? 'error', ok: false })
  }
  return out
}

async function steps(client: Client, alpha: string, out: Step[]): Promise<void> {
  const page = client.page
  const check = (step: string, ok: boolean) => out.push({ step, ok })
  const disk = (rel: string) => existsSync(path.join(alpha, rel))

  if (!(await client.sees('README.md', 3_000))) await client.click('Explorer')
  check('tree shows', await client.sees('README.md', 30_000))
  await shot(client, options.root, '1-tree')

  await client.click('src')
  check('src opens', await client.sees('utils.ts', 15_000))
  await shot(client, options.root, '2-src')

  await rightClick(client, 'utils.ts')
  await page.waitForTimeout(300)
  await shot(client, options.root, '3-menu')
  await client.click('Rename')
  await page.waitForTimeout(500)
  // Headless Chromium leaves React's field unfocused after the menu closes; a user clicks it.
  if (client.name === 'react') await page.getByRole('textbox', { name: 'New name' }).click()
  await page.keyboard.press('Control+a')
  await page.keyboard.type('helpers.ts', { delay: 20 })
  await page.keyboard.press('Enter')
  check(
    'renamed on disk',
    await until(async () => disk('src/helpers.ts') && !disk('src/utils.ts'), 10_000)
  )
  check('renamed in tree', await client.sees('helpers.ts', 10_000))
  await shot(client, options.root, '4-renamed')

  await client.click('New File')
  await page.waitForTimeout(300)
  await page.keyboard.type('created.rs', { delay: 20 })
  await shot(client, options.root, '5-create-field')
  await page.keyboard.press('Enter')
  check(
    'created on disk',
    await until(async () => disk('src/created.rs') || disk('created.rs'), 10_000)
  )
  check('created in tree', await client.sees('created.rs', 10_000))

  await rightClick(client, 'created.rs')
  await page.waitForTimeout(300)
  await client.click('Delete…', false)
  check('confirm shows', await client.sees('This cannot be undone', 10_000))
  await shot(client, options.root, '6-confirm')
  await client.click('Delete')
  check(
    'deleted on disk',
    await until(async () => !disk('src/created.rs') && !disk('created.rs'), 10_000)
  )
  await page.waitForTimeout(500)
  await shot(client, options.root, '7-deleted')

  await client.click('README.md')
  check('README opens in a tab', await client.sees('The matrix project.', 15_000))
  await shot(client, options.root, '8-opened')
}

async function phoneSteps(client: Client, alpha: string, out: Step[]): Promise<void> {
  const page = client.page
  const check = (step: string, ok: boolean) => out.push({ step, ok })
  const disk = (rel: string) => existsSync(path.join(alpha, rel))

  await client.click('More')
  await page.waitForTimeout(400)
  await client.click('Files')
  check('files sheet shows', await client.sees('Project files', 30_000))
  await shot(client, options.root, 'p1-files')

  await client.click('Open folder src')
  check('src opens', await client.sees('utils.ts', 15_000))
  await shot(client, options.root, 'p2-src')

  await client.click('Actions for utils.ts')
  await page.waitForTimeout(400)
  await shot(client, options.root, 'p3-actions')
  await client.click('Rename')
  await page.waitForTimeout(500)
  if (client.name === 'react') await page.getByRole('textbox', { name: 'Rename utils.ts' }).click()
  await page.keyboard.press('Control+a')
  await page.keyboard.type('helpers.ts', { delay: 20 })
  await page.keyboard.press('Enter')
  check(
    'renamed on disk',
    await until(async () => disk('src/helpers.ts') && !disk('src/utils.ts'), 10_000)
  )
  check('renamed in list', await client.sees('helpers.ts', 10_000))

  await client.click('New file')
  await page.waitForTimeout(500)
  if (client.name === 'react') await page.getByRole('textbox', { name: 'New file name' }).click()
  await page.keyboard.type('created.rs', { delay: 20 })
  await page.keyboard.press('Enter')
  check('created on disk', await until(async () => disk('src/created.rs'), 10_000))
  check('created in list', await client.sees('created.rs', 10_000))
  await shot(client, options.root, 'p4-created')

  await client.click('Actions for created.rs')
  await page.waitForTimeout(400)
  await client.click('Delete')
  check('confirm shows', await client.sees('This cannot be undone', 10_000))
  await shot(client, options.root, 'p5-confirm')
  await client.click('Delete')
  check('deleted on disk', await until(async () => !disk('src/created.rs'), 10_000))

  await client.click('Back to parent folder')
  check('back at the root', await client.sees('Project files', 10_000))
  await client.click('Open README.md')
  check('README opens', await client.sees('The matrix project.', 15_000))
  await shot(client, options.root, 'p6-opened')
}

const world = await createWorld({
  port: Number(values.port),
  root: options.root,
  binary: values['server-bin'] as string | undefined
})
const browser = await chromium.launch({ args: runeWebArgs(REFERENCE_ARGS) })
const results: Record<string, Step[] | string> = {}
try {
  for (const which of ['react', 'rune'] as const) {
    if (values.only && values.only !== which) continue
    const steps = (client: Client) => run(client, world.dirs.alpha)
    try {
      results[which] =
        which === 'react'
          ? await inReact(browser, world, options, steps)
          : await inRune(browser, world, options, steps)
    } catch (e) {
      results[which] = String(e)
    }
    // Put the tree back for the next client.
    const { renameSync } = await import('node:fs')
    const helpers = path.join(world.dirs.alpha, 'src', 'helpers.ts')
    if (existsSync(helpers)) renameSync(helpers, path.join(world.dirs.alpha, 'src', 'utils.ts'))
  }
} finally {
  await browser.close()
  await world.server.stop()
}
writePairs(options.root)
console.log(JSON.stringify(results, null, 2))
