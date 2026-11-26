import { type ChildProcess, spawn } from 'node:child_process'
import { existsSync, mkdtempSync, rmSync } from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { setTimeout as sleep } from 'node:timers/promises'
import { chromium, type Page } from 'playwright'
import {
  detectRenderer,
  RUNE_APPS,
  type RuneAppName,
  runeBinary,
  runeWebDist,
  serveStatic,
  WEBGPU_ARGS
} from '../../parity/drivers/rune-apps.ts'
import {
  openRuneSocket,
  type RuneElement,
  type RuneSocket
} from '../../parity/drivers/rune-socket.ts'
import { createCollector, type PerfCollector } from '../metrics/cdp-collector.ts'
import type { RoleOf } from '../sampling/proc-sampler.ts'
import { browserPid } from './chromium.ts'
import type { ClientDriver, ClientHandle, ClientLaunchOptions, ClientTreeCensus } from './client.ts'

/**
 * The Rune clients of the Linux lane. `rune-web` is a Rune wasm build in
 * headed Chromium on the run's display, served from a static folder;
 * `rune-desktop` is the native build as a real window on that display,
 * driven through the live driver socket. Rune has no Troupe screen until R5,
 * so both open a Rune app (`--rune-app`: `hello` by default, or the gallery
 * on the web, whose native build has no live driver): the
 * `launch` and `idle` scenarios run, and the chat and terminal actions fail
 * with the unit that brings them. The page and the byte proxy see no Troupe
 * traffic, since these apps never talk to the server. For a Rune app,
 * `startup.projectListMs` is its first painted frame and
 * `startup.launcherReadyMs` is the renderer settling (the web build's
 * hand-over from its CPU canvas to WebGPU, when Chromium offers an adapter).
 */

const READY_MS = 60_000
const LAVAPIPE = '/usr/share/vulkan/icd.d/lvp_icd.json'

/** The rest of the scenario surface waits on Rune's Troupe screens. */
const notYet = (what: string, unit: string) => async (): Promise<never> => {
  throw new Error(`the Rune app has no ${what} until ${unit}`)
}

function runeOnlyDriver(handle: ClientHandle) {
  return {
    handle,
    startChat: notYet('chat', 'R6'),
    chatCount: notYet('chat', 'R6'),
    switchToChat: notYet('chat', 'R6'),
    openTerminal: notYet('terminal', 'R7'),
    runInTerminal: notYet('terminal', 'R7'),
    waitForTerminalText: notYet('terminal', 'R7'),
    chatStartParts: () => [],
    crashed: async () => null
  }
}

export interface RuneWebHandle extends ClientHandle {
  page: Page
  collector: PerfCollector
  /** Chromium was offered a WebGPU adapter, so Rune may hand over from its CPU canvas. */
  webgpu: boolean
}

/** Rune asks for the GPU 250 ms after its first CPU frame and gives up after 3 s. */
const HANDOVER_MS = 3500

/** Whether the mirror (`[data-rune-a11y]`) shows `text`. */
const mirrorShows = (page: Page, text: string) =>
  page.evaluate(
    (want) =>
      [...(document.querySelector('[data-rune-a11y]')?.querySelectorAll('*') ?? [])].some(
        (el) => el.textContent?.trim() === want
      ),
    text
  )

async function until(fn: () => Promise<boolean>, ms: number, what: string): Promise<void> {
  const deadline = Date.now() + ms
  while (!(await fn().catch(() => false))) {
    if (Date.now() > deadline) throw new Error(`${what} after ${ms} ms`)
    await sleep(150)
  }
}

export async function launchRuneWeb(opts: ClientLaunchOptions): Promise<RuneWebHandle> {
  const app = opts.runeApp
  const server = await serveStatic(runeWebDist(app))
  const t0 = performance.now()
  const lavapipe = opts.gl === 'lavapipe'
  const browser = await chromium.launch({
    headless: false,
    env: {
      ...process.env,
      DISPLAY: opts.display,
      ...(lavapipe ? { VK_ICD_FILENAMES: LAVAPIPE } : {})
    },
    args: [
      ...(lavapipe
        ? [
            '--use-angle=vulkan',
            '--enable-features=Vulkan,DefaultANGLEVulkan,VulkanFromANGLE',
            '--ignore-gpu-blocklist',
            ...WEBGPU_ARGS
          ]
        : []),
      ...(opts.gl === 'cpu' ? ['--disable-gpu'] : []),
      `--window-size=${opts.width},${opts.height}`,
      '--window-position=0,0'
    ]
  })
  const rootPid = browserPid()
  const context = await browser.newContext({ viewport: null, colorScheme: 'dark' })
  const page = await context.newPage()
  const collector = await createCollector(page)
  if (!opts.noPageMetrics) await collector.inject()
  const launchedMs = performance.now() - t0
  await page.goto(`${server.url}/`, { waitUntil: 'domcontentloaded', timeout: 30_000 })
  return {
    kind: 'rune-web',
    rootPid,
    launchedMs,
    version: `Chromium ${browser.version()}, Rune ${app} wasm`,
    backend: 'rune, renderer not settled yet',
    webgpu: lavapipe,
    page,
    collector,
    close: async () => {
      collector.stop()
      await browser.close().catch(() => undefined)
      await server.close()
    }
  }
}

export function runeWebDriver(handle: RuneWebHandle, app: RuneAppName): ClientDriver {
  const { page, collector } = handle
  return {
    ...runeOnlyDriver(handle),
    // The first frame: the mirror shows the app's ready text.
    waitForProjectList: () =>
      until(
        () => mirrorShows(page, RUNE_APPS[app].ready),
        READY_MS,
        `rune-web ${app} never painted`
      ),
    // Settled: the renderer that keeps drawing, after any hand-over to the GPU.
    waitForLauncher: async () => {
      const deadline = Date.now() + (handle.webgpu ? HANDOVER_MS : 0)
      let renderer = await detectRenderer(page)
      while (renderer === 'cpu' && Date.now() < deadline) {
        await sleep(150)
        renderer = await detectRenderer(page)
      }
      handle.backend = `rune ${renderer}`
    },
    beginPhase: async (name) => {
      await collector.drainPhase()
      await collector.markPhaseStart(name)
    },
    endPhase: async (name) => {
      await collector.markPhaseEnd(name)
      return (await collector.drainPhase()).page
    },
    // The web build exposes no element tree; its accessibility mirror stands in for one.
    census: () =>
      page.evaluate(() => {
        const all = [...(document.querySelector('[data-rune-a11y]')?.querySelectorAll('*') ?? [])]
        const painted = all.filter((el) => {
          const r = el.getBoundingClientRect()
          return r.width > 0 && r.height > 0
        }).length
        return { reachable: all.length, painted, hidden: all.length - painted }
      })
  }
}

/** Every process of the native app is the app. */
export const runeDesktopRole: RoleOf = () => 'app'

export interface RuneDesktopHandle extends ClientHandle {
  socket(): Promise<RuneSocket>
}

export async function launchRuneDesktop(opts: ClientLaunchOptions): Promise<RuneDesktopHandle> {
  const app = opts.runeApp
  if (!RUNE_APPS[app].liveDriver) {
    throw new Error(
      `the native ${app} does not link the live driver; run rune-desktop with --rune-app hello`
    )
  }
  const bin = runeBinary(app)
  const sockDir = mkdtempSync(path.join(os.tmpdir(), 'rune-drive-'))
  const sock = path.join(sockDir, 'drive.sock')
  const { WAYLAND_DISPLAY: _wayland, ...env } = process.env
  const vulkan =
    opts.gl === 'lavapipe' && existsSync(LAVAPIPE) ? { VK_ICD_FILENAMES: LAVAPIPE } : {}
  const t0 = performance.now()
  const child: ChildProcess = spawn(bin, [], {
    env: { ...env, ...vulkan, DISPLAY: opts.display, RUNE_DRIVE_SOCK: sock },
    stdio: 'ignore'
  })
  if (child.pid === undefined) throw new Error(`could not start ${bin}`)
  const launchedMs = performance.now() - t0
  let socket: Promise<RuneSocket> | null = null
  return {
    kind: 'rune-desktop',
    rootPid: child.pid,
    launchedMs,
    version: `Rune ${app} (${bin})`,
    backend: Object.keys(vulkan).length ? 'Vulkan, lavapipe' : 'Vulkan, system driver',
    socket: () => {
      socket ??= openRuneSocket(sock, READY_MS)
      return socket
    },
    close: async () => {
      if (socket) (await socket.catch(() => null))?.close()
      child.kill()
      if (child.exitCode === null) await new Promise((r) => child.once('exit', r))
      rmSync(sockDir, { recursive: true, force: true })
    }
  }
}

export function runeDesktopDriver(handle: RuneDesktopHandle, app: RuneAppName): ClientDriver {
  const ready = async () => {
    const socket = await handle.socket()
    await socket.must({ cmd: 'wait', text: RUNE_APPS[app].ready, timeout_ms: READY_MS })
  }
  return {
    ...runeOnlyDriver(handle),
    waitForProjectList: ready,
    waitForLauncher: ready,
    beginPhase: async () => {},
    endPhase: async () => null,
    census: async (): Promise<ClientTreeCensus> => {
      const socket = await handle.socket()
      const elements = ((await socket.must({ cmd: 'tree' })).elements as RuneElement[]) ?? []
      const painted = elements.filter((e) => e.rect[2] > 0 && e.rect[3] > 0).length
      return { reachable: elements.length, painted, hidden: elements.length - painted }
    }
  }
}
