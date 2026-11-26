/**
 * The `react` client: today's web client in Playwright Chromium, the matrix's
 * reference capture.
 *
 * Capture settings: 1280x800, dark, reduced motion, device scale 1, slight
 * font hinting and grayscale text (`chromium.ts`), service worker blocked,
 * and the page clock fixed at the world's clock.
 * The server serves the `dist-web` it was built with, so the page's static
 * files are answered from `dist` by route interception instead. That is how
 * a scratch build (a planted change) runs against the same server.
 */

import { existsSync } from 'node:fs'
import path from 'node:path'
import type { Browser, Locator, Page } from 'playwright'
import type { PartRects } from '../diff'
import type { ActionLog, Target } from '../types'
import { TOKEN } from '../world'
import {
  type ClientLauncher,
  type Driver,
  describe,
  errorLine,
  GONE_TIMEOUT_MS,
  normalize,
  ordered,
  PARK_POINT,
  PARKS_POINTER,
  type Point,
  type Points,
  PRESENT_TIMEOUT_MS,
  poll,
  SETTLE_AFTER_ACTION_MS
} from './driver'

export const VIEWPORT = { width: 1280, height: 800 }
/** The phone profile: an iPhone 12-class portrait window with touch, at device scale 1. */
export const PHONE_VIEWPORT = { width: 390, height: 844 }
export const BOOT_TEXT = 'What should we do in alpha?'
const READY_TIMEOUT_MS = 30_000
const SETTLE_BEFORE_GRAB_MS = 400

/** `ctrl-shift-k` -> `Control+Shift+k`, `escape` -> `Escape`; strokes split on spaces. */
export function playwrightKeys(keys: string): string[] {
  const named: Record<string, string> = {
    escape: 'Escape',
    enter: 'Enter',
    tab: 'Tab',
    backspace: 'Backspace',
    delete: 'Delete',
    up: 'ArrowUp',
    down: 'ArrowDown',
    left: 'ArrowLeft',
    right: 'ArrowRight',
    space: ' ',
    home: 'Home',
    end: 'End',
    f2: 'F2'
  }
  const mods: Record<string, string> = { ctrl: 'Control', alt: 'Alt', shift: 'Shift', cmd: 'Meta' }
  return keys.split(' ').map((stroke) => {
    const parts = stroke.match(/^((?:(?:ctrl|alt|shift|cmd)-)*)(.+)$/)
    const prefix = (parts?.[1] ?? '').split('-').filter(Boolean)
    const key = parts?.[2] ?? stroke
    return [...prefix.map((m) => mods[m]), named[key] ?? key].join('+')
  })
}

function locate(page: Page, target: Target): Locator {
  if ('testId' in target) return page.getByTestId(target.testId).filter({ visible: true }).first()
  if ('text' in target) {
    return page
      .getByText(target.text, { exact: target.exact ?? true })
      .filter({ visible: true })
      .first()
  }
  if ('label' in target) {
    return page
      .getByRole(target.role ?? 'button', { name: target.label, exact: true })
      .filter({ visible: true })
      .first()
  }
  if ('point' in target) throw new Error('a point target names no DOM element')
  return page.locator(target.css).filter({ visible: true }).first()
}

/**
 * Text and icon boxes of the current frame, from the DOM: one rect per line
 * box of every visible text node, the content box of every text field, and
 * the box of every outermost svg and every img.
 */
export function paintedParts(): PartRects {
  const box = (r: DOMRect) => ({ x: r.x, y: r.y, width: r.width, height: r.height })
  const onScreen = (r: DOMRect) =>
    r.width > 0 &&
    r.height > 0 &&
    r.right > 0 &&
    r.bottom > 0 &&
    r.x < innerWidth &&
    r.y < innerHeight
  const seen = (el: Element) =>
    el.checkVisibility({ opacityProperty: true, visibilityProperty: true })
  const text: PartRects['text'] = []
  const icons: PartRects['icons'] = []
  const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT)
  const range = document.createRange()
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    if (!node.textContent?.trim() || !node.parentElement || !seen(node.parentElement)) continue
    if (node.parentElement.closest('svg')) continue
    range.selectNodeContents(node)
    for (const r of range.getClientRects()) if (onScreen(r)) text.push(box(r))
  }
  for (const el of document.querySelectorAll('input, textarea, [contenteditable="true"]')) {
    const r = el.getBoundingClientRect()
    if (seen(el) && onScreen(r)) text.push(box(r))
  }
  for (const el of document.querySelectorAll('svg, img')) {
    if (el.parentElement?.closest('svg')) continue
    const r = el.getBoundingClientRect()
    if (seen(el) && onScreen(r)) icons.push(box(r))
  }
  return { text, icons }
}

/** `touch`: a click is a tap and no pointer is parked (the phone profile). */
function pageDriver(page: Page, points: Points, filesDir: string, touch: boolean): Driver {
  /** Where the client acts on `target`: its centre, or the `at` / `frac` spot inside it. */
  const pointOf = async (key: string, target: Target): Promise<Point> => {
    if ('point' in target) return target.point
    const locator = locate(page, target)
    await locator.waitFor({ state: 'visible', timeout: PRESENT_TIMEOUT_MS })
    const box = await locator.boundingBox()
    if (!box) throw new Error('no box')
    const spot: { at?: Point; frac?: Point } = 'css' in target ? target : {}
    const point = spot.at
      ? { x: box.x + spot.at.x, y: box.y + spot.at.y }
      : spot.frac
        ? { x: box.x + box.width * spot.frac.x, y: box.y + box.height * spot.frac.y }
        : { x: box.x + box.width / 2, y: box.y + box.height / 2 }
    points.set(key, point)
    return point
  }
  const atPoint = (target: Target) =>
    'point' in target || ('css' in target && !!(target.at || target.frac))
  return {
    async act(step, index, action) {
      const key = `${step}/${index}`
      const log: ActionLog = { step, action: describe(action), via: 'dom', ok: true }
      try {
        switch (action.do) {
          case 'click': {
            const point = await pointOf(key, action.target)
            if (touch && !action.button && !action.modifiers) {
              if (atPoint(action.target)) await page.touchscreen.tap(point.x, point.y)
              else await locate(page, action.target).tap()
              break
            }
            if (atPoint(action.target)) {
              if (action.modifiers) await page.keyboard.down('Control')
              await page.mouse.click(point.x, point.y, { button: action.button })
              if (action.modifiers) await page.keyboard.up('Control')
              break
            }
            await locate(page, action.target).click({
              button: action.button,
              modifiers: action.modifiers ? ['ControlOrMeta'] : undefined
            })
            break
          }
          case 'dblclick':
            await pointOf(key, action.target)
            await locate(page, action.target).dblclick()
            break
          case 'hover':
            await pointOf(key, action.target)
            await locate(page, action.target).hover()
            break
          case 'wheel': {
            const point = await pointOf(key, action.target)
            await page.mouse.move(point.x, point.y)
            await page.mouse.wheel(0, action.deltaY)
            break
          }
          case 'drag': {
            const from = await pointOf(key, action.from)
            const to = await pointOf(`${key}/to`, action.to)
            if (!atPoint(action.to)) {
              await locate(page, action.from).dragTo(locate(page, action.to))
              break
            }
            // HTML5 drags (tabs, explorer rows) reveal a pane's drop zones only once they start.
            await page.mouse.move(from.x, from.y)
            await page.mouse.down()
            await page.mouse.move(from.x + 12, from.y + 4, { steps: 4 })
            await page.mouse.move(to.x, to.y, { steps: 16 })
            await page.waitForTimeout(200)
            await page.mouse.up()
            break
          }
          case 'press':
            log.via = 'key'
            for (const stroke of playwrightKeys(action.keys)) await page.keyboard.press(stroke)
            break
          case 'type':
            log.via = 'key'
            await page.keyboard.type(action.text)
            break
          case 'wait':
            log.via = 'none'
            await page.waitForTimeout(action.ms)
            break
          case 'choose': {
            await pointOf(key, action.target)
            const [chooser] = await Promise.all([
              page.waitForEvent('filechooser', { timeout: PRESENT_TIMEOUT_MS }),
              locate(page, action.target).click()
            ])
            await chooser.setFiles(path.join(filesDir, action.file))
            break
          }
        }
        if (!touch && PARKS_POINTER.has(action.do))
          await page.mouse.move(PARK_POINT.x, PARK_POINT.y)
        await page.waitForTimeout(SETTLE_AFTER_ACTION_MS)
      } catch (error) {
        log.ok = false
        log.error = errorLine(error)
      }
      return log
    },
    async check(step, check) {
      /** The painted text: rendered text, then the placeholder of each visible empty field. */
      const textOf = (within?: string) =>
        page.evaluate((testId) => {
          const root = (
            testId ? document.querySelector(`[data-testid="${testId}"]`) : document.body
          ) as HTMLElement | null
          if (!root) return ''
          const placeholders = [
            ...root.querySelectorAll<HTMLInputElement>('input[placeholder], textarea[placeholder]')
          ]
            .filter((field) => field.value === '' && field.checkVisibility())
            .map((field) => field.placeholder)
          return [root.innerText, ...placeholders].join('\n')
        }, within)
      if (check.order) {
        const order = check.order
        return {
          step,
          text: order.join(' < '),
          expect: 'present',
          pass: await poll(
            async () => ordered(await textOf(check.within), order),
            PRESENT_TIMEOUT_MS
          )
        }
      }
      const want = normalize(check.text)
      const painted = async () => normalize(await textOf(check.within)).includes(want)
      const pass = check.gone
        ? await poll(async () => !(await painted()), check.timeoutMs ?? GONE_TIMEOUT_MS)
        : await poll(painted, check.timeoutMs ?? PRESENT_TIMEOUT_MS)
      return { step, text: check.text, expect: check.gone ? 'gone' : 'present', pass }
    },
    async grab(file) {
      await page.evaluate(() => document.fonts.ready)
      await page.waitForTimeout(SETTLE_BEFORE_GRAB_MS)
      await page.screenshot({ path: file, animations: 'disabled', caret: 'hide' })
    },
    async measure(selector) {
      const locator = page.locator(selector).filter({ visible: true }).first()
      return (await locator.count()) > 0 ? locator.boundingBox() : null
    },
    parts: () => page.evaluate(paintedParts)
  }
}

/** The React client served from `dist` in `browser`. */
export function reactClient(opts: { browser: Browser; dist: string }): ClientLauncher {
  const dist = path.resolve(opts.dist)
  if (!existsSync(path.join(dist, 'index.html'))) {
    throw new Error(`${dist}/index.html is missing: run bun run build:web`)
  }
  return {
    client: 'react',
    resolvesTargets: true,
    async session({ world, surface, deviceId, points }, body) {
      const phone = surface.profile === 'phone'
      const context = await opts.browser.newContext({
        viewport: phone ? PHONE_VIEWPORT : VIEWPORT,
        isMobile: phone,
        hasTouch: phone,
        // Copy buttons write the clipboard; headless Chromium leaves that promise pending without the grant.
        permissions: ['clipboard-read', 'clipboard-write'],
        deviceScaleFactor: 1,
        colorScheme: 'dark',
        reducedMotion: 'reduce',
        serviceWorkers: 'block'
      })
      try {
        await context.route(`${world.server.baseUrl}/**`, async (route) => {
          const request = route.request()
          const url = new URL(request.url())
          const file = path.join(dist, url.pathname === '/' ? 'index.html' : url.pathname)
          if (
            request.method() === 'GET' &&
            file.startsWith(dist) &&
            existsSync(file) &&
            !file.endsWith(path.sep)
          ) {
            await route.fulfill({ path: file })
            return
          }
          await route.continue()
        })
        await context.clock.setFixedTime(world.clock)
        await context.addInitScript(
          ({ token, deviceId }) => {
            localStorage.setItem('termul.webAuthToken', token)
            localStorage.setItem('termul.deviceId', deviceId)
            // RENAME_NOTICE_STORAGE_KEY in src/renderer/lib/rename-notice.ts: a fresh device sees the notice.
            localStorage.setItem('termul.renameNotice', 'dismissed')
          },
          { token: TOKEN, deviceId }
        )
        const page = await context.newPage()
        const errors: string[] = []
        page.on('pageerror', (error) => errors.push(error.message))
        await page.goto(`${world.server.baseUrl}/${surface.start?.hash ?? ''}`)
        await page
          .getByText(surface.start?.ready ?? BOOT_TEXT)
          .first()
          .waitFor({ state: 'visible', timeout: READY_TIMEOUT_MS })
        const result = await body(pageDriver(page, points, world.root, phone))
        if (errors.length > 0)
          console.error(`[parity] ${surface.id}: page errors: ${errors.slice(0, 3).join(' | ')}`)
        return result
      } finally {
        await context.close()
      }
    }
  }
}
