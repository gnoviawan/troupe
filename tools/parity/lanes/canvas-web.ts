/**
 * R11c Lane 8: in the Rune web client, open the canvas iframe over the canvas via the DOM host,
 * open a Rune menu over it and verify the menu is readable (the iframe moves offscreen while the
 * overlay is open and returns when it closes), and check the phone profile refusal.
 *
 *   bun tools/parity/lanes/canvas-web.ts --port 8198 --root /tmp/rune-r11c/web \
 *     --server-bin <troupe-server> [--media-dir <dir>]
 */

import { copyFileSync, mkdirSync } from 'node:fs'
import path from 'node:path'
import { parseArgs } from 'node:util'
import { chromium } from 'playwright'
import { REFERENCE_ARGS } from '../drivers/chromium'
import { PHONE_VIEWPORT, VIEWPORT } from '../drivers/react'
import { runeWebDist, serveRuneClient } from '../drivers/rune-apps'
import { mirrorPoint, openRuneWeb, runeWebArgs } from '../drivers/rune-web'
import { REPO_DIR } from '../server'
import { createWorld, PROJECTS, prepareDevice, TOKEN } from '../world'
import { until } from './kit'

const { values } = parseArgs({
  args: Bun.argv.slice(2),
  options: {
    port: { type: 'string', default: '8198' },
    root: { type: 'string', default: '/tmp/rune-r11c/web' },
    'server-bin': { type: 'string' },
    'media-dir': { type: 'string' }
  }
})

const root = path.resolve(values.root as string)
const mediaDir = values['media-dir'] ? path.resolve(values['media-dir'] as string) : root
mkdirSync(root, { recursive: true })
mkdirSync(mediaDir, { recursive: true })

const FIXTURE = path.join(REPO_DIR, 'apps/rune/fixtures/sample.figcraft')
const notes: string[] = []
const check = (ok: boolean, what: string): boolean => {
  notes.push(`${ok ? 'ok' : 'FAIL'} ${what}`)
  console.log(`canvas-web: ${ok ? 'ok' : 'FAIL'} ${what}`)
  return ok
}

const world = await createWorld({
  port: Number(values.port),
  root,
  binary: values['server-bin'] as string | undefined
})
copyFileSync(FIXTURE, path.join(world.dirs.alpha, 'sample.figcraft'))

const served = await serveRuneClient(runeWebDist('troupe'), world.server.baseUrl)
const browser = await chromium.launch({ args: runeWebArgs(REFERENCE_ARGS) })
let ok = true
try {
  // 1. Desktop web: open Canvas, verify iframe mounts visible over slot, open Rune menu over it.
  const desktopDevice = 'canvas-web-desktop'
  await prepareDevice(world, desktopDevice)
  const desktop = await openRuneWeb({
    browser,
    url: `${served.url}/`,
    ready: 'What should we do in alpha?',
    viewport: VIEWPORT,
    storage: { 'termul.webAuthToken': TOKEN, 'termul.deviceId': desktopDevice }
  })
  try {
    const { page } = desktop
    const canvasBtn = await mirrorPoint(page, 'Canvas', true)
    if (!canvasBtn) throw new Error('rail Canvas button not found in mirror')
    await page.mouse.click(canvasBtn.x, canvasBtn.y)

    const iframeSelector = `iframe[data-canvas-project="${PROJECTS.alpha.id}"]`
    const iframeVisible = await until(async () => {
      return page.evaluate((sel) => {
        const el = document.querySelector(sel)
        if (!el) return false
        const r = el.getBoundingClientRect()
        return (
          el.getAttribute('data-canvas-frame-state') === 'visible' &&
          r.width > 300 &&
          r.height > 200
        )
      }, iframeSelector)
    }, 30_000)
    ok = check(iframeVisible, 'canvas iframe mounted and positioned over slot') && ok

    const ready = await until(async () => (await mirrorPoint(page, 'Ready', true)) !== null, 45_000)
    ok = check(ready, 'canvas editor reported Ready over postMessage') && ok
    await page.waitForTimeout(1500)
    await page.screenshot({ path: path.join(mediaDir, 'r11-canvas-web.png') })

    // Open a Rune menu (Command Palette via Search in the rail) over the canvas.
    const searchBtn = await mirrorPoint(page, 'Search', true)
    if (!searchBtn) throw new Error('rail Search button not found in mirror')
    await page.mouse.click(searchBtn.x, searchBtn.y)
    const menuVisible = await until(
      async () => (await mirrorPoint(page, 'Search commands', false)) !== null,
      10_000
    )
    ok = check(menuVisible, 'Rune menu opened over canvas') && ok

    const iframeHidden = await until(async () => {
      return page.evaluate((sel) => {
        const el = document.querySelector(sel)
        return el?.getAttribute('data-canvas-frame-state') === 'hidden'
      }, iframeSelector)
    }, 5_000)
    ok = check(iframeHidden, 'iframe moved offscreen while Rune menu is open (menu readable)') && ok
    await page.waitForTimeout(500)
    await page.screenshot({ path: path.join(mediaDir, 'r11-canvas-web-menu.png') })

    // Close the menu with Escape and confirm iframe returns.
    await page.keyboard.press('Escape')
    const iframeReturned = await until(async () => {
      return page.evaluate((sel) => {
        const el = document.querySelector(sel)
        return el?.getAttribute('data-canvas-frame-state') === 'visible'
      }, iframeSelector)
    }, 5_000)
    ok = check(iframeReturned, 'iframe returned to visible slot after menu closed') && ok
  } finally {
    await desktop.close()
  }

  // 2. Phone profile (390x844): open menu -> Files -> tap sample.figcraft -> verify refusal notice.
  const phoneDevice = 'canvas-web-phone'
  await prepareDevice(world, phoneDevice)
  const phone = await openRuneWeb({
    browser,
    url: `${served.url}/`,
    ready: 'What should we do in alpha?',
    viewport: PHONE_VIEWPORT,
    touch: true,
    storage: { 'termul.webAuthToken': TOKEN, 'termul.deviceId': phoneDevice }
  })
  try {
    const { page } = phone
    const drawerBtn = await mirrorPoint(page, 'Open menu', false)
    if (!drawerBtn) throw new Error('phone menu button not found')
    await page.touchscreen.tap(drawerBtn.x, drawerBtn.y)
    const filesReady = await until(
      async () => (await mirrorPoint(page, 'Files', true)) !== null,
      10_000
    )
    if (!filesReady) throw new Error('Files tab not found in phone drawer')
    const filesPt = (await mirrorPoint(page, 'Files', true))!
    await page.touchscreen.tap(filesPt.x, filesPt.y)

    const sampleReady = await until(
      async () => (await mirrorPoint(page, 'sample.figcraft', false)) !== null,
      15_000
    )
    if (!sampleReady) throw new Error('sample.figcraft not found in phone Files sheet')
    const samplePt = (await mirrorPoint(page, 'sample.figcraft', false))!
    await page.touchscreen.tap(samplePt.x, samplePt.y)

    const refused = await until(
      async () =>
        (await mirrorPoint(page, 'The canvas is not available at this screen width.', false)) !==
        null,
      15_000
    )
    ok = check(refused, 'phone profile shows canvas refusal notice') && ok

    const noVisibleIframe = await page.evaluate(() => {
      const el = document.querySelector('iframe[data-canvas-project]')
      return el?.getAttribute('data-canvas-frame-state') !== 'visible'
    })
    ok = check(noVisibleIframe, 'no visible canvas iframe on phone profile') && ok
    await page.waitForTimeout(500)
    await page.screenshot({ path: path.join(mediaDir, 'r11-canvas-phone.png') })
  } finally {
    await phone.close()
  }
} catch (error) {
  ok = check(false, String(error))
} finally {
  await browser.close()
  await served.close()
  world.ws.close()
  await world.server.stop()
}

await Bun.write(path.join(root, 'results.json'), JSON.stringify({ ok, notes }, null, 2))
console.log(`canvas-web: ${ok ? 'PASS' : 'FAIL'}`)
process.exitCode = ok ? 0 : 1
