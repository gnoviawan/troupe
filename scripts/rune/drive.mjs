#!/usr/bin/env node
// Client for the Rune live driver (apps/rune/crates/automation). Sends each
// command as one JSON line to the socket a `RUNE_DRIVE_SOCK=<path>` app serves,
// prints each reply line, and stops at the first reply with `ok: false`.
// `node scripts/rune/drive.mjs [--connect-timeout MS] <socket> '<json>'...`
// The socket appears before the window does, so the connection is retried
// until it succeeds or the timeout (default 60000 ms) passes.

import { createConnection } from 'node:net'
import { createInterface } from 'node:readline'
import { fileURLToPath } from 'node:url'

const RETRY_MS = 100

function connectOnce(path) {
  return new Promise((resolve, reject) => {
    const socket = createConnection(path)
    socket.once('connect', () => {
      socket.off('error', reject)
      resolve(socket)
    })
    socket.once('error', reject)
  })
}

export async function connect(path, { timeoutMs = 60000 } = {}) {
  const deadline = Date.now() + timeoutMs
  for (;;) {
    try {
      return await connectOnce(path)
    } catch (error) {
      const retryable = error.code === 'ENOENT' || error.code === 'ECONNREFUSED'
      if (!retryable || Date.now() >= deadline) throw error
      await new Promise((r) => setTimeout(r, RETRY_MS))
    }
  }
}

// One request, one reply, in order: replies resolve the oldest pending send.
export function session(socket) {
  const pending = []
  let closed = false
  const fail = (error) => {
    closed = true
    for (const p of pending.splice(0)) p.reject(error)
  }
  createInterface({ input: socket }).on('line', (line) => {
    const p = pending.shift()
    if (!p) return
    try {
      p.resolve(JSON.parse(line))
    } catch {
      p.reject(new Error(`bad reply: ${line}`))
    }
  })
  socket.on('close', () => fail(new Error('socket closed')))
  socket.on('error', fail)
  return {
    send(cmd) {
      if (closed) return Promise.reject(new Error('socket closed'))
      return new Promise((resolve, reject) => {
        pending.push({ resolve, reject })
        socket.write(`${JSON.stringify(cmd)}\n`)
      })
    },
    close() {
      socket.end()
    }
  }
}

// Sends `commands` in order and prints each reply. Returns false at the first
// reply that is not ok.
export async function drive(path, commands, { timeoutMs, print = console.log } = {}) {
  const s = session(await connect(path, { timeoutMs }))
  try {
    for (const cmd of commands) {
      const reply = await s.send(cmd)
      print(JSON.stringify(reply))
      if (!reply.ok) return false
    }
    return true
  } finally {
    s.close()
  }
}

export function parseArgs(argv) {
  const args = [...argv]
  let timeoutMs
  if (args[0] === '--connect-timeout') {
    args.shift()
    timeoutMs = Number(args.shift())
    if (!Number.isFinite(timeoutMs)) throw new Error('--connect-timeout needs a number of ms')
  }
  const [path, ...raw] = args
  if (!path || raw.length === 0)
    throw new Error('usage: drive.mjs [--connect-timeout MS] <socket> <json>...')
  const commands = raw.map((text) => {
    try {
      return JSON.parse(text)
    } catch {
      throw new Error(`not JSON: ${text}`)
    }
  })
  return { path, commands, timeoutMs }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  let args
  try {
    args = parseArgs(process.argv.slice(2))
  } catch (error) {
    console.error(error.message)
    process.exit(2)
  }
  drive(args.path, args.commands, { timeoutMs: args.timeoutMs }).then(
    (ok) => process.exit(ok ? 0 : 1),
    (error) => {
      console.error(`drive: ${error.message}`)
      process.exit(1)
    }
  )
}
