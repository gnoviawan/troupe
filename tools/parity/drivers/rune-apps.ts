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
 *   bash <rune checkout>/scripts/web-build.sh hello --out target/web
 *   bash <rune checkout>/scripts/web-build.sh gallery --out target/web-gallery
 */

import { existsSync, readFileSync } from 'node:fs'
import { createServer, type Server } from 'node:http'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import type { Page } from 'playwright'

const REPO_DIR = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../..')
const RUNE_DIR = path.join(REPO_DIR, 'apps/rune')

/**
 * A Rune app the drivers can open. `ready` is a text the app paints once its
 * first frame is up: a mirror button on the web, an element's text natively.
 */
export interface RuneApp {
  name: 'hello' | 'gallery'
  /** The default `web-build.sh --out` folder, relative to `apps/rune`. */
  web: string
  ready: string
  /** The native build links the live driver (`automation`), so `RUNE_DRIVE_SOCK` drives its window. */
  liveDriver: boolean
}

export const RUNE_APPS = {
  hello: { name: 'hello', web: 'target/web', ready: 'Press me', liveDriver: true },
  gallery: {
    name: 'gallery',
    web: 'target/web-gallery',
    ready: 'Surface tokens',
    liveDriver: false
  }
} as const satisfies Record<string, RuneApp>
export type RuneAppName = keyof typeof RUNE_APPS

/**
 * Every Troupe surface waits on this unit for the Rune clients: there is no
 * Troupe screen in Rune until R5 serves `/?ui=rune`. Delete it when R5 lands
 * and the matrix drives the surfaces through the same drivers.
 */
export const TROUPE_ON_RUNE_PENDING = {
  unit: 'R5',
  reason: 'Rune has no Troupe screen yet; the R5 shell serves it at /?ui=rune'
} as const

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
  const found = ['release', 'debug'].map((p) => path.join(target, p, app)).find(existsSync)
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
