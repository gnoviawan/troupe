/**
 * The React reference of the gallery profile: the `design-system` canvas page
 * in Playwright Chromium, on its Vite dev server. Each registry entry is shot
 * alone: the page hides everything but the entry's item and moves its preview
 * frame to a fixed spot at zoom 1 (as `apps/rune/tools/dump-registry.ts`
 * does), so one frame is the entry exactly as the canvas draws it.
 */

import path from 'node:path'
import type { Browser } from 'playwright'
import { cropRects } from '../diff'
import { pickPort, REPO_DIR } from '../server'
import { type GallerySession, poll } from './driver'
import { paintedParts } from './react'

const VITE_CONFIG = 'apps/rune/tools/design-system.vite.config.ts'
const VITE_TIMEOUT_MS = 120_000
const CANVAS_TIMEOUT_MS = 300_000
/** Where the shot frame sits: room for its shadow and focus ring, as the registry dump leaves. */
const MARGIN = 16

export async function designSystemGallery(browser: Browser): Promise<GallerySession> {
  const port = await pickPort()
  const vite = Bun.spawn(
    [
      'bun',
      'x',
      'vite',
      '--config',
      VITE_CONFIG,
      '--host',
      '127.0.0.1',
      '--port',
      String(port),
      '--strictPort'
    ],
    { cwd: REPO_DIR, stdout: 'ignore', stderr: 'pipe' }
  )
  console.error(`[design-system] vite pid ${vite.pid} on ${port}`)
  const base = `http://127.0.0.1:${port}/`
  const context = await browser.newContext({
    viewport: { width: 1600, height: 1400 },
    deviceScaleFactor: 1,
    colorScheme: 'dark',
    reducedMotion: 'reduce',
    serviceWorkers: 'block'
  })
  const close = async () => {
    await context.close()
    vite.kill()
    await vite.exited
  }
  try {
    const up = await poll(
      () =>
        fetch(base).then(
          (r) => r.ok,
          () => false
        ),
      VITE_TIMEOUT_MS
    )
    if (!up) throw new Error(`the design-system server at ${base} never answered`)
    await context.addInitScript(() => {
      localStorage.setItem(
        'termul.design-system-canvas.layout',
        JSON.stringify({ v: 1, camera: null, themeId: 'termul', positions: {} })
      )
    })
    const page = await context.newPage()
    await page.goto(base, { waitUntil: 'commit' })
    await page.waitForSelector('[data-canvas-item] [data-preview-frame]', {
      timeout: CANVAS_TIMEOUT_MS
    })
    await page.evaluate(() => document.fonts.ready)
    return {
      renderer: `Chromium ${browser.version()}`,
      async shoot(entry, file) {
        await page.evaluate(
          ({ id, margin }) => {
            const item = document.querySelector<HTMLElement>(
              `[data-canvas-item="${CSS.escape(id)}"]`
            )
            const frame = item?.querySelector<HTMLElement>('[data-preview-frame]')
            const world = item?.parentElement
            if (!item || !frame || !world) throw new Error(`the canvas has no frame for ${id}`)
            // Only the world stays visible: the page header and toolbar float over the canvas.
            for (let node: Element | null = world; node?.parentElement; node = node.parentElement) {
              for (const sibling of node.parentElement.children) {
                if (sibling !== node) (sibling as HTMLElement).style.visibility = 'hidden'
              }
            }
            for (const other of world.children) {
              ;(other as HTMLElement).style.visibility = other === item ? '' : 'hidden'
            }
            // The camera writes the world's transform only when it moves, so this one stays.
            world.style.transform = 'none'
            const placed = frame.getBoundingClientRect()
            world.style.transform = `translate(${margin - placed.left}px, ${margin - placed.top}px)`
          },
          { id: entry.id, margin: MARGIN }
        )
        await page.evaluate(
          () =>
            new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))
        )
        const clip = { x: MARGIN, y: MARGIN, width: entry.width, height: entry.height }
        await page.screenshot({
          path: path.resolve(file),
          clip,
          animations: 'disabled',
          caret: 'hide'
        })
        return { via: 'dom', parts: cropRects(await page.evaluate(paintedParts), clip) }
      },
      close
    }
  } catch (error) {
    await close()
    const stderr = await new Response(vite.stderr).text().catch(() => '')
    throw new Error(
      `${error instanceof Error ? error.message : error}${stderr ? `; vite: ${stderr.trim().split('\n').at(-1)}` : ''}`
    )
  }
}
