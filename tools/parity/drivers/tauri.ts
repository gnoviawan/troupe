/**
 * The `tauri` client: the shipped desktop app (Tauri on WebKitGTK), for a
 * surface the web client cannot render. WebKitGTK has no automation hook on
 * Linux, so the app is driven like a user drives it, with XTEST keys and
 * clicks at the points the target-resolving client recorded, and read only
 * through its pixels: no checks, no region, no text boxes.
 *
 * Each session gets a throwaway HOME and XDG dirs under its state dir, so the
 * app never opens the real data dir, and starts with the world's projects.
 * `TROUPE_TAURI_BIN` (or `TERMUL_TAURI_BIN`) picks the binary; the default is
 * this checkout's release build:
 *
 *   bun x tauri build --no-bundle
 */

import { copyFileSync, existsSync, mkdirSync, rmSync } from 'node:fs'
import path from 'node:path'
import { readPng } from '../png'
import { REPO_DIR } from '../server'
import type { ActionLog } from '../types'
import {
  type ClientLauncher,
  describe,
  errorLine,
  PARK_POINT,
  PARKS_POINTER,
  type Point,
  SETTLE_AFTER_ACTION_MS
} from './driver'
import { VIEWPORT } from './react'
import { grabFrame, startSessionBus, startXvfb, x11, x11Keys } from './x11'

const TAURI_BIN =
  process.env.TROUPE_TAURI_BIN ??
  process.env.TERMUL_TAURI_BIN ??
  path.join(REPO_DIR, 'src-tauri/target/release/troupe')
/** The identifiers of the release and the dev config: the app data dir each build reads. */
const IDENTIFIERS = ['com.termul-manager.app', 'com.termul-manager.app.dev']
const WINDOW_TITLE = 'Troupe'
/** Room around the window, so the whole window is on screen wherever the app first maps it. */
const SCREEN_MARGIN = 64
const READY_TIMEOUT_MS = 90_000
const SETTLE_TIMEOUT_MS = 20_000
/** alpha's avatar in the rail, `blue` as the app paints it. */
const AVATAR_RGB = [60, 131, 246]

/** The rail painted alpha's avatar: the app has loaded its projects. */
function avatarShown(file: string): boolean {
  const png = readPng(file)
  let n = 0
  for (let y = 100; y < Math.min(400, png.height); y++) {
    for (let x = 0; x < Math.min(200, png.width); x++) {
      const i = (y * png.width + x) * 4
      if (
        Math.abs(png.data[i] - AVATAR_RGB[0]) < 12 &&
        Math.abs(png.data[i + 1] - AVATAR_RGB[1]) < 12 &&
        Math.abs(png.data[i + 2] - AVATAR_RGB[2]) < 12
      )
        n++
    }
  }
  return n >= 100
}

export function tauriClient(): ClientLauncher {
  return {
    client: 'tauri',
    resolvesTargets: false,
    async session({ world, points, stateDir }, body) {
      if (!existsSync(TAURI_BIN))
        throw new Error(`no desktop app at ${TAURI_BIN} (see drivers/tauri.ts)`)
      const home = path.join(stateDir, 'home')
      rmSync(home, { recursive: true, force: true })
      // The desktop app keeps the same project registry file the server writes.
      for (const identifier of IDENTIFIERS) {
        const dataDir = path.join(home, '.local/share', identifier)
        mkdirSync(dataDir, { recursive: true })
        copyFileSync(
          path.join(world.server.stateDir, 'projects.json'),
          path.join(dataDir, 'projects.json')
        )
      }
      const { width, height } = VIEWPORT
      const xvfb = await startXvfb(width + SCREEN_MARGIN, height + SCREEN_MARGIN)
      const bus = await startSessionBus(xvfb.display)
      const app = Bun.spawn([TAURI_BIN], {
        env: {
          ...process.env,
          DISPLAY: xvfb.display,
          ...(bus.address && { DBUS_SESSION_BUS_ADDRESS: bus.address }),
          HOME: home,
          XDG_DATA_HOME: path.join(home, '.local/share'),
          XDG_CONFIG_HOME: path.join(home, '.config'),
          XDG_CACHE_HOME: path.join(home, '.cache'),
          XDG_STATE_HOME: path.join(home, '.local/state')
        },
        stdout: Bun.file(path.join(stateDir, 'app.log')),
        stderr: Bun.file(path.join(stateDir, 'app.err.log'))
      })
      console.error(`[parity] desktop app pid ${app.pid} on ${xvfb.display}`)
      const origin: Point = { x: SCREEN_MARGIN / 2, y: SCREEN_MARGIN / 2 }
      const shoot = (file: string) => grabFrame(xvfb.display, width, height, file, origin)
      /**
       * Grab until two grabs a second apart match, so the frame is one the app
       * has stopped painting. The app's text cannot be read, so this stands in
       * for the checks a DOM client waits on.
       */
      const settled = async (file: string, ready: () => boolean, timeoutMs: number) => {
        const deadline = Date.now() + timeoutMs
        let previous = ''
        for (;;) {
          if (app.exitCode !== null) throw new Error(`desktop app exited with ${app.exitCode}`)
          await shoot(file)
          const frame = Bun.hash(await Bun.file(file).arrayBuffer()).toString()
          if (frame === previous && ready()) return true
          if (Date.now() > deadline) return false
          previous = frame
          await Bun.sleep(1000)
        }
      }
      const at = (point: Point) => [
        String(Math.round(origin.x + point.x)),
        String(Math.round(origin.y + point.y))
      ]
      try {
        x11(
          xvfb.display,
          'x11-window.py',
          WINDOW_TITLE,
          String(origin.x),
          String(origin.y),
          String(width),
          String(height)
        )
        const probe = path.join(stateDir, 'ready.png')
        if (!(await settled(probe, () => avatarShown(probe), READY_TIMEOUT_MS))) {
          throw new Error('the desktop app never painted its project rail')
        }
        return await body({
          async act(step, index, action) {
            const log: ActionLog = { step, action: describe(action), via: 'none', ok: true }
            try {
              switch (action.do) {
                case 'click': {
                  if (action.button || action.modifiers) throw new Error('only plain left clicks')
                  const fixed = 'point' in action.target ? action.target.point : undefined
                  const point = fixed ?? points.get(`${step}/${index}`)
                  if (!point) throw new Error('no client recorded a point for this click')
                  log.via = fixed ? 'point' : 'recorded-point'
                  x11(xvfb.display, 'xinput.py', 'click', ...at(point))
                  break
                }
                case 'press':
                  log.via = 'key'
                  for (const stroke of action.keys.split(' '))
                    x11(xvfb.display, 'xinput.py', 'key', x11Keys(stroke))
                  break
                case 'type':
                  log.via = 'key'
                  x11(xvfb.display, 'xinput.py', 'type', action.text)
                  break
                case 'wait':
                  await Bun.sleep(action.ms)
                  break
                case 'choose': {
                  const point = points.get(`${step}/${index}`)
                  if (!point) throw new Error('no client recorded a point for this chooser')
                  log.via = 'recorded-point'
                  x11(xvfb.display, 'xinput.py', 'click', ...at(point))
                  x11(
                    xvfb.display,
                    'x11-file-chooser.py',
                    'Open File',
                    path.join(world.root, action.file)
                  )
                  break
                }
                default:
                  throw new Error(`${action.do} is not driven in the desktop app`)
              }
              if (PARKS_POINTER.has(action.do))
                x11(xvfb.display, 'xinput.py', 'move', ...at(PARK_POINT))
              await Bun.sleep(SETTLE_AFTER_ACTION_MS)
            } catch (error) {
              log.ok = false
              log.error = errorLine(error)
            }
            return log
          },
          async grab(file) {
            if (!(await settled(file, () => true, SETTLE_TIMEOUT_MS))) {
              console.error(`[parity] ${file}: still painting after ${SETTLE_TIMEOUT_MS} ms`)
            }
          }
        })
      } finally {
        app.kill()
        await app.exited
        bus.stop()
        xvfb.stop()
      }
    }
  }
}
