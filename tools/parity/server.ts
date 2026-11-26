/**
 * A throwaway troupe-server for a matrix run: fixed token, its own state dir,
 * every persisted path pinned inside it (`seededServerArgs`, as the e2e suite
 * does), and HTTP seeding helpers.
 */

import { existsSync } from 'node:fs'
import { mkdir, rm } from 'node:fs/promises'
import net from 'node:net'
import path from 'node:path'
import { seededServerArgs } from '../../tests/e2e/seeded-server-args'

export const REPO_DIR = path.resolve(import.meta.dir, '../..')

export interface TestServer {
  baseUrl: string
  token: string
  stateDir: string
  /** The server's HOME, a throwaway dir inside `stateDir`. */
  home: string
  stop(): Promise<void>
}

export interface SeedProject {
  id: string
  name: string
  path: string
  color: string
}

const HEALTH_TIMEOUT_MS = 30_000

/** `TROUPE_SERVER_BIN` (or the older `TERMUL_SERVER_BIN`), else this checkout's release build. */
export function defaultServerBinary(): string {
  const fromEnv = (process.env.TROUPE_SERVER_BIN ?? process.env.TERMUL_SERVER_BIN)?.trim()
  const binary = fromEnv || path.join(REPO_DIR, 'src-tauri/target/release/troupe-server')
  if (!existsSync(binary)) {
    throw new Error(
      `no troupe-server at ${binary}: set TROUPE_SERVER_BIN or run ` +
        'cargo build --release --bin troupe-server --features standalone-server'
    )
  }
  return binary
}

async function waitForHealth(baseUrl: string, exited: Promise<number>): Promise<void> {
  const deadline = Date.now() + HEALTH_TIMEOUT_MS
  let gone = false
  void exited.then(() => {
    gone = true
  })
  while (Date.now() < deadline) {
    if (gone) throw new Error('troupe-server exited during startup (is the port taken?)')
    try {
      if ((await fetch(`${baseUrl}/health`)).ok) return
    } catch {
      // not listening yet
    }
    await Bun.sleep(200)
  }
  throw new Error(`troupe-server at ${baseUrl} never became healthy`)
}

export async function startTestServer(opts: {
  port: number
  token: string
  stateDir: string
  binary?: string
}): Promise<TestServer> {
  const binary = opts.binary ?? defaultServerBinary()
  await rm(opts.stateDir, { recursive: true, force: true })
  await mkdir(opts.stateDir, { recursive: true })
  const baseUrl = `http://127.0.0.1:${opts.port}`
  const logFile = Bun.file(path.join(opts.stateDir, 'server.log'))
  // A throwaway HOME keeps the host's own agents, skills and settings out of the frames.
  const home = path.join(opts.stateDir, 'home')
  await mkdir(home, { recursive: true })
  const child = Bun.spawn(
    [binary, ...seededServerArgs({ port: opts.port, stateDir: opts.stateDir })],
    {
      env: {
        ...process.env,
        HOME: home,
        XDG_CONFIG_HOME: path.join(home, '.config'),
        XDG_DATA_HOME: path.join(home, '.local/share'),
        XDG_STATE_HOME: path.join(home, '.local/state'),
        XDG_CACHE_HOME: path.join(home, '.cache'),
        TROUPE_WEB_AUTH_TOKEN: opts.token,
        RUST_LOG: 'termul_manager_lib=info'
      },
      stdout: logFile,
      stderr: logFile
    }
  )
  console.error(`[parity] troupe-server pid ${child.pid} on ${baseUrl}, log ${logFile.name}`)
  const stop = async (): Promise<void> => {
    if (child.exitCode !== null || child.signalCode !== null) return
    child.kill('SIGTERM')
    const exited = await Promise.race([
      child.exited.then(() => true),
      Bun.sleep(3000).then(() => false)
    ])
    if (!exited) {
      child.kill('SIGKILL')
      await child.exited
    }
  }
  try {
    await waitForHealth(baseUrl, child.exited)
  } catch (error) {
    await stop()
    throw error
  }
  return { baseUrl, token: opts.token, stateDir: opts.stateDir, home, stop }
}

/** An authenticated HTTP call to the server; throws on a non-2xx reply. */
export async function serverFetch(
  server: TestServer,
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
  if (!res.ok) {
    throw new Error(`${init.method ?? 'GET'} ${route}: HTTP ${res.status} ${await res.text()}`)
  }
  return res
}

/** Register a project over HTTP. The folder is created. */
export async function addProject(server: TestServer, project: SeedProject): Promise<void> {
  await mkdir(project.path, { recursive: true })
  await serverFetch(server, '/projects', { method: 'POST', body: JSON.stringify(project) })
}

const PORT_RANGE = [8190, 8199] as const

function isPortFree(port: number): Promise<boolean> {
  const probe = net.createServer()
  return new Promise((resolve) => {
    probe.once('error', () => resolve(false))
    probe.listen(port, '127.0.0.1', () => probe.close(() => resolve(true)))
  })
}

/** `requested` when it is free, else the first free port in 8190-8199. */
export async function pickPort(requested?: number): Promise<number> {
  if (requested !== undefined) {
    if (!(await isPortFree(requested))) throw new Error(`port ${requested} is taken`)
    return requested
  }
  for (let port = PORT_RANGE[0]; port <= PORT_RANGE[1]; port++) {
    if (await isPortFree(port)) return port
  }
  throw new Error(`no free port in ${PORT_RANGE.join('-')}`)
}
