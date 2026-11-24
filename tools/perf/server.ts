/**
 * A throwaway troupe-server for the perf client lane: fixed token, its own
 * state dir, one seeded project and the tools/perf fake ACP agent,
 * registered over the server's own HTTP + WS surfaces.
 *
 * Mirrors the GPUIX perf server helper, folded flat because tools/parity is
 * a sibling unit's scope: `startTestServer` + `addProject` are inlined here.
 */

import { spawn } from 'node:child_process'
import { existsSync, openSync } from 'node:fs'
import { mkdir, rm } from 'node:fs/promises'
import net from 'node:net'
import path from 'node:path'
import { setTimeout as sleep } from 'node:timers/promises'
import { fileURLToPath } from 'node:url'
import { scopedKey } from '../../src/shared/types/persistence.types.ts'
import { WS_PROTOCOL_VERSION } from '../../src/shared/types/web-protocol.types.ts'
import { descendants, readCmdline } from './sampling/proc-sampler.ts'

export const TOOLKIT_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)))
export const REPO_ROOT = path.resolve(TOOLKIT_ROOT, '..', '..')

export const PERF_TOKEN = 'perf-token'
export const PERF_DEVICE_ID = 'perf-device'
export const PERF_AGENT_CONFIG_ID = 'custom-perfstub'
export const PERF_AGENT_NAME = 'Perf Stub Agent'
export const PERF_PROJECT_NAME = 'perf-project'

export interface PerfServer {
  baseUrl: string
  token: string
  stateDir: string
  /** The troupe-server process; the fake agents and PTYs it spawns are its children. */
  pid: number | null
  stop(): Promise<void>
}

const HEALTH_TIMEOUT_MS = 30_000

/** The troupe-server binary: `--server-bin`, $TROUPE_SERVER_BIN, or the worktree release build. */
export function defaultServerBinary(): string {
  const fromEnv = process.env.TROUPE_SERVER_BIN?.trim()
  if (fromEnv) return fromEnv
  const candidate = path.join(REPO_ROOT, 'src-tauri', 'target', 'release', 'troupe-server')
  if (existsSync(candidate)) return candidate
  throw new Error(
    `no troupe-server binary: set TROUPE_SERVER_BIN or pass --server-bin (looked for ${candidate})`
  )
}

/**
 * Fails when anything already listens on `port`. Without this, /health can
 * answer from another unit's server on the shared 8190-8199 range while ours
 * dies on EADDRINUSE, and the seeding below would write into that server.
 */
export async function assertPortFree(port: number): Promise<void> {
  const probe = net.createServer()
  const { promise, resolve, reject } = Promise.withResolvers<void>()
  probe.once('error', (err) => reject(new Error(`port ${port} is taken: ${err.message}`)))
  probe.listen(port, '127.0.0.1', () => probe.close(() => resolve()))
  await promise
}

async function waitForHealth(baseUrl: string, exited: Promise<number | null>): Promise<void> {
  const deadline = Date.now() + HEALTH_TIMEOUT_MS
  let gone = false
  void exited.then(() => {
    gone = true
  })
  while (Date.now() < deadline) {
    if (gone) throw new Error('troupe-server exited during startup (is the port taken?)')
    try {
      const res = await fetch(`${baseUrl}/health`)
      if (res.ok) return
    } catch {
      // not up yet
    }
    await sleep(200)
  }
  throw new Error(`troupe-server at ${baseUrl} never became healthy`)
}

/**
 * Spawn a throwaway troupe-server gated on the perf token, wait for /health,
 * then seed the perf world (project + fake agent).
 */
export async function startPerfServer(opts: {
  binary: string
  port: number
  stateRoot: string
  agentEnv: Record<string, string>
  token?: string
}): Promise<PerfServer> {
  await assertPortFree(opts.port)
  const token = opts.token ?? PERF_TOKEN
  const stateDir = path.join(opts.stateRoot, 'server')
  await rm(stateDir, { recursive: true, force: true })
  await mkdir(stateDir, { recursive: true })
  const baseUrl = `http://127.0.0.1:${opts.port}`
  const logFd = openSync(path.join(stateDir, 'server.log'), 'a')
  const child = spawn(
    opts.binary,
    [
      '--host',
      '127.0.0.1',
      '--port',
      String(opts.port),
      '--state-dir',
      stateDir,
      '--sessions-dir',
      path.join(stateDir, 'sessions'),
      '--projects-file',
      path.join(stateDir, 'projects.json'),
      '--store-file',
      path.join(stateDir, 'store.json'),
      '--web-auth-token',
      token
    ],
    {
      env: { ...process.env, RUST_LOG: 'termul_manager_lib=info' },
      stdio: ['ignore', logFd, logFd]
    }
  )
  const exited = new Promise<number | null>((resolvePromise) => {
    child.once('exit', (code) => resolvePromise(code))
  })
  const stop = async (): Promise<void> => {
    if (child.exitCode === null && child.signalCode === null) {
      child.kill('SIGTERM')
      const done = await Promise.race([exited.then(() => true), sleep(3000).then(() => false)])
      if (!done) {
        child.kill('SIGKILL')
        await exited
      }
    }
  }
  try {
    await waitForHealth(baseUrl, exited)
    const pid = descendants(process.pid).find(
      (p) => p !== process.pid && readCmdline(p).includes(path.basename(opts.binary))
    )
    const server: PerfServer = { baseUrl, token, stateDir, pid: pid ?? child.pid ?? null, stop }
    await seedPerfWorld(server, opts.agentEnv)
    return server
  } catch (err) {
    await stop()
    throw err
  }
}

/**
 * Seed the perf world into a server the caller started (or we spawned):
 * one project over HTTP plus the fake agent over the WS store — the same
 * routes and shapes the app itself uses.
 */
export async function seedPerfWorld(
  server: { baseUrl: string; token: string; stateDir?: string },
  agentEnv: Record<string, string>
): Promise<void> {
  const projectDir = path.join(server.stateDir ?? '/tmp/troupe-perf', PERF_PROJECT_NAME)
  await mkdir(projectDir, { recursive: true })
  await addProject(server, {
    id: 'perf-project-0001',
    name: PERF_PROJECT_NAME,
    path: projectDir,
    color: 'blue'
  })
  await seedFakeAgent(server, agentEnv)
}

async function serverFetch(
  server: { baseUrl: string; token: string },
  route: string,
  init: RequestInit = {}
): Promise<Response> {
  const res = await fetch(`${server.baseUrl}${route}`, {
    ...init,
    headers: {
      Authorization: `Bearer ${server.token}`,
      'content-type': 'application/json',
      ...(init.headers ?? {})
    }
  })
  if (!res.ok) throw new Error(`${init.method ?? 'GET'} ${route} failed: HTTP ${res.status}`)
  return res
}

/** Register a project over HTTP (the parity of the WS `add_project`). The folder is created. */
export async function addProject(
  server: { baseUrl: string; token: string },
  project: { id: string; name: string; path: string; color: string }
): Promise<void> {
  await mkdir(project.path, { recursive: true })
  await serverFetch(server, '/projects', { method: 'POST', body: JSON.stringify(project) })
}

async function seedFakeAgent(
  server: { baseUrl: string; token: string },
  env: Record<string, string>
): Promise<void> {
  // id equals configId, as the persistence migration backfills it. With the
  // two different, the web launcher's restore check (AgentLauncher #840/#907)
  // rejects the saved pick on its second mount and falls back to Codex.
  const config = {
    id: PERF_AGENT_CONFIG_ID,
    templateId: PERF_AGENT_CONFIG_ID,
    configId: PERF_AGENT_CONFIG_ID,
    name: PERF_AGENT_NAME,
    command: 'bun',
    args: [path.join(REPO_ROOT, 'tools', 'perf', 'fake-agent', 'fake-agent.ts')],
    env,
    allowTerminal: false
  }
  const agentsKey = scopedKey('acp/agents', PERF_DEVICE_ID)
  const current = await wsRequest<{ revision: number }>(server, 'store_read', { key: agentsKey })
  await wsRequest(server, 'store_write', {
    key: agentsKey,
    value: { _version: 1, data: [config] },
    baseRevision: current.revision ?? 0
  })
  await wsRequest(server, 'store_write', {
    key: scopedKey('agents/last-selected', PERF_DEVICE_ID),
    value: { _version: 1, data: { mode: 'acp', agentId: PERF_AGENT_CONFIG_ID } }
  })
}

interface ReplyFrame {
  id?: string
  ok?: boolean
  payload?: unknown
  err?: { code?: string; message?: string }
}

function wsRequest<T = unknown>(
  server: { baseUrl: string; token: string },
  type: string,
  payload: unknown
): Promise<T> {
  const socket = new WebSocket(`${server.baseUrl.replace(/^http/, 'ws')}/ws`)
  const { promise, resolve, reject } = Promise.withResolvers<T>()
  const timer = setTimeout(() => reject(new Error(`ws ${type} timed out`)), 15_000)
  socket.onopen = () => {
    socket.send(
      JSON.stringify({
        id: 'auth',
        type: 'authenticate',
        payload: {
          protocolVersion: WS_PROTOCOL_VERSION,
          deviceId: PERF_DEVICE_ID,
          clientInstanceId: 'perf-runner',
          token: server.token
        }
      })
    )
  }
  socket.onmessage = (event) => {
    const frame = JSON.parse(String(event.data)) as ReplyFrame
    if (frame.id === 'auth') {
      if (frame.ok) socket.send(JSON.stringify({ id: 'req', type, payload }))
      else reject(new Error(`ws authenticate refused: ${frame.err?.message}`))
      return
    }
    if (frame.id === 'req') {
      if (frame.ok) resolve(frame.payload as T)
      else reject(new Error(`ws ${type}: ${frame.err?.code} ${frame.err?.message}`))
    }
  }
  socket.onerror = () => reject(new Error(`ws ${type}: socket error`))
  return promise.finally(() => {
    clearTimeout(timer)
    socket.close()
  })
}
