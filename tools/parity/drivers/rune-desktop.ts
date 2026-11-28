/**
 * The `rune-desktop` client: a native Rune window under a private Xvfb,
 * drawn by Mesa's lavapipe (no GPU here) and driven only through the live
 * driver (`apps/rune/crates/automation`) on the socket `RUNE_DRIVE_SOCK`
 * names. The driver reads Rune's own element tree, so targets are found by
 * key (a surface's test id), by text or by accessible name; input goes in
 * through the window's runtime, and frames come back through `snap`.
 *
 * The live driver clicks, types and presses keys. It has no hover, wheel,
 * drag, right click or file chooser yet, so those actions fail with that
 * reason.
 */

import { existsSync, mkdirSync, mkdtempSync, rmSync } from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import type { PartRects } from '../diff'
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
  type Point,
  type Points,
  PRESENT_TIMEOUT_MS,
  poll,
  SETTLE_AFTER_ACTION_MS
} from './driver'
import { VIEWPORT } from './react'
import { runeBinary } from './rune-apps'
import { openRuneSocket, type RuneElement, type RuneSocket, runeKey } from './rune-socket'
import { startXvfb } from './x11'

const LAVAPIPE = '/usr/share/vulkan/icd.d/lvp_icd.json'
const READY_TIMEOUT_MS = 90_000

/** `VK_ICD_FILENAMES` as set, else lavapipe when this host has it. */
export function vulkanEnv(): Record<string, string> {
  const set = process.env.VK_ICD_FILENAMES
  if (set) return { VK_ICD_FILENAMES: set }
  return existsSync(LAVAPIPE) ? { VK_ICD_FILENAMES: LAVAPIPE } : {}
}

const centre = ({ rect: [x, y, w, h] }: RuneElement): Point => ({ x: x + w / 2, y: y + h / 2 })

const elementsOf = async (socket: RuneSocket) =>
  ((await socket.must({ cmd: 'tree' })).elements as RuneElement[]) ?? []

/** Every text and name in the tree, one per line, in tree order. */
const treeText = async (socket: RuneSocket) =>
  (await elementsOf(socket))
    .flatMap((e) => [e.text, e.label].filter((t): t is string => !!t))
    .join('\n')

export function runeDesktopDriver(socket: RuneSocket, points: Points): Driver {
  const pointOf = async (
    key: string,
    target: Target
  ): Promise<{ point: Point; via: ActionLog['via'] }> => {
    if ('point' in target) return { point: target.point, via: 'point' }
    const recorded = points.get(key)
    if ('css' in target) {
      if (!recorded) throw new Error('Rune has no CSS selectors, and no client recorded a point')
      return { point: recorded, via: 'recorded-point' }
    }
    if ('testId' in target && recorded) {
      // A test id may name no Rune key (React repeats `new-tab-menu-trigger` in every pane,
      // Rune keys each pane's apart), so the reference's point stands in, as on the web.
      const hit = await socket.send({ cmd: 'wait', key: target.testId, timeout_ms: 1000 })
      if (!hit.ok) return { point: recorded, via: 'recorded-point' }
    }
    if ('text' in target && target.exact === false) {
      // The live driver matches whole texts only; a prefix (a truncated chat title) is found
      // in the tree, as the web driver finds it in the mirror.
      const needle = normalize(target.text)
      let found: RuneElement | undefined
      const shown = await poll(async () => {
        found = (await elementsOf(socket)).find((e) =>
          [e.text, e.label].some((t) => !!t && normalize(t).includes(needle))
        )
        return found !== undefined
      }, PRESENT_TIMEOUT_MS)
      if (!shown || !found) throw new Error(`the tree never showed "${target.text}"`)
      const point = centre(found)
      points.set(key, point)
      return { point, via: 'text' }
    }
    const [query, via]: [{ key: string } | { text: string }, ActionLog['via']] =
      'testId' in target
        ? [{ key: target.testId }, 'testid']
        : ['text' in target ? { text: target.text } : { text: target.label }, 'text']
    await socket.must({ cmd: 'wait', ...query, timeout_ms: PRESENT_TIMEOUT_MS })
    const point = centre((await socket.must({ cmd: 'find', ...query })).element as RuneElement)
    points.set(key, point)
    return { point, via }
  }
  return {
    async act(step, index, action) {
      const log: ActionLog = { step, action: describe(action), via: 'none', ok: true }
      try {
        switch (action.do) {
          case 'click': {
            if (action.button || action.modifiers)
              throw new Error('the live driver has no right or modified click')
            const { point, via } = await pointOf(`${step}/${index}`, action.target)
            log.via = via
            await socket.must({ cmd: 'click', x: point.x, y: point.y })
            break
          }
          case 'press':
            log.via = 'key'
            for (const stroke of action.keys.split(' '))
              await socket.must({ cmd: 'key', ...runeKey(stroke) })
            break
          case 'type':
            log.via = 'key'
            await socket.must({ cmd: 'type', text: action.text })
            break
          case 'wait':
            await new Promise((r) => setTimeout(r, action.ms))
            break
          default:
            throw new Error(`the live driver has no ${action.do}`)
        }
        await new Promise((r) => setTimeout(r, SETTLE_AFTER_ACTION_MS))
      } catch (error) {
        log.ok = false
        log.error = errorLine(error)
      }
      return log
    },
    // The tree carries keys but not the reference's test id subtrees, so `within` reads the whole tree.
    async check(step, check) {
      if (check.order) {
        const order = check.order
        return {
          step,
          text: order.join(' < '),
          expect: 'present',
          pass: await poll(async () => ordered(await treeText(socket), order), PRESENT_TIMEOUT_MS)
        }
      }
      const want = normalize(check.text)
      const painted = async () => normalize(await treeText(socket)).includes(want)
      const pass = check.gone
        ? await poll(async () => !(await painted()), check.timeoutMs ?? GONE_TIMEOUT_MS)
        : await poll(painted, check.timeoutMs ?? PRESENT_TIMEOUT_MS)
      return { step, text: check.text, expect: check.gone ? 'gone' : 'present', pass }
    },
    async grab(file) {
      await socket.must({ cmd: 'snap', path: path.resolve(file) })
    },
    async parts(): Promise<PartRects> {
      const box = ({ rect: [x, y, width, height] }: RuneElement) => ({ x, y, width, height })
      const elements = (await elementsOf(socket)).filter((e) => e.rect[2] > 0 && e.rect[3] > 0)
      return {
        text: elements.filter((e) => e.text && e.role !== 'button').map(box),
        icons: elements.filter((e) => e.role === 'image').map(box)
      }
    }
  }
}

export interface RuneDesktopSession {
  pid: number
  socket: RuneSocket
  driver: Driver
  close(): Promise<void>
}

export interface RuneDesktopOptions {
  bin: string
  /** Per-session scratch space: a throwaway HOME and the app's logs. */
  stateDir: string
  /** A text the window paints once its first frame is up. */
  ready: string
  screen?: { width: number; height: number }
  env?: Record<string, string>
  points?: Points
}

/** Starts `bin` as a real window on a private Xvfb and waits until the live driver sees `ready`. */
export async function openRuneDesktop(opts: RuneDesktopOptions): Promise<RuneDesktopSession> {
  const { width, height } = opts.screen ?? VIEWPORT
  const home = path.join(opts.stateDir, 'home')
  rmSync(home, { recursive: true, force: true })
  mkdirSync(home, { recursive: true })
  // A Unix socket path is limited to about 100 bytes; state dirs can be longer.
  const sockDir = mkdtempSync(path.join(os.tmpdir(), 'rune-drive-'))
  const sock = path.join(sockDir, 'drive.sock')
  const xvfb = await startXvfb(width, height)
  const { WAYLAND_DISPLAY: _wayland, ...inherited } = process.env
  const app = Bun.spawn([opts.bin], {
    env: {
      ...inherited,
      ...vulkanEnv(),
      ...opts.env,
      DISPLAY: xvfb.display,
      RUNE_DRIVE_SOCK: sock,
      HOME: home,
      XDG_DATA_HOME: path.join(home, '.local/share'),
      XDG_CONFIG_HOME: path.join(home, '.config'),
      XDG_CACHE_HOME: path.join(home, '.cache'),
      XDG_STATE_HOME: path.join(home, '.local/state')
    },
    stdout: Bun.file(path.join(opts.stateDir, 'app.log')),
    stderr: Bun.file(path.join(opts.stateDir, 'app.err.log'))
  })
  console.error(`[rune-desktop] ${path.basename(opts.bin)} pid ${app.pid} on ${xvfb.display}`)
  const stop = async () => {
    app.kill()
    await app.exited
    xvfb.stop()
    rmSync(sockDir, { recursive: true, force: true })
  }
  try {
    const exited = app.exited.then((code) => {
      throw new Error(
        `${opts.bin} exited with ${code} before its window was up (see ${opts.stateDir}/app.err.log)`
      )
    })
    const socket = await Promise.race([openRuneSocket(sock, READY_TIMEOUT_MS), exited])
    await socket.must({ cmd: 'wait', text: opts.ready, timeout_ms: READY_TIMEOUT_MS })
    return {
      pid: app.pid,
      socket,
      driver: runeDesktopDriver(socket, opts.points ?? new Map()),
      async close() {
        socket.close()
        await stop()
      }
    }
  } catch (error) {
    await stop()
    throw error
  }
}

/**
 * The matrix's `rune-desktop` client: `troupe-dev` (or `TROUPE_RUNE_APP_BIN`)
 * pointed at the seeded server through `TROUPE_SERVER_URL`. With
 * `TROUPE_RUNE_APP_BIN` set to `troupe-preview`, the desktop app attaches to
 * that server instead of starting its own, and keeps its logs, lock and frame
 * under the session's state dir.
 */
export function runeDesktopClient(): ClientLauncher {
  return {
    client: 'rune-desktop',
    resolvesTargets: true,
    async session({ world, surface, deviceId, points, stateDir }, body) {
      const bin = process.env.TROUPE_RUNE_APP_BIN ?? runeBinary('troupe')
      mkdirSync(stateDir, { recursive: true })
      const session = await openRuneDesktop({
        bin,
        stateDir,
        ready: surface.start?.ready ?? 'What should we do in alpha?',
        points,
        env: {
          TROUPE_SERVER_URL: world.server.baseUrl,
          TROUPE_SERVER_TOKEN: TOKEN,
          TROUPE_DEVICE_ID: deviceId,
          TROUPE_ROUTE: surface.start?.hash ?? '',
          TROUPE_PREVIEW_DATA_DIR: path.join(stateDir, 'preview')
        }
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
 * The Rune gallery as a native frame per scene: `gallery --scene <id> --size
 * WxH --snap`, Rune's headless render through lavapipe. The gallery does not
 * link the live driver, and a scene picked by `--scene` is the frame alone.
 */
export async function runeDesktopGallery(): Promise<GallerySession> {
  const bin = runeBinary('gallery')
  return {
    renderer: `native --snap (${vulkanEnv().VK_ICD_FILENAMES ? 'lavapipe' : 'system Vulkan'})`,
    async shoot(entry, file) {
      const snap = Bun.spawnSync(
        [
          bin,
          '--scene',
          entry.id,
          '--size',
          `${entry.width}x${entry.height}`,
          '--scale',
          '1',
          '--snap',
          path.resolve(file)
        ],
        { env: { ...process.env, ...vulkanEnv() }, stdout: 'pipe', stderr: 'pipe' }
      )
      if (snap.exitCode !== 0) {
        throw new Error(
          `gallery --scene ${entry.id}: ${snap.stderr.toString().trim().split('\n').at(-1)}`
        )
      }
      return { via: 'none', parts: null }
    },
    close: async () => {}
  }
}
