/**
 * The `rune-web` client: a Rune wasm build in Playwright Chromium.
 *
 * Rune draws into a canvas, so the DOM holds no controls. It does hold Rune's
 * accessibility mirror (`[data-rune-a11y]`): transparent buttons, links,
 * headings, images and text placed over what Rune drew, at the rects Rune
 * laid out. The driver finds a target there by its text or name, acts at that
 * point through Playwright's mouse and keyboard, reads checks from the
 * mirror's text, and grabs frames with `page.screenshot`. A target the mirror
 * cannot name (a test id, a CSS selector) is acted on at the point the
 * reference recorded.
 *
 * Rune paints first on a CPU canvas and hands over to WebGPU when the page
 * has an adapter. Headless Chromium here has none unless
 * `TROUPE_RUNE_WEB_GPU=1` adds `--enable-unsafe-webgpu` (SwiftShader), and
 * a headless screenshot of that WebGPU canvas came out blank, so the CPU
 * canvas is the default. Every session records which renderer drew.
 */

import type { Browser, Page } from 'playwright'
import type { PartRects, Rect } from '../diff'
import type { ActionLog, Target } from '../types'
import { TOKEN } from '../world'
import {
  type ClientLauncher,
  type Driver,
  describe,
  errorLine,
  type GallerySession,
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
import { BOOT_TEXT, PHONE_VIEWPORT, playwrightKeys, VIEWPORT } from './react'
import {
  detectRenderer,
  type RuneRenderer,
  runeWebDist,
  type StaticServer,
  serveRuneClient,
  serveStatic,
  WEBGPU_ARGS
} from './rune-apps'

const READY_TIMEOUT_MS = 60_000
/** Rune asks for the GPU 250 ms after its first CPU frame and gives up after 3 s. */
const HANDOVER_TIMEOUT_MS = 4_000
const SETTLE_BEFORE_GRAB_MS = 300

/** `TROUPE_RUNE_WEB_GPU=1`: offer WebGPU, so Rune hands over from the CPU canvas. */
export const runeWebGpu = () => process.env.TROUPE_RUNE_WEB_GPU === '1'

/** Launch flags for a Chromium that runs a Rune wasm build. */
export const runeWebArgs = (args: string[]) => (runeWebGpu() ? [...args, ...WEBGPU_ARGS] : args)

/**
 * Where the mirror shows `want`: the centre of the smallest visible mirror
 * element whose text (or accessible name) is `want`, or contains it when
 * `exact` is false, with its centre inside `within` when given. Null while
 * no element matches.
 */
export function mirrorPoint(
  page: Page,
  want: string,
  exact: boolean,
  within?: Rect
): Promise<Point | null> {
  return page.evaluate(
    ({ want, exact, within }) => {
      const flat = (s: string | null) => (s ?? '').replace(/\s+/g, ' ').trim()
      const root = document.querySelector('[data-rune-a11y]')
      if (!root) return null
      let best: { area: number; x: number; y: number } | null = null
      for (const el of root.querySelectorAll('*')) {
        const names = [flat(el.textContent), flat(el.getAttribute('aria-label'))]
        if (!names.some((n) => (exact ? n === want : n.includes(want)))) continue
        const r = el.getBoundingClientRect()
        if (r.width <= 0 || r.height <= 0) continue
        const [cx, cy] = [r.x + r.width / 2, r.y + r.height / 2]
        if (
          within &&
          (cx < within.x ||
            cy < within.y ||
            cx > within.x + within.width ||
            cy > within.y + within.height)
        )
          continue
        const area = r.width * r.height
        if (!best || area < best.area) best = { area, x: r.x + r.width / 2, y: r.y + r.height / 2 }
      }
      return best && { x: best.x, y: best.y }
    },
    { want: normalize(want), exact, within }
  )
}

/** Every text the mirror holds, one line per element, in document order. */
const mirrorText = (page: Page) =>
  page.evaluate(() => {
    const root = document.querySelector('[data-rune-a11y]')
    if (!root) return ''
    return [...root.querySelectorAll('*')]
      .filter((el) => el.children.length === 0)
      .map((el) => el.textContent ?? '')
      .join('\n')
  })

/**
 * Text and icon boxes from the mirror: its text items and its images. A
 * button's item is the whole control, so it counts as neither.
 */
function mirrorParts(page: Page): Promise<PartRects> {
  return page.evaluate(() => {
    const box = (r: DOMRect) => ({ x: r.x, y: r.y, width: r.width, height: r.height })
    const text: PartRects['text'] = []
    const icons: PartRects['icons'] = []
    const root = document.querySelector('[data-rune-a11y]')
    for (const el of root?.querySelectorAll('*') ?? []) {
      const r = el.getBoundingClientRect()
      if (r.width <= 0 || r.height <= 0) continue
      if (el.getAttribute('role') === 'img') icons.push(box(r))
      else if (/^(P|H[1-6]|A|SPAN)$/.test(el.tagName) && el.textContent?.trim()) text.push(box(r))
    }
    return { text, icons }
  })
}

/** A driver over a Rune page. `touch`: a plain click is a tap and no pointer is parked. */
export function runeWebDriver(page: Page, points: Points, touch = false): Driver {
  const pointOf = async (
    key: string,
    target: Target
  ): Promise<{ point: Point; via: ActionLog['via'] }> => {
    if ('point' in target) return { point: target.point, via: 'point' }
    const named = 'text' in target ? target.text : 'label' in target ? target.label : null
    if (named === null) {
      const recorded = points.get(key)
      if (!recorded)
        throw new Error('the mirror names no test id or selector, and no client recorded a point')
      return { point: recorded, via: 'recorded-point' }
    }
    const exact = 'text' in target ? (target.exact ?? true) : true
    let found: Point | null = null
    const shown = await poll(async () => {
      found = await mirrorPoint(page, named, exact)
      return found !== null
    }, PRESENT_TIMEOUT_MS)
    if (!shown || !found) throw new Error(`the mirror never showed "${named}"`)
    points.set(key, found)
    return { point: found, via: 'text' }
  }
  return {
    async act(step, index, action) {
      const key = `${step}/${index}`
      const log: ActionLog = { step, action: describe(action), via: 'none', ok: true }
      try {
        switch (action.do) {
          case 'click': {
            const { point, via } = await pointOf(key, action.target)
            log.via = via
            if (touch && !action.button && !action.modifiers) {
              await page.touchscreen.tap(point.x, point.y)
              break
            }
            if (action.modifiers) await page.keyboard.down('Control')
            await page.mouse.click(point.x, point.y, { button: action.button })
            if (action.modifiers) await page.keyboard.up('Control')
            break
          }
          case 'dblclick': {
            const { point, via } = await pointOf(key, action.target)
            log.via = via
            await page.mouse.dblclick(point.x, point.y)
            break
          }
          case 'hover': {
            const { point, via } = await pointOf(key, action.target)
            log.via = via
            await page.mouse.move(point.x, point.y)
            break
          }
          case 'wheel': {
            const { point, via } = await pointOf(key, action.target)
            log.via = via
            await page.mouse.move(point.x, point.y)
            await page.mouse.wheel(0, action.deltaY)
            break
          }
          case 'drag': {
            const from = await pointOf(key, action.from)
            const to = await pointOf(`${key}/to`, action.to)
            log.via = from.via
            await page.mouse.move(from.point.x, from.point.y)
            await page.mouse.down()
            await page.mouse.move(from.point.x + 12, from.point.y + 4, { steps: 4 })
            await page.mouse.move(to.point.x, to.point.y, { steps: 16 })
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
            await page.waitForTimeout(action.ms)
            break
          case 'choose':
            throw new Error('Rune has no file dialog on the web')
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
    // The mirror carries no test ids, so `within` cannot scope a check: it reads the whole mirror.
    async check(step, check) {
      if (check.order) {
        const order = check.order
        return {
          step,
          text: order.join(' < '),
          expect: 'present',
          pass: await poll(async () => ordered(await mirrorText(page), order), PRESENT_TIMEOUT_MS)
        }
      }
      const want = normalize(check.text)
      const painted = async () => normalize(await mirrorText(page)).includes(want)
      const pass = check.gone
        ? await poll(async () => !(await painted()), check.timeoutMs ?? GONE_TIMEOUT_MS)
        : await poll(painted, check.timeoutMs ?? PRESENT_TIMEOUT_MS)
      return { step, text: check.text, expect: check.gone ? 'gone' : 'present', pass }
    },
    async grab(file) {
      await page.evaluate(
        () => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))
      )
      await page.waitForTimeout(SETTLE_BEFORE_GRAB_MS)
      await page.screenshot({ path: file })
    },
    parts: () => mirrorParts(page)
  }
}

export interface RuneWebSession {
  page: Page
  renderer: RuneRenderer
  driver: Driver
  close(): Promise<void>
}

export interface RuneWebOptions {
  browser: Browser
  url: string
  /** A text the mirror shows once the app's first frame is up. */
  ready: string
  viewport?: { width: number; height: number }
  touch?: boolean
  points?: Points
  /** Fixed page clock, as the reference capture sets it. */
  clock?: number
  /** localStorage entries written before the page loads. */
  storage?: Record<string, string>
}

/** Opens `url` in a fresh context of `browser`, waits for `ready` in the mirror and for the GPU hand-over. */
export async function openRuneWeb(opts: RuneWebOptions): Promise<RuneWebSession> {
  const touch = opts.touch ?? false
  const context = await opts.browser.newContext({
    viewport: opts.viewport ?? VIEWPORT,
    isMobile: touch,
    hasTouch: touch,
    permissions: ['clipboard-read', 'clipboard-write'],
    deviceScaleFactor: 1,
    colorScheme: 'dark',
    reducedMotion: 'reduce',
    serviceWorkers: 'block'
  })
  try {
    if (opts.clock !== undefined) await context.clock.setFixedTime(opts.clock)
    if (opts.storage) {
      await context.addInitScript((entries) => {
        for (const [k, v] of Object.entries(entries)) localStorage.setItem(k, v)
      }, opts.storage)
    }
    const page = await context.newPage()
    const notes: string[] = []
    page.on('pageerror', (error) => notes.push(error.message))
    page.on('console', (m) => {
      if (m.type() === 'error' || m.text().startsWith('rune:')) notes.push(m.text())
    })
    await page.goto(opts.url)
    const up = await poll(
      async () => (await mirrorPoint(page, opts.ready, true)) !== null,
      READY_TIMEOUT_MS
    )
    if (!up) {
      throw new Error(
        `the mirror never showed "${opts.ready}"${notes.length ? `: ${notes.slice(0, 2).join(' | ')}` : ''}`
      )
    }
    let renderer = await detectRenderer(page)
    if (runeWebGpu() && renderer === 'cpu') {
      await poll(async () => {
        renderer = await detectRenderer(page)
        return renderer !== 'cpu'
      }, HANDOVER_TIMEOUT_MS)
    }
    console.error(`[rune-web] ${opts.url} drew with ${renderer}`)
    return {
      page,
      renderer,
      driver: runeWebDriver(page, opts.points ?? new Map(), touch),
      close: () => context.close()
    }
  } catch (error) {
    await context.close()
    throw error
  }
}

/**
 * The matrix's `rune-web` client: the Rune web client's build (`troupe-web`)
 * served on the seeded server's origin through `serveRuneClient`, signed in
 * with the React client's `localStorage` keys. One server serves every session
 * of a run. When the Troupe server serves the build itself, open it there.
 */
export function runeWebClient(opts: { browser: Browser }): ClientLauncher {
  let served: Promise<StaticServer> | null = null
  return {
    client: 'rune-web',
    resolvesTargets: true,
    async session({ world, surface, deviceId, points }, body) {
      served ??= serveRuneClient(runeWebDist('troupe'), world.server.baseUrl)
      const phone = surface.profile === 'phone'
      const session = await openRuneWeb({
        browser: opts.browser,
        url: `${(await served).url}/${surface.start?.hash ?? ''}`,
        ready: surface.start?.ready ?? BOOT_TEXT,
        viewport: phone ? PHONE_VIEWPORT : VIEWPORT,
        touch: phone,
        points,
        clock: world.clock,
        storage: { 'termul.webAuthToken': TOKEN, 'termul.deviceId': deviceId }
      })
      try {
        return await body(session.driver)
      } finally {
        await session.close()
      }
    }
  }
}

/**
 * The gallery's layout (`apps/rune/crates/gallery/src/lib.rs`): a 260 px
 * scene list, then the scene's frame inside 24 px of padding. The window is
 * wide and tall enough that the list needs no scrolling and every frame fits.
 */
const GALLERY_VIEWPORT = { width: 1600, height: 2400 }
const GALLERY_FRAME = { x: 260 + 24, y: 24 }
const GALLERY_LIST: Rect = { x: 0, y: 0, width: 260, height: GALLERY_VIEWPORT.height }

/** The Rune gallery's wasm build, picking each scene by its name in the scene list. */
export async function runeWebGallery(browser: Browser): Promise<GallerySession> {
  const server = await serveStatic(runeWebDist('gallery'))
  try {
    const session = await openRuneWeb({
      browser,
      url: `${server.url}/`,
      ready: 'Surface tokens',
      viewport: GALLERY_VIEWPORT
    })
    const { page } = session
    return {
      renderer: session.renderer,
      async shoot(entry, file) {
        const point = await mirrorPoint(page, entry.name, true, GALLERY_LIST)
        if (!point) throw new Error(`the scene list has no "${entry.name}"`)
        await page.mouse.click(point.x, point.y)
        await page.mouse.move(GALLERY_VIEWPORT.width - 1, GALLERY_VIEWPORT.height - 1)
        await page.evaluate(
          () =>
            new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))
        )
        await page.waitForTimeout(SETTLE_BEFORE_GRAB_MS)
        const clip = { ...GALLERY_FRAME, width: entry.width, height: entry.height }
        await page.screenshot({ path: file, clip })
        const parts = await mirrorParts(page)
        const inFrame = (list: PartRects['text']) =>
          list
            .map((r) => ({ ...r, x: r.x - clip.x, y: r.y - clip.y }))
            .filter(
              (r) =>
                r.x + r.width > 0 && r.y + r.height > 0 && r.x < clip.width && r.y < clip.height
            )
        return { via: 'text', parts: { text: inFrame(parts.text), icons: inFrame(parts.icons) } }
      },
      async close() {
        await session.close()
        await server.close()
      }
    }
  } catch (error) {
    await server.close()
    throw error
  }
}
