/**
 * The Rune builds the drivers open, where their artifacts live, the static
 * server a wasm build runs from, and how a page tells which renderer drew.
 *
 * Node only (no Bun APIs, no extensionless imports): `tools/perf` runs under
 * Node and imports this file too.
 *
 * Build the artifacts from `apps/rune` (the paths `.github/workflows/rune.yml` uses):
 *
 *   cargo build --release -p hello --features tooling   # live driver linked in
 *   cargo build --release -p gallery
 *   cargo build --release -p troupe-dev                  # the shell, live driver linked in
 *   bash <rune checkout>/scripts/web-build.sh hello --out target/web
 *   bash <rune checkout>/scripts/web-build.sh gallery --out target/web-gallery
 *   bash <rune checkout>/scripts/web-build.sh troupe-web --out target/web-troupe --no-html
 *   cp crates/web/index.html target/web-troupe/
 */

import { existsSync, readFileSync } from 'node:fs'
import { createServer, request, type Server } from 'node:http'
import { connect } from 'node:net'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import type { Page } from 'playwright'
import type { AreaId, Surface } from '../types'

const REPO_DIR = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../..')
const RUNE_DIR = path.join(REPO_DIR, 'apps/rune')

/**
 * A Rune app the drivers can open. `ready` is a text the app paints once its
 * first frame is up: a mirror button on the web, an element's text natively.
 */
export interface RuneApp {
  name: 'hello' | 'gallery' | 'troupe'
  /** The default `web-build.sh --out` folder, relative to `apps/rune`. */
  web: string
  /** The native binary's name under the cargo target dir. */
  bin: string
  ready: string
  /** The native build links the live driver (`automation`), so `RUNE_DRIVE_SOCK` drives its window. */
  liveDriver: boolean
}

export const RUNE_APPS = {
  hello: { name: 'hello', web: 'target/web', bin: 'hello', ready: 'Press me', liveDriver: true },
  gallery: {
    name: 'gallery',
    web: 'target/web-gallery',
    bin: 'gallery',
    ready: 'Surface tokens',
    liveDriver: false
  },
  /** The workspace shell (`troupe-web`, `troupe-dev`); it needs a seeded server to paint `ready`. */
  troupe: {
    name: 'troupe',
    web: 'target/web-troupe',
    bin: 'troupe-dev',
    ready: 'What should we do in alpha?',
    liveDriver: true
  }
} as const satisfies Record<string, RuneApp>
export type RuneAppName = keyof typeof RUNE_APPS

/**
 * The surfaces the Rune shell draws today: the frame, the rail's projects and
 * the side panel frame (R5a), the tab bar's `+` menu (R5b), and the rail's
 * overlays, the theme picker, the shortcut menu and the phone shell (R5c),
 * and the chat transcript (R6b). `chat.table-zoom` waits: its click is a CSS selector, which
 * the Rune drivers act on at React's recorded point, after React scrolled the table into view.
 * Every other surface waits on the unit that ports its content, named by its
 * area in `RUNE_UNIT_BY_AREA`.
 */
export const RUNE_SHELL_SURFACES: ReadonlySet<string> = new Set([
  'shell.dashboard',
  'shell.not-found',
  'rail.project-list',
  'rail.collapse',
  'rail.project-menu',
  'rail.project-colour',
  'rail.delete-project',
  'rail.group-menu',
  'rail.delete-group',
  'rail.new-project',
  'rail.new-project-modal',
  'rail.new-group',
  'rail.chat-menu',
  'theme.picker',
  'shortcuts.menu',
  'phone.drawer',
  'phone.header-more',
  'phone.project-sheet',
  'phone.recents-actions',
  'terminal.new-tab-menu',
  'chat.transcript',
  'composer.mention',
  'composer.slash',
  'composer.add-menu'
])

const RUNE_UNIT_BY_AREA: Record<AreaId, string> = {
  rail: 'R5c',
  'side-panel': 'R5c',
  statusbar: 'R5c',
  theme: 'R5c',
  palette: 'R5c',
  shortcuts: 'R5c',
  dialogs: 'R5c',
  notifications: 'R5c',
  remote: 'R5c',
  chat: 'R6',
  composer: 'R6',
  terminal: 'R7',
  explorer: 'R8',
  editor: 'R8',
  git: 'R9',
  github: 'R9',
  settings: 'R9',
  ssh: 'R9',
  snapshots: 'R9',
  automations: 'R9',
  browser: 'R11',
  canvas: 'R11'
}

/** Why a surface cannot run on a Rune client yet, or undefined when the shell draws it. */
export function runePending(surface: Pick<Surface, 'id' | 'area' | 'profile'>): Surface['pending'] {
  if (RUNE_SHELL_SURFACES.has(surface.id)) return undefined
  if ((surface.profile ?? 'desktop') === 'phone') {
    const unit = RUNE_UNIT_BY_AREA[surface.area]
    return {
      unit,
      reason: `the Rune phone shell draws its header, drawer and sheets; ${unit} ports this surface`
    }
  }
  const unit = RUNE_UNIT_BY_AREA[surface.area]
  return { unit, reason: `the Rune shell draws the frame only; ${unit} ports this surface` }
}

const envName = (app: RuneAppName, kind: 'WEB' | 'BIN') =>
  `TROUPE_RUNE_${app.toUpperCase()}_${kind}`

/** `TROUPE_RUNE_<APP>_WEB`, else `apps/rune/<app.web>`: a `web-build.sh` output folder. */
export function runeWebDist(app: RuneAppName): string {
  const dir = process.env[envName(app, 'WEB')] ?? path.join(RUNE_DIR, RUNE_APPS[app].web)
  if (!existsSync(path.join(dir, 'app_bg.wasm'))) {
    throw new Error(
      `no wasm build of ${app} at ${dir}: set ${envName(app, 'WEB')} or run web-build.sh (see drivers/rune-apps.ts)`
    )
  }
  return dir
}

/** `TROUPE_RUNE_<APP>_BIN`, else the release build, then the debug build, under `CARGO_TARGET_DIR` or `apps/rune/target`. */
export function runeBinary(app: RuneAppName): string {
  const fromEnv = process.env[envName(app, 'BIN')]
  if (fromEnv) {
    if (!existsSync(fromEnv)) throw new Error(`${envName(app, 'BIN')}=${fromEnv} does not exist`)
    return fromEnv
  }
  const target = process.env.CARGO_TARGET_DIR ?? path.join(RUNE_DIR, 'target')
  const found = ['release', 'debug']
    .map((p) => path.join(target, p, RUNE_APPS[app].bin))
    .find(existsSync)
  if (!found) {
    throw new Error(
      `no native build of ${app} under ${target}: set ${envName(app, 'BIN')} or build it (see drivers/rune-apps.ts)`
    )
  }
  return found
}

const TYPES: Record<string, string> = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript',
  // `WebAssembly.instantiateStreaming` refuses any other type.
  '.wasm': 'application/wasm',
  '.ttf': 'font/ttf',
  '.woff2': 'font/woff2',
  '.json': 'application/json',
  '.css': 'text/css'
}

/** Test servers stay in 8190-8199; scanning down keeps clear of the parity and perf servers, which scan up. */
const PORTS = [8199, 8198, 8197, 8196, 8195, 8194, 8193, 8192, 8191, 8190]

export interface StaticServer {
  url: string
  close(): Promise<void>
}

function listen(server: Server, port: number): Promise<boolean> {
  return new Promise((resolve) => {
    server.once('error', () => resolve(false))
    server.listen(port, '127.0.0.1', () => resolve(true))
  })
}

/** Serves `dir` on 127.0.0.1 at the first free port of 8199 down to 8190. */
export async function serveStatic(dir: string): Promise<StaticServer> {
  const root = path.resolve(dir)
  for (const port of PORTS) {
    const server = createServer((req, res) => {
      const pathname = decodeURIComponent(new URL(req.url ?? '/', 'http://x').pathname)
      const file = path.join(root, pathname === '/' ? 'index.html' : pathname)
      let body: Buffer
      try {
        if (!file.startsWith(root + path.sep)) throw new Error('outside the build')
        body = readFileSync(file)
      } catch {
        res.writeHead(404).end()
        return
      }
      res.writeHead(200, {
        'content-type': TYPES[path.extname(file)] ?? 'application/octet-stream',
        'cache-control': 'no-store'
      })
      res.end(body)
    })
    if (await listen(server, port)) {
      console.error(`[rune] serving ${root} on http://127.0.0.1:${port}/`)
      return {
        url: `http://127.0.0.1:${port}`,
        close: () => new Promise((resolve) => server.close(() => resolve()))
      }
    }
  }
  throw new Error(`no free port in 8190-8199 for ${root}`)
}

/**
 * Serves the Rune web client's build at `/` and forwards every other request,
 * WebSocket upgrades included, to the Troupe server at `upstream`, so the page
 * and the server share one origin as they will when the server serves the
 * client itself. The Host header passes through unchanged, so the server's
 * Origin check sees the page's own origin.
 */
export async function serveRuneClient(dir: string, upstream: string): Promise<StaticServer> {
  const root = path.resolve(dir)
  const target = new URL(upstream)
  const own = (pathname: string): string | null => {
    const file = path.join(root, pathname === '/' ? 'index.html' : pathname)
    return file.startsWith(root + path.sep) && existsSync(file) ? file : null
  }
  for (const port of PORTS) {
    const server = createServer((req, res) => {
      const pathname = decodeURIComponent(new URL(req.url ?? '/', 'http://x').pathname)
      const file = own(pathname)
      if (file) {
        res.writeHead(200, {
          'content-type': TYPES[path.extname(file)] ?? 'application/octet-stream',
          'cache-control': 'no-store'
        })
        res.end(readFileSync(file))
        return
      }
      const forward = request(
        {
          host: target.hostname,
          port: target.port,
          method: req.method,
          path: req.url,
          headers: req.headers
        },
        (reply) => {
          res.writeHead(reply.statusCode ?? 502, reply.headers)
          reply.pipe(res)
        }
      )
      forward.on('error', () => res.writeHead(502).end())
      req.pipe(forward)
    })
    server.on('upgrade', (req, socket, head) => {
      const back = connect(Number(target.port), target.hostname, () => {
        const lines = [`${req.method} ${req.url} HTTP/${req.httpVersion}`]
        for (let i = 0; i < req.rawHeaders.length; i += 2) {
          lines.push(`${req.rawHeaders[i]}: ${req.rawHeaders[i + 1]}`)
        }
        back.write(`${lines.join('\r\n')}\r\n\r\n`)
        if (head.length) back.write(head)
        back.pipe(socket)
        socket.pipe(back)
      })
      const drop = () => {
        back.destroy()
        socket.destroy()
      }
      back.on('error', drop)
      socket.on('error', drop)
      back.on('close', () => socket.destroy())
      socket.on('close', () => back.destroy())
    })
    if (await listen(server, port)) {
      // A matrix run ends without closing it; it must not keep the process alive.
      server.unref()
      console.error(
        `[rune] serving ${root} on http://127.0.0.1:${port}/, the rest from ${upstream}`
      )
      return {
        url: `http://127.0.0.1:${port}`,
        close: () => new Promise((resolve) => server.close(() => resolve()))
      }
    }
  }
  throw new Error(`no free port in 8190-8199 for ${root}`)
}

export type RuneRenderer = 'webgpu' | 'webgl2' | 'cpu' | 'none'

/**
 * Which renderer drew the page's last frame. Rune paints first into a CPU
 * canvas marked `data-rune-cpu` and keeps it while WebGPU (or the opt-in
 * WebGL2) never comes. Once the GPU took over, the page canvas already holds
 * its context, so asking for the other kind returns null and creates nothing.
 * Call it after the app's first frame is up.
 */
export function detectRenderer(page: Page): Promise<RuneRenderer> {
  return page.evaluate(() => {
    if (document.querySelector('canvas[data-rune-cpu]')) return 'cpu' as const
    for (const canvas of document.querySelectorAll('canvas')) {
      if (canvas.getContext('webgpu')) return 'webgpu' as const
      if (canvas.getContext('webgl2')) return 'webgl2' as const
    }
    return 'none' as const
  })
}

/** Chromium flags that expose a WebGPU adapter (SwiftShader, or lavapipe through ANGLE Vulkan when headed). */
export const WEBGPU_ARGS = ['--enable-unsafe-webgpu']
