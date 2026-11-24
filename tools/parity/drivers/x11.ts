/**
 * The X server side of a pixel lane: a private Xvfb, a private D-Bus session
 * bus, frame grabs with ffmpeg, and the XTEST helpers in `../x11/`.
 */

import { existsSync } from 'node:fs'
import path from 'node:path'
import type { Point } from './driver'

const X11_DIR = path.join(import.meta.dir, '../x11')

function freeDisplay(): number {
  for (let n = 90; n < 200; n++) {
    if (!existsSync(`/tmp/.X${n}-lock`) && !existsSync(`/tmp/.X11-unix/X${n}`)) return n
  }
  throw new Error('no free X display between :90 and :199')
}

export async function startXvfb(
  width: number,
  height: number
): Promise<{ display: string; stop(): void }> {
  const number = freeDisplay()
  const display = `:${number}`
  const child = Bun.spawn(['Xvfb', display, '-nocursor', '-screen', '0', `${width}x${height}x24`], {
    stdout: 'ignore',
    stderr: 'ignore'
  })
  console.error(`[parity] Xvfb ${display} pid ${child.pid}`)
  const socket = `/tmp/.X11-unix/X${number}`
  const deadline = Date.now() + 5000
  while (!existsSync(socket)) {
    if (child.exitCode !== null) throw new Error(`Xvfb ${display} exited with ${child.exitCode}`)
    if (Date.now() > deadline) {
      child.kill()
      throw new Error(`Xvfb ${display} did not come up`)
    }
    await Bun.sleep(50)
  }
  return { display, stop: () => child.kill() }
}

/**
 * A private D-Bus session bus with the display in its activation environment,
 * so the app's file dialogs reach xdg-desktop-portal as on a desktop. Without
 * `dbus-daemon` the run goes on without a bus.
 */
export async function startSessionBus(
  display: string
): Promise<{ address: string | null; stop(): void }> {
  if (!Bun.which('dbus-daemon')) return { address: null, stop: () => {} }
  const child = Bun.spawn(['dbus-daemon', '--session', '--nofork', '--print-address=1'], {
    stdout: 'pipe',
    stderr: 'ignore'
  })
  const reader = child.stdout.getReader()
  const { value } = await reader.read()
  reader.releaseLock()
  const address = new TextDecoder().decode(value).split('\n')[0]?.trim() ?? ''
  if (!address) {
    child.kill()
    return { address: null, stop: () => {} }
  }
  Bun.spawnSync(['dbus-update-activation-environment', `DISPLAY=${display}`], {
    env: { ...process.env, DBUS_SESSION_BUS_ADDRESS: address },
    stdout: 'ignore',
    stderr: 'ignore'
  })
  return { address, stop: () => child.kill() }
}

/** One frame of the `width`x`height` screen area at `origin`, without the pointer. */
export async function grabFrame(
  display: string,
  width: number,
  height: number,
  outPath: string,
  origin: Point = { x: 0, y: 0 }
): Promise<void> {
  const ffmpeg = Bun.spawn(
    [
      'ffmpeg',
      '-loglevel',
      'error',
      '-y',
      '-f',
      'x11grab',
      '-draw_mouse',
      '0',
      '-video_size',
      `${width}x${height}`,
      '-i',
      `${display}+${origin.x},${origin.y}`,
      '-frames:v',
      '1',
      outPath
    ],
    { stdout: 'ignore', stderr: 'pipe' }
  )
  if ((await ffmpeg.exited) !== 0) {
    throw new Error(`ffmpeg x11grab failed: ${await new Response(ffmpeg.stderr).text()}`)
  }
}

/** Run one of the `../x11/*.py` helpers on `display` and return its stdout. */
export function x11(display: string, script: string, ...args: string[]): string {
  const proc = Bun.spawnSync(
    [
      'uv',
      'run',
      '--quiet',
      '--with',
      'python-xlib==0.33',
      'python',
      '-I',
      path.join(X11_DIR, script),
      ...args
    ],
    { env: { ...process.env, DISPLAY: display } }
  )
  if (proc.exitCode !== 0) {
    throw new Error(
      `${script} ${args.join(' ')}: ${proc.stderr.toString().trim().split('\n').at(-1)}`
    )
  }
  return proc.stdout.toString().trim()
}

/** `ctrl-shift-n`, `enter` to an X keysym combination (`Control_L+Shift_L+n`, `Return`). */
export function x11Keys(stroke: string): string {
  const named: Record<string, string> = {
    ctrl: 'Control_L',
    shift: 'Shift_L',
    alt: 'Alt_L',
    enter: 'Return',
    escape: 'Escape',
    tab: 'Tab',
    backspace: 'BackSpace',
    space: 'space',
    ',': 'comma',
    '=': 'equal',
    '-': 'minus'
  }
  const parts = stroke.match(/^((?:(?:ctrl|alt|shift)-)*)(.+)$/)
  const mods = (parts?.[1] ?? '').split('-').filter(Boolean)
  const key = parts?.[2] ?? stroke
  return [...mods, key].map((part) => named[part] ?? part).join('+')
}
