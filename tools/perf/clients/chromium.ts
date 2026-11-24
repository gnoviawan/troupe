import { existsSync, statSync } from 'node:fs'
import path from 'node:path'
import { type Browser, chromium, type Locator, type Page } from 'playwright'
import { createCollector, type PerfCollector } from '../metrics/cdp-collector.ts'
import { LAUNCHER_COMPOSER, START_CHAT_SELECTOR, waitForLauncher } from '../runner/scenario-ui.ts'
import { descendants, readCmdline } from '../sampling/proc-sampler.ts'
import { PERF_AGENT_NAME, PERF_DEVICE_ID, PERF_PROJECT_NAME, PERF_TOKEN } from '../server.ts'
import type { ChatStartPart, ClientDriver, ClientHandle, ClientLaunchOptions } from './client.ts'

export interface ChromiumHandle extends ClientHandle {
  page: Page
  collector: PerfCollector
}

/**
 * Headed Chromium (Playwright's bundled build) on the run's X display. The
 * test server's compiled-in asset dir belongs to the checkout it was built
 * from, so GETs for files that exist in this worktree's dist-web are
 * answered from disk instead. Service workers are blocked because sw.js
 * fetches skip routing.
 */
export async function launchChromium(opts: ClientLaunchOptions): Promise<ChromiumHandle> {
  const t0 = performance.now()
  const lavapipe = opts.gl === 'lavapipe'
  const cpu = opts.gl === 'cpu'
  const browser: Browser = await chromium.launch({
    headless: false,
    env: {
      ...process.env,
      DISPLAY: opts.display,
      ...(lavapipe ? { VK_ICD_FILENAMES: '/usr/share/vulkan/icd.d/lvp_icd.json' } : {})
    },
    args: [
      '--disable-features=LocalNetworkAccessChecks,PrivateNetworkAccessChecks',
      ...(lavapipe
        ? [
            '--use-angle=vulkan',
            '--enable-features=Vulkan,DefaultANGLEVulkan,VulkanFromANGLE',
            '--ignore-gpu-blocklist'
          ]
        : []),
      ...(cpu ? ['--disable-gpu'] : []),
      `--window-size=${opts.width},${opts.height}`,
      '--window-position=0,0'
    ]
  })
  const rootPid = browserPid()
  const context = await browser.newContext({
    viewport: null,
    colorScheme: 'dark',
    serviceWorkers: 'block'
  })
  await context.addInitScript(
    ({ token, deviceId }) => {
      localStorage.setItem('termul.webAuthToken', token)
      localStorage.setItem('termul.deviceId', deviceId)
    },
    { token: PERF_TOKEN, deviceId: PERF_DEVICE_ID }
  )
  await context.route(`${opts.url}/**`, async (route) => {
    const req = route.request()
    if (req.method() !== 'GET') return route.continue()
    const pathname = new URL(req.url()).pathname
    const rel = pathname === '/' ? 'index.html' : decodeURIComponent(pathname.slice(1))
    const file = path.join(opts.distWeb, rel)
    if (!file.startsWith(opts.distWeb) || !existsSync(file) || !statSync(file).isFile()) {
      return route.continue()
    }
    return route.fulfill({ path: file })
  })
  const page = await context.newPage()
  const collector = await createCollector(page)
  if (!opts.noPageMetrics) await collector.inject()
  const launchedMs = performance.now() - t0
  await page.goto(opts.url, { waitUntil: 'domcontentloaded', timeout: 30_000 })
  const backend = await page.evaluate(() => {
    const gl = document.createElement('canvas').getContext('webgl2')
    const ext = gl?.getExtension('WEBGL_debug_renderer_info')
    return gl && ext ? String(gl.getParameter(ext.UNMASKED_RENDERER_WEBGL)) : 'no webgl2'
  })
  return {
    kind: 'chromium',
    rootPid,
    launchedMs,
    version: `Chromium ${browser.version()}`,
    backend,
    page,
    collector,
    close: async () => {
      collector.stop()
      await browser.close().catch(() => undefined)
    }
  }
}

/** Playwright spawns Chromium as a direct child of this Node process. */
export function browserPid(): number {
  for (const pid of descendants(process.pid)) {
    if (pid === process.pid) continue
    const cmd = readCmdline(pid)
    // Match the executable only: other children (the server) carry run paths in their args.
    const exe = path.basename(cmd.split(' ')[0] ?? '')
    if (/chrom/i.test(exe) && !cmd.includes('--type=')) return pid
  }
  throw new Error('could not find the launched Chromium root process')
}

/** Generous: the page's main thread is saturated during the storm, and an
 * action that waits is part of what is measured, not a harness failure. */
const ACTION_MS = 30_000
const CHAT_TAB = '[data-chat-tab-state]'
const PROJECT_ROW = `[aria-label^="Project: ${PERF_PROJECT_NAME}"]`
const VISIBLE_TERMINAL = '.xterm:visible'
const SHOW_CHATS = `button[aria-label="Show chats in ${PERF_PROJECT_NAME}"]`
const RAIL_NEW_CHAT = `button[aria-label="New chat in ${PERF_PROJECT_NAME}"]`

/**
 * The launcher's editor can remount while agent config loads, which drops
 * the first click's focus. Type, read back, and retry until the text stuck.
 */
async function typeIntoComposer(page: Page, composer: Locator, text: string): Promise<void> {
  for (let attempt = 0; attempt < 5; attempt++) {
    await composer.click({ timeout: ACTION_MS })
    await page.keyboard.press('Control+A')
    await page.keyboard.press('Backspace')
    await composer.pressSequentially(text, { delay: 5, timeout: ACTION_MS })
    if ((await composer.innerText()).trim() === text) return
    await new Promise((r) => setTimeout(r, 1000))
  }
  throw new Error(`launcher composer never accepted "${text}"`)
}

export function chromiumDriver(handle: ChromiumHandle): ClientDriver {
  const { page, collector } = handle
  const titles: string[] = []
  const parts: ChatStartPart[] = []
  /** Chat rows in the rail, minus each row's "More actions" button. The perf
   * world seeds one project, so the open-project list is the only list. */
  const chatRows = () =>
    page.locator(
      '[data-testid="rail-project-chats"] button[aria-label]:not([aria-label^="More actions"])'
    )

  const ensureChatsExpanded = async () => {
    const toggle = page.locator(SHOW_CHATS).first()
    if (await toggle.isVisible().catch(() => false)) {
      await toggle.click({ timeout: ACTION_MS }).catch(() => undefined)
    }
  }
  return {
    handle,
    waitForProjectList: () =>
      page.locator(PROJECT_ROW).first().waitFor({ state: 'visible', timeout: 60_000 }),
    waitForLauncher: () => waitForLauncher(page),
    startChat: async (prompt) => {
      const composer = page.locator(LAUNCHER_COMPOSER).first()
      if (!(await composer.isVisible())) {
        // The rail project row's "+" shows only while the row is hovered;
        // hover the row first, then fall back to the pane toolbar's
        // New tab → New chat menu when it never appears.
        const railNew = page.locator(RAIL_NEW_CHAT).first()
        await page
          .locator(PROJECT_ROW)
          .first()
          .hover({ timeout: ACTION_MS })
          .catch(() => undefined)
        if (await railNew.isVisible().catch(() => false)) {
          await railNew.click({ timeout: ACTION_MS })
        } else {
          await page.locator('button[aria-label="New tab"]').first().click({ timeout: ACTION_MS })
          await page
            .locator('[role=menuitem]:has-text("New chat")')
            .first()
            .click({ timeout: ACTION_MS })
        }
        await composer.waitFor({ timeout: ACTION_MS })
      }
      await page
        .locator(`button[aria-label$="Currently ${PERF_AGENT_NAME}"]:visible`)
        .first()
        .waitFor({ timeout: ACTION_MS })
      const before = await page.locator(CHAT_TAB).count()
      const typed = performance.now()
      await typeIntoComposer(page, composer, prompt)
      const clicked = performance.now()
      await page.locator(START_CHAT_SELECTOR).first().click({ timeout: ACTION_MS })
      await page.waitForFunction(
        ([sel, n]) => document.querySelectorAll(sel as string).length > (n as number),
        [CHAT_TAB, before] as const,
        { timeout: ACTION_MS }
      )
      await composer.waitFor({ state: 'hidden', timeout: ACTION_MS })
      parts.push({
        typeMs: Math.round(clicked - typed),
        openMs: Math.round(performance.now() - clicked)
      })
      titles.push(prompt)
    },
    chatStartParts: () => parts,
    crashed: async () => null,
    chatCount: () => page.locator(CHAT_TAB).count(),
    switchToChat: async (index) => {
      const title = titles[index % titles.length]
      // A rail chat row's aria-label starts with the chat title.
      const byTitle = page.locator(
        `[data-testid="rail-project-chats"] button[aria-label^=${JSON.stringify(title)}]`
      )
      // The rail collapses a project's chat list behind a chevron; open it once.
      if (
        (await byTitle.count()) === 0 ||
        !(await byTitle
          .first()
          .isVisible()
          .catch(() => false))
      ) {
        await ensureChatsExpanded()
      }
      if ((await byTitle.count()) > 0) {
        await byTitle.first().click({ timeout: ACTION_MS })
        return
      }
      // Fallback: the nth row of the open project's list.
      const count = await chatRows().count()
      if (count === 0) throw new Error('switchToChat: the rail lists no chats')
      await chatRows()
        .nth(index % count)
        .click({ timeout: ACTION_MS })
    },
    openTerminal: async () => {
      const before = await page.locator('.xterm').count()
      await page.locator('button[aria-label="New tab"]').first().click({ timeout: ACTION_MS })
      await page
        .locator('[role=menuitem]:has-text("New terminal"):not(:has-text("with"))')
        .first()
        .click({ timeout: ACTION_MS })
      await page.waitForFunction((n) => document.querySelectorAll('.xterm').length > n, before, {
        timeout: ACTION_MS
      })
      await page.locator(`${VISIBLE_TERMINAL} .xterm-helper-textarea`).last().focus()
    },
    runInTerminal: async (command) => {
      await page.locator(`${VISIBLE_TERMINAL} .xterm-helper-textarea`).last().focus()
      await page.keyboard.type(command)
      await page.keyboard.press('Enter')
    },
    waitForTerminalText: async () => {
      throw new Error('xterm draws to canvas; use the terminal probe result file instead')
    },
    beginPhase: async (name) => {
      // Discard what accrued between phases so the page window matches the
      // runner's proc and byte windows.
      await collector.drainPhase()
      await collector.markPhaseStart(name)
    },
    endPhase: async (name) => {
      await collector.markPhaseEnd(name)
      return (await collector.drainPhase()).page
    }
  }
}
