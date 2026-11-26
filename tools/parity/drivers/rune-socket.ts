/**
 * Client for the Rune live driver (`apps/rune/crates/automation`): newline
 * JSON over the Unix socket a `RUNE_DRIVE_SOCK=<path>` app serves, one reply
 * per command, in order. The same protocol as `scripts/rune/drive.mjs`, typed.
 *
 * Node only (no Bun APIs, no extensionless imports): `tools/perf` runs under
 * Node and imports this file too.
 */

import { createConnection, type Socket } from 'node:net'
import { createInterface } from 'node:readline'

/** One element of the live driver's tree, as `protocol.rs` serializes it. */
export interface RuneElement {
  key: string | null
  path: string
  role: string
  label: string | null
  text: string | null
  /** x, y, width, height in logical pixels. */
  rect: [number, number, number, number]
  focused: boolean
}

export type RuneModifier = 'cmd' | 'ctrl' | 'shift' | 'alt' | 'logo'

export type RuneCommand =
  | { cmd: 'tree' }
  | { cmd: 'a11y' }
  | { cmd: 'find'; key: string }
  | { cmd: 'find'; text: string }
  | { cmd: 'wait'; key: string; timeout_ms?: number }
  | { cmd: 'wait'; text: string; timeout_ms?: number }
  | { cmd: 'click'; key: string }
  | { cmd: 'click'; x: number; y: number }
  | { cmd: 'type'; text: string }
  | { cmd: 'key'; name: string; mods?: RuneModifier[] }
  | { cmd: 'snap'; path: string }

export type RuneReply = { ok: true; us?: number; [field: string]: unknown } | RuneFailure
interface RuneFailure {
  ok: false
  error: string
}

const RETRY_MS = 100

function connectOnce(path: string): Promise<Socket> {
  return new Promise((resolve, reject) => {
    const socket = createConnection(path)
    socket.once('connect', () => {
      socket.off('error', reject)
      resolve(socket)
    })
    socket.once('error', reject)
  })
}

/** Connects to `path`, retrying while the app has not bound it yet. */
async function connect(path: string, timeoutMs: number): Promise<Socket> {
  const deadline = Date.now() + timeoutMs
  for (;;) {
    try {
      return await connectOnce(path)
    } catch (error) {
      const code = (error as NodeJS.ErrnoException).code
      if ((code !== 'ENOENT' && code !== 'ECONNREFUSED') || Date.now() >= deadline) throw error
      await new Promise((r) => setTimeout(r, RETRY_MS))
    }
  }
}

export interface RuneSocket {
  /** Sends `cmd` and resolves to its reply, ok or not. */
  send(cmd: RuneCommand): Promise<RuneReply>
  /** `send`, throwing the driver's error when the reply is not ok. */
  must(cmd: RuneCommand): Promise<Extract<RuneReply, { ok: true }>>
  close(): void
}

export async function openRuneSocket(path: string, timeoutMs = 60_000): Promise<RuneSocket> {
  const socket = await connect(path, timeoutMs)
  const pending: { resolve(reply: RuneReply): void; reject(error: Error): void }[] = []
  let closed = false
  const fail = (error: Error) => {
    closed = true
    for (const p of pending.splice(0)) p.reject(error)
  }
  createInterface({ input: socket }).on('line', (line) => {
    const p = pending.shift()
    if (!p) return
    try {
      p.resolve(JSON.parse(line) as RuneReply)
    } catch {
      p.reject(new Error(`bad reply from the live driver: ${line}`))
    }
  })
  socket.on('close', () => fail(new Error('the live driver closed its socket')))
  socket.on('error', fail)
  const send = (cmd: RuneCommand) =>
    closed
      ? Promise.reject(new Error('the live driver closed its socket'))
      : new Promise<RuneReply>((resolve, reject) => {
          pending.push({ resolve, reject })
          socket.write(`${JSON.stringify(cmd)}\n`)
        })
  return {
    send,
    async must(cmd) {
      const reply = await send(cmd)
      if (!reply.ok) throw new Error(`${cmd.cmd}: ${reply.error}`)
      return reply
    },
    close: () => socket.end()
  }
}

/**
 * A matrix key stroke (`ctrl-shift-k`, `escape`, `alt-,`) as the live
 * driver's `key` command. Names follow `protocol.rs::key_event`.
 */
export function runeKey(stroke: string): { name: string; mods: RuneModifier[] } {
  const named: Record<string, string> = {
    escape: 'Escape',
    enter: 'Enter',
    tab: 'Tab',
    backspace: 'Backspace',
    delete: 'Delete',
    up: 'Up',
    down: 'Down',
    left: 'Left',
    right: 'Right',
    home: 'Home',
    end: 'End',
    pageup: 'PageUp',
    pagedown: 'PageDown',
    space: 'Space'
  }
  const parts = stroke.match(/^((?:(?:ctrl|alt|shift|cmd)-)*)(.+)$/)
  const mods = (parts?.[1] ?? '').split('-').filter(Boolean) as RuneModifier[]
  const key = parts?.[2] ?? stroke
  return { name: named[key] ?? key, mods }
}
