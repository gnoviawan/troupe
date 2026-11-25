/**
 * The R8b viewers lane: each client opens a PNG, a 10-page PDF (scrolled to page 5) and an
 * MP4 (played for 3 s, with two frames 1 s apart compared) in an editor tab.
 *
 *   bun tools/parity/lanes/viewers.ts --port 8193 --root /tmp/rune-r8b/viewers \
 *     --server-bin <troupe-server> --dist <dist-web> --png a.png --pdf a.pdf --mp4 a.mp4 \
 *     [--only react|rune]
 */

import { copyFileSync } from 'node:fs'
import path from 'node:path'
import { parseArgs } from 'node:util'
import { chromium } from 'playwright'
import { REFERENCE_ARGS } from '../drivers/chromium'
import { runeWebArgs } from '../drivers/rune-web'
import { REPO_DIR } from '../server'
import { createWorld, seedEditorTab } from '../world'
import { type Client, inReact, inRune, type LaneOptions, shot, writePairs } from './kit'

const { values } = parseArgs({
  args: Bun.argv.slice(2),
  options: {
    port: { type: 'string', default: '8193' },
    root: { type: 'string', default: '/tmp/troupe-parity/viewers' },
    'server-bin': { type: 'string' },
    dist: { type: 'string', default: path.join(REPO_DIR, 'dist-web') },
    png: { type: 'string' },
    pdf: { type: 'string' },
    mp4: { type: 'string' },
    only: { type: 'string' }
  }
})
const options: LaneOptions = {
  root: path.resolve(values.root as string),
  phone: false,
  dist: path.resolve(values.dist as string),
  device: 'viewers-lane'
}

interface Result {
  client: string
  image: string
  pdf: string
  video: string
  framesDiffer: boolean
}

/** The visible text, for the facts line and the page counter. */
async function bodyText(client: Client): Promise<string> {
  return client.page.evaluate(() => document.body.innerText.replace(/\s+/g, ' '))
}

async function image(client: Client): Promise<string> {
  await client.sees('Fit', 30_000)
  await shot(client, options.root, '1-image')
  return /\d+ × \d+ · [\d.]+ [KM]?B/.exec(await bodyText(client))?.[0] ?? ''
}

async function pdf(client: Client): Promise<string> {
  await client.sees('1 / 10', 30_000)
  await client.page.waitForTimeout(1500)
  await shot(client, options.root, '2-pdf')
  await client.page.mouse.move(640, 500)
  for (let i = 0; i < 40; i++) {
    await client.page.mouse.wheel(0, 200)
    await client.page.waitForTimeout(60)
    if (/\b5 \/ 10\b/.test(await bodyText(client))) break
  }
  await client.page.waitForTimeout(1500)
  await shot(client, options.root, '3-pdf-page-5')
  return /\d+ \/ 10/.exec(await bodyText(client))?.[0] ?? ''
}

async function video(client: Client): Promise<{ facts: string; differ: boolean }> {
  await client.page.waitForTimeout(2000)
  if (client.name === 'react') {
    await client.page.evaluate(() => document.querySelector('video')?.play())
  } else {
    await client.click('Play', true)
  }
  await client.page.waitForTimeout(1000)
  const a = await client.page.screenshot()
  await client.page.waitForTimeout(1000)
  const b = await client.page.screenshot()
  await client.page.waitForTimeout(1000)
  await shot(client, options.root, '4-video')
  const facts =
    /\d+ × \d+ · [\d:]+( \/ [\d:]+)? · [\d.]+ [KM]?B/.exec(await bodyText(client))?.[0] ?? ''
  return { facts, differ: Buffer.compare(a, b) !== 0 }
}

const world = await createWorld({
  port: Number(values.port),
  root: options.root,
  binary: values['server-bin'] as string | undefined
})
const files = {
  png: path.join(world.dirs.alpha, 'shot.png'),
  pdf: path.join(world.dirs.alpha, 'ten-pages.pdf'),
  mp4: path.join(world.dirs.alpha, 'clip.mp4')
}
copyFileSync(values.png as string, files.png)
copyFileSync(values.pdf as string, files.pdf)
copyFileSync(values.mp4 as string, files.mp4)
const browser = await chromium.launch({ args: runeWebArgs(REFERENCE_ARGS) })
const results: Result[] = []
try {
  for (const which of ['react', 'rune'] as const) {
    if (values.only && values.only !== which) continue
    const device = `${options.device}-${which}`
    const inClient = which === 'react' ? inReact : inRune
    await seedEditorTab(world, device, files.png)
    const img = await inClient(browser, world, options, image)
    await seedEditorTab(world, device, files.pdf)
    const page = await inClient(browser, world, options, pdf)
    await seedEditorTab(world, device, files.mp4)
    const vid = await inClient(browser, world, options, video)
    results.push({
      client: which,
      image: img,
      pdf: page,
      video: vid.facts,
      framesDiffer: vid.differ
    })
  }
} finally {
  await browser.close()
  await world.server.stop()
}
writePairs(options.root)
console.log(JSON.stringify(results, null, 2))
