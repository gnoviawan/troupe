/**
 * What a two-client lane shares: a client that finds text, clicks it and types into the
 * composer in the React client (Playwright locators) or the Rune web client (its
 * accessibility mirror), the frame capture, the React and Rune harnesses, and the
 * `pair-<step>.png` images (React left, Rune right).
 */

import { existsSync, readdirSync } from 'node:fs'
import path from 'node:path'
import type { Browser, Page } from 'playwright'
import { PHONE_VIEWPORT, VIEWPORT } from '../drivers/react'
import { runeWebDist, serveRuneClient } from '../drivers/rune-apps'
import { mirrorPoint, openRuneWeb } from '../drivers/rune-web'
import { blankImage, blit, readPng, writePng } from '../png'
import { prepareDevice, TOKEN, type World } from '../world'

export interface Client {
  name: 'react' | 'rune'
  page: Page
  /** Clicks the smallest element that shows `text` (or names it), waiting up to 20 s. */
  click(text: string, exact?: boolean): Promise<void>
  /** True once `text` shows, waiting up to `ms`. */
  sees(text: string, ms?: number): Promise<boolean>
  send(prompt: string): Promise<void>
  /** Types `text` into the field labelled `label`. */
  fill(label: string, text: string): Promise<void>
}

export interface LaneOptions {
  root: string
  phone: boolean
  /** React's `dist-web`. */
  dist: string
  device: string
}

export async function until(check: () => Promise<boolean>, ms: number): Promise<boolean> {
  for (const end = Date.now() + ms; Date.now() < end; await Bun.sleep(200)) {
    if (await check()) return true
  }
  return false
}

export async function shot(client: Client, root: string, step: string): Promise<void> {
  await client.page.waitForTimeout(600)
  await client.page.evaluate(
    () => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))
  )
  await client.page.screenshot({ path: path.join(root, `${client.name}-${step}.png`) })
}

function reactClient(page: Page, phone: boolean): Client {
  return {
    name: 'react',
    page,
    async click(text, exact = true) {
      const target = page
        .getByRole('button', { name: text, exact })
        .or(page.getByText(text, { exact }))
        .first()
      await target.click({ timeout: 20_000 })
    },
    sees: (text, ms = 45_000) => until(async () => (await page.getByText(text).count()) > 0, ms),
    async send(prompt) {
      const editor = page.locator('[contenteditable="true"]').first()
      await editor.click({ timeout: 20_000 })
      await page.keyboard.type(prompt, { delay: 15 })
      await page.waitForTimeout(300)
      // A phone's Return breaks the line; its Send button sends.
      if (phone)
        await page
          .getByRole('button', { name: /^(Send message|Start agent chat)$/ })
          .first()
          .click({ timeout: 20_000 })
      else await page.keyboard.press('Enter')
    },
    async fill(label, text) {
      // A field's name also carries its description (`Runs Whole runs of the suite.`).
      await page.getByLabel(label).first().fill(text, { timeout: 20_000 })
    }
  }
}

function runeClient(page: Page, phone: boolean): Client {
  const at = async (text: string, exact: boolean) => {
    let point = null
    await until(async () => {
      point = await mirrorPoint(page, text, exact)
      return point !== null
    }, 20_000)
    if (!point) throw new Error(`the Rune mirror never showed "${text}"`)
    return point as { x: number; y: number }
  }
  return {
    name: 'rune',
    page,
    async click(text, exact = true) {
      const { x, y } = await at(text, exact)
      await page.mouse.click(x, y)
    },
    sees: (text, ms = 45_000) =>
      until(async () => (await mirrorPoint(page, text, false)) !== null, ms),
    async send(prompt) {
      const { x, y } = await at('Message', false)
      await page.mouse.click(x, y)
      await page.keyboard.type(prompt, { delay: 15 })
      await page.waitForTimeout(300)
      if (phone) await this.click('Send message')
      else await page.keyboard.press('Enter')
    },
    async fill(label, text) {
      // The text field named `label`, not the caption above it that shows the same words.
      const field = await page.evaluate((want) => {
        const root = document.querySelector('[data-rune-a11y]')
        const fields =
          root?.querySelectorAll('input, textarea, [role="textbox"], [contenteditable="true"]') ??
          []
        for (const el of fields) {
          if ((el.getAttribute('aria-label') ?? '').trim() !== want) continue
          const r = el.getBoundingClientRect()
          if (r.width > 0 && r.height > 0) return { x: r.x + r.width / 2, y: r.y + r.height / 2 }
        }
        return null
      }, label)
      const { x, y } = field ?? (await at(label, true))
      await page.mouse.click(x, y)
      await page.keyboard.type(text, { delay: 15 })
    }
  }
}

/** Runs `steps` in the React client served from `dist` against the world's server. */
export async function inReact<T>(
  browser: Browser,
  world: World,
  o: LaneOptions,
  steps: (client: Client) => Promise<T>
): Promise<T> {
  const device = `${o.device}-react`
  await prepareDevice(world, device)
  const context = await browser.newContext({
    viewport: o.phone ? PHONE_VIEWPORT : VIEWPORT,
    isMobile: o.phone,
    hasTouch: o.phone,
    colorScheme: 'dark',
    reducedMotion: 'reduce',
    serviceWorkers: 'block'
  })
  await context.route(`${world.server.baseUrl}/**`, async (route) => {
    const url = new URL(route.request().url())
    const file = path.join(o.dist, url.pathname === '/' ? 'index.html' : url.pathname)
    const local = file.startsWith(o.dist) && (await Bun.file(file).exists())
    if (route.request().method() === 'GET' && local) await route.fulfill({ path: file })
    else await route.continue()
  })
  await context.addInitScript(
    ({ token, device }) => {
      localStorage.setItem('termul.webAuthToken', token)
      localStorage.setItem('termul.deviceId', device)
      localStorage.setItem('termul.renameNotice', 'dismissed')
    },
    { token: TOKEN, device }
  )
  const page = await context.newPage()
  await page.goto(`${world.server.baseUrl}/`)
  await page.getByText('What should we do in alpha?').first().waitFor({ timeout: 60_000 })
  try {
    return await steps(reactClient(page, o.phone))
  } finally {
    await context.close()
  }
}

/** Runs `steps` in the Rune web client's wasm build against the world's server. */
export async function inRune<T>(
  browser: Browser,
  world: World,
  o: LaneOptions,
  steps: (client: Client) => Promise<T>
): Promise<T> {
  const device = `${o.device}-rune`
  await prepareDevice(world, device)
  const served = await serveRuneClient(runeWebDist('troupe'), world.server.baseUrl)
  try {
    const session = await openRuneWeb({
      browser,
      url: `${served.url}/`,
      ready: 'What should we do in alpha?',
      viewport: o.phone ? PHONE_VIEWPORT : VIEWPORT,
      touch: o.phone,
      storage: { 'termul.webAuthToken': TOKEN, 'termul.deviceId': device }
    })
    const lines: string[] = []
    session.page.on('console', (m) => lines.push(`${m.type()} ${m.text()}`))
    try {
      return await steps(runeClient(session.page, o.phone))
    } finally {
      await Bun.write(path.join(o.root, 'rune-console.log'), lines.join('\n'))
      await session.close()
    }
  } finally {
    await served.close()
  }
}

/** `pair-<step>.png`: React's frame on the left, Rune's on the right, for each step both saved. */
export function writePairs(root: string): void {
  for (const file of readdirSync(root).filter((f) => f.startsWith('react-'))) {
    const other = path.join(root, file.replace(/^react-/, 'rune-'))
    if (!existsSync(other)) continue
    const [a, b] = [readPng(path.join(root, file)), readPng(other)]
    const out = blankImage(a.width + 16 + b.width, Math.max(a.height, b.height), 255)
    blit(a, out, 0, 0, a.width, a.height, 0, 0)
    blit(b, out, 0, 0, b.width, b.height, a.width + 16, 0)
    writePng(path.join(root, file.replace(/^react-/, 'pair-')), out)
  }
}
