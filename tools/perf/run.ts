/**
 * Linux perf runner for the Troupe web client candidates: every run starts
 * Xvfb, a seeded throwaway troupe-server, a byte proxy and the client under
 * its own display, then walks the scenario's phases behind proc, thread,
 * byte, screen and page metrics.
 *
 *   node tools/perf/run.ts --scenarios stream-storm,pty-flood --runs 3
 *   node tools/perf/run.ts --clients chromium,chromium ...   (interleaved per run)
 *
 * `perf browser <scenario>` runs one client once through this engine;
 * `perf compare` runs two clients interleaved and prints the report.
 */
import { type ChildProcess, execFileSync, spawn } from 'node:child_process'
import { mkdirSync, writeFileSync } from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { setTimeout as sleep } from 'node:timers/promises'
import { fileURLToPath } from 'node:url'
import { RUNE_APPS, type RuneAppName } from '../parity/drivers/rune-apps.ts'
import { startByteProxy } from './byte-proxy.ts'
import {
  DEFAULTS,
  type Flags,
  type PhaseClock,
  SCENARIOS,
  type ScenarioName
} from './client-scenarios.ts'
import { chromiumDriver, launchChromium } from './clients/chromium.ts'
import type {
  ChatStartPart,
  ClientDriver,
  ClientKind,
  ClientLaunchOptions,
  ClientTreeCensus
} from './clients/client.ts'
import {
  launchRuneDesktop,
  launchRuneWeb,
  runeDesktopDriver,
  runeDesktopRole,
  runeWebDriver
} from './clients/rune.ts'
import {
  chromiumRole,
  type MachineSample,
  type ProcSample,
  type RoleOf,
  startMachineSampler,
  startProcSampler,
  waitForQuiet,
  withPageRenderer
} from './sampling/proc-sampler.ts'
import { type ScreenLog, startScreenLog, summarizeScreen } from './sampling/screen-frames.ts'
import { snapshotThreads, type ThreadSnapshot, threadCpu } from './sampling/thread-sampler.ts'
import { defaultServerBinary, type PerfServer, seedPerfWorld, startPerfServer } from './server.ts'
import { aggregate, median, type Numbers, summarizePage, summarizeProc } from './stats.ts'
import { type PageMetrics, type PerfRunResult, summarize } from './types.ts'

export const TOOLKIT_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)))
export const REPO_ROOT = path.resolve(TOOLKIT_ROOT, '..', '..')
const DIST_WEB = path.join(REPO_ROOT, 'dist-web')

export interface ClientSpec {
  kind: ClientKind
  /** Run-set directory and report column label (`chromium:a` → label `a`). */
  label: string
}

interface Options {
  scenarios: ScenarioName[]
  runs: number
  clients: ClientSpec[]
  gl: 'lavapipe' | 'swiftshader' | 'cpu'
  /** Spawn this troupe-server per run... */
  serverBin: string | null
  /** ...or attach to this already-running server (still seeded, still proxied). */
  serverUrl: string | null
  token: string
  port: number
  display: string
  width: number
  height: number
  seed: number
  /** Wait before each run until the machine's busy CPU drops under this (% of one core). */
  quietBelowPct: number
  quietWaitS: number
  noPageMetrics: boolean
  /** Log X DAMAGE events on the run's display for screen frame timing. */
  screenFrames: boolean
  /** Per-run server state, client state and probe files go under here. */
  stateRoot: string
  distWeb: string
  /** The Rune app the Rune clients open (`--rune-app`). */
  runeApp: RuneAppName
  outDir: string
  overrides: Flags
}

const CLIENT_KINDS: ClientKind[] = ['chromium', 'rune-web', 'rune-desktop']

/** `kind[:label]` — a bare kind labels itself; a duplicated kind needs labels. */
function parseClientSpec(raw: string): ClientSpec {
  const [kind, label] = raw.split(':')
  if (!CLIENT_KINDS.includes(kind as ClientKind)) {
    throw new Error(`unknown client ${kind} (have: ${CLIENT_KINDS.join(', ')})`)
  }
  return { kind: kind as ClientKind, label: label || kind }
}

export function parseRunArgs(argv: string[]): Options {
  const raw: Record<string, string> = {}
  for (let i = 0; i < argv.length; i++) {
    if (!argv[i].startsWith('--')) continue
    raw[argv[i].slice(2)] = argv[i + 1]
    i++
  }
  const take = (key: string, fallback: string) => {
    const v = raw[key] ?? fallback
    delete raw[key]
    return v
  }
  const stamp = new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19)
  const serverUrl = raw.server ? take('server', '').replace(/\/$/, '') : null
  const opts: Options = {
    scenarios: take('scenarios', 'launch').split(',') as ScenarioName[],
    runs: Number(take('runs', '3')),
    clients: take('clients', 'chromium').split(',').map(parseClientSpec),
    gl: take('gl', 'lavapipe') as 'lavapipe' | 'swiftshader' | 'cpu',
    serverBin: serverUrl ? null : take('server-bin', '') || defaultServerBinary(),
    serverUrl,
    token: take('token', 'perf-token'),
    port: Number(take('port', serverUrl ? new URL(serverUrl).port || '8190' : '8196')),
    display: take('display', ':181'),
    width: Number(take('width', '1440')),
    height: Number(take('height', '900')),
    seed: Number(take('seed', '42')),
    quietBelowPct: Number(take('quiet-below', '150')),
    quietWaitS: Number(take('quiet-wait', '600')),
    noPageMetrics: take('page-metrics', 'on') === 'off',
    screenFrames: take('screen-frames', 'on') !== 'off',
    stateRoot: path.resolve(
      take('state-root', process.env.TROUPE_PERF_STATE ?? '/tmp/troupe-perf')
    ),
    distWeb: path.resolve(take('dist-web', DIST_WEB)),
    runeApp: take('rune-app', 'hello') as RuneAppName,
    outDir: path.resolve(take('out', path.join(TOOLKIT_ROOT, 'results', `linux-${stamp}`))),
    overrides: {}
  }
  for (const s of opts.scenarios) if (!(s in SCENARIOS)) throw new Error(`unknown scenario ${s}`)
  if (!(opts.runeApp in RUNE_APPS)) throw new Error(`unknown --rune-app ${opts.runeApp}`)
  const labels = new Set(opts.clients.map((c) => c.label))
  if (labels.size !== opts.clients.length) {
    // Same kind twice without labels: suffix them a, b, ... so run dirs differ.
    const seen = new Map<string, number>()
    for (const c of opts.clients) {
      const n = seen.get(c.label) ?? 0
      seen.set(c.label, n + 1)
      c.label = `${c.label}-${'abcdefghijklmnopqrstuvwxyz'[n]}`
    }
  }
  for (const [k, v] of Object.entries(raw)) opts.overrides[k] = Number(v)
  return opts
}

/** troupe-server, its spawned fake agents and the in-terminal probes. */
const serverRole: RoleOf = (_pid, cmd) => {
  if (cmd.includes('troupe-server')) return 'server'
  if (cmd.includes('fake-agent')) return 'agent'
  if (cmd.includes('terminal-probe')) return 'probe'
  return 'other'
}

async function startXvfb(display: string, width: number, height: number): Promise<ChildProcess> {
  const xvfb = spawn(
    'Xvfb',
    [display, '-screen', '0', `${width}x${height}x24`, '-nolisten', 'tcp'],
    {
      stdio: 'ignore'
    }
  )
  await sleep(700)
  if (xvfb.exitCode !== null) throw new Error(`Xvfb ${display} exited (display in use?)`)
  return xvfb
}

const within = <T extends { t: number }>(samples: T[], from: number, to: number) =>
  samples.filter((s) => s.t >= from && s.t <= to)

interface PhaseWindow {
  name: string
  from: number
  to: number
  bytes: { down: number; up: number }
  page: Numbers
  threads?: { before: ThreadSnapshot; after: ThreadSnapshot }
}

export interface RunRecord {
  machine: Record<string, unknown>
  /** Raw in-page metrics per phase, kept in memory for result.json; stripped
   * before the record hits disk (frame deltas are megabytes on long runs). */
  rawPhases?: Array<{
    name: string
    startedAtMs: number
    durationMs: number
    page: PageMetrics | null
    gaps: string[]
  }>
  meta: {
    scenario: string
    run: number
    client: string
    status: string
    preRunBusyPct: number
    quietWaitS: number
    load1Before: number
    [key: string]: unknown
  }
  metrics: Record<string, number>
  errors: string[]
  notes: Record<string, unknown>
  processes: Array<{ pid: number; role: string; cmd: string }>
  procSeries: unknown[]
}

/**
 * The server side of one run: spawn ours, or attach to a running one. Either
 * way the perf world (project + fake agent) is seeded and the client reaches
 * it through the byte proxy so the traffic is counted.
 */
async function upServer(
  opts: Options,
  scratch: string,
  flags: Flags
): Promise<{
  server: PerfServer
  proxy: {
    url: string
    counters(): { down: number; up: number }
    reset(): void
    close(): Promise<void>
  }
}> {
  const agentEnv = {
    PERF_AGENT_SEED: String(opts.seed),
    PERF_AGENT_RATE: String(flags.rate ?? 20),
    PERF_AGENT_CHUNK_CHARS: '120',
    PERF_AGENT_UNIQUE_SESSIONS: '1'
  }
  if (opts.serverUrl) {
    const server: PerfServer = {
      baseUrl: opts.serverUrl,
      token: opts.token,
      stateDir: path.join(scratch, 'server'),
      pid: null,
      stop: async () => {}
    }
    await seedPerfWorld(server, agentEnv)
    const proxy = await startByteProxy(opts.port, opts.port + 1)
    return { server, proxy }
  }
  const server = await startPerfServer({
    binary: opts.serverBin ?? defaultServerBinary(),
    port: opts.port,
    stateRoot: scratch,
    agentEnv,
    token: opts.token
  })
  const proxy = await startByteProxy(opts.port, opts.port + 1).catch(async (err) => {
    await server.stop()
    throw err
  })
  return { server, proxy }
}

async function runOnce(
  opts: Options,
  machine: Record<string, unknown>,
  client: ClientSpec,
  scenario: ScenarioName,
  runIndex: number
): Promise<RunRecord> {
  const flags: Flags = { ...DEFAULTS[scenario], ...opts.overrides }
  // No dashes: the client types probe paths as keystrokes.
  const scratch = path.join(
    opts.stateRoot,
    `${client.label}${scenario.replace(/-/g, '')}${runIndex}`
  )
  mkdirSync(scratch, { recursive: true })
  const errors: string[] = []
  const notes: Record<string, unknown> = {}
  const windows: PhaseWindow[] = []
  const rawPhases: NonNullable<RunRecord['rawPhases']> = []
  const census: Record<string, ClientTreeCensus> = {}
  notes.census = census
  const quiet = await waitForQuiet(opts.quietBelowPct, opts.quietWaitS)
  const load1Before = os.loadavg()[0]

  const xvfb = await startXvfb(opts.display, opts.width, opts.height)
  let screen: ScreenLog | null = null
  if (opts.screenFrames) {
    screen = await startScreenLog(opts.display).catch((err) => {
      errors.push(`screen frames: ${String(err).split('\n')[0]}`)
      return null
    })
  }
  const { server, proxy } = await upServer(opts, scratch, flags).catch(async (err) => {
    await screen?.stop()
    xvfb.kill()
    throw err
  })
  const serverSampler = server.pid !== null ? startProcSampler(server.pid, serverRole) : null
  const machineSampler = startMachineSampler()
  let driver: ClientDriver | null = null
  let sampler: ReturnType<typeof startProcSampler> | null = null
  let clientSamples: ProcSample[] = []
  let serverSamples: ProcSample[] = []
  let machineSamples: MachineSample[] = []
  let processes: Array<{ pid: number; role: string; cmd: string }> = []
  const startup: Numbers = {}
  try {
    const t0 = performance.now()
    const wall0 = Date.now()
    const launchOpts: ClientLaunchOptions = {
      url: proxy.url,
      display: opts.display,
      width: opts.width,
      height: opts.height,
      distWeb: opts.distWeb,
      gl: opts.gl,
      noPageMetrics: opts.noPageMetrics,
      runeApp: opts.runeApp
    }
    switch (client.kind) {
      case 'chromium': {
        const handle = await launchChromium(launchOpts)
        sampler = startProcSampler(handle.rootPid, chromiumRole)
        driver = chromiumDriver(handle)
        break
      }
      case 'rune-web': {
        const handle = await launchRuneWeb(launchOpts)
        sampler = startProcSampler(handle.rootPid, chromiumRole)
        driver = runeWebDriver(handle, opts.runeApp)
        break
      }
      case 'rune-desktop': {
        const handle = await launchRuneDesktop(launchOpts)
        sampler = startProcSampler(handle.rootPid, runeDesktopRole)
        driver = runeDesktopDriver(handle, opts.runeApp)
        break
      }
    }
    const handle = driver.handle
    notes.client = handle.version
    startup['startup.processUpMs'] = Math.round(handle.launchedMs)
    await driver.waitForProjectList()
    startup['startup.projectListMs'] = Math.round(performance.now() - t0)
    await driver.waitForLauncher()
    startup['startup.launcherReadyMs'] = Math.round(performance.now() - t0)
    // A Rune web client knows its renderer only once the GPU hand-over settled.
    notes.backend = handle.backend
    const bootPage = await driver.endPhase('boot')
    const bootTo = Date.now()
    windows.push({
      name: 'startup',
      from: wall0,
      to: bootTo,
      bytes: proxy.counters(),
      page: bootPage && !opts.noPageMetrics ? summarizePage(bootPage, bootTo - wall0) : {}
    })
    rawPhases.push({
      name: 'startup',
      startedAtMs: 0,
      durationMs: bootTo - wall0,
      page: bootPage,
      gaps: []
    })

    const d = driver
    const clock: PhaseClock = {
      phase: async (name, body) => {
        await d.beginPhase(name)
        proxy.reset()
        const before = snapshotThreads(d.handle.rootPid)
        const from = Date.now()
        await body()
        const to = Date.now()
        const after = snapshotThreads(d.handle.rootPid)
        const bytes = proxy.counters()
        const page = await d.endPhase(name)
        const tree = await d.census?.().catch((err: unknown) => {
          errors.push(`census ${name}: ${String(err).split('\n')[0]}`)
          return undefined
        })
        if (tree) census[name] = tree
        windows.push({
          name,
          from,
          to,
          bytes,
          page: page && !opts.noPageMetrics ? summarizePage(page, to - from) : {},
          threads: { before, after }
        })
        rawPhases.push({
          name,
          startedAtMs: from - wall0,
          durationMs: to - from,
          page,
          gaps: []
        })
      },
      note: (key, value) => {
        notes[key] = value
      },
      error: (message) => {
        errors.push(message)
      }
    }
    await SCENARIOS[scenario]({ driver: d, clock, flags, seed: opts.seed, scratch })
    notes.chatStartParts = d.chatStartParts()
  } catch (err) {
    errors.push(`fatal: ${err instanceof Error ? err.stack : String(err)}`)
  } finally {
    const crash = await driver?.crashed().catch(() => null)
    if (crash) errors.push(`fatal: app crashed: ${crash}`)
    if (sampler) {
      clientSamples = sampler.stop()
      processes = sampler.processes()
      const roleOfPid = new Map(processes.map((p) => [p.pid, p.role]))
      if (client.kind !== 'rune-desktop')
        notes.pageRendererPid = withPageRenderer(clientSamples, roleOfPid)
      const pageRenderer = notes.pageRendererPid
      const role = (pid: number) =>
        pid === pageRenderer ? 'page-renderer' : (roleOfPid.get(pid) ?? 'other')
      notes.threads = Object.fromEntries(
        windows.flatMap((w) =>
          w.threads ? [[w.name, threadCpu(w.threads.before, w.threads.after, role)]] : []
        )
      )
    }
    await driver?.handle.close()
    await screen?.stop()
    await proxy.close()
    serverSamples = serverSampler?.stop() ?? []
    machineSamples = machineSampler.stop()
    await server.stop()
    xvfb.kill()
    notes.serverSamples = serverSamples.length
    notes.serverProc = Object.fromEntries(
      windows.map((w) => [w.name, summarizeProc(within(serverSamples, w.from, w.to), 'server.')])
    )
  }

  const metrics: Numbers = { ...startup }
  for (const w of windows) {
    const seconds = (w.to - w.from) / 1000
    Object.assign(
      metrics,
      prefixed(w.name, summarizeProc(within(clientSamples, w.from, w.to), 'proc.'))
    )
    Object.assign(metrics, prefixed(w.name, w.page))
    if (screen)
      Object.assign(
        metrics,
        prefixed(w.name, summarizeScreen(screen.frames(w.from, w.to), w.to - w.from))
      )
    const serverCpu =
      summarizeProc(within(serverSamples, w.from, w.to), 'server.')['server.total.cpuMeanPct'] ?? 0
    const machine = within(machineSamples, w.from, w.to).map((m) => m.busy)
    const machineCpu = machine.length ? machine.reduce((a, b) => a + b, 0) / machine.length : 0
    const clientCpu = metrics[`${w.name}.proc.total.cpuMeanPct`] ?? 0
    metrics[`${w.name}.server.cpuMeanPct`] = serverCpu
    metrics[`${w.name}.machine.cpuBusyPct`] = Math.round(machineCpu)
    metrics[`${w.name}.machine.foreignCpuPct`] = Math.round(
      Math.max(0, machineCpu - clientCpu - serverCpu)
    )
    metrics[`${w.name}.durationS`] = Math.round(seconds * 10) / 10
    metrics[`${w.name}.bytesDownKBps`] = Math.round(w.bytes.down / 1024 / seconds)
    metrics[`${w.name}.bytesUpKBps`] = Math.round(w.bytes.up / 1024 / seconds)
    const tree = census[w.name]
    if (tree) {
      metrics[`${w.name}.tree.reachable`] = tree.reachable
      metrics[`${w.name}.tree.painted`] = tree.painted
      metrics[`${w.name}.tree.hidden`] = tree.hidden
    }
  }
  Object.assign(metrics, probeMetrics(notes))
  if (typeof notes.chatsOpen === 'number') metrics.chatsOpen = notes.chatsOpen
  const chatStart = notes.chatStartMs as number[] | undefined
  if (chatStart?.length) {
    metrics['chat.firstStartMs'] = chatStart[0]
    const loaded = chatStart.slice(1)
    if (loaded.length) {
      metrics['chat.startUnderLoadP50Ms'] = median(loaded)
      metrics['chat.startUnderLoadMaxMs'] = Math.max(...loaded)
    }
  }
  const parts = notes.chatStartParts as ChatStartPart[] | undefined
  if (parts && parts.length > 1) {
    metrics['chat.typeUnderLoadP50Ms'] = median(parts.slice(1).map((p) => p.typeMs))
    metrics['chat.openUnderLoadP50Ms'] = median(parts.slice(1).map((p) => p.openMs))
  }
  if (typeof notes.switches === 'number') metrics['chat.switches'] = notes.switches
  return {
    machine,
    rawPhases,
    meta: {
      scenario,
      run: runIndex,
      client: client.label,
      clientVersion: notes.client,
      backend: notes.backend,
      gl: opts.gl,
      pageMetrics: client.kind !== 'rune-desktop' && !opts.noPageMetrics,
      runeApp: client.kind === 'chromium' ? undefined : opts.runeApp,
      screenFrames: screen !== null,
      seed: opts.seed,
      flags,
      viewport: `${opts.width}x${opts.height}`,
      startedAt: new Date().toISOString(),
      gitCommit: execFileSync('git', ['rev-parse', 'HEAD'], { cwd: REPO_ROOT }).toString().trim(),
      quietWaitS: quiet.waitedS,
      preRunBusyPct: quiet.busyPct,
      load1Before,
      load1After: os.loadavg()[0],
      status: errors.some((e) => e.startsWith('fatal')) ? 'failed' : 'ok'
    },
    metrics,
    errors,
    notes,
    processes,
    procSeries: clientSamples.filter((_, i) => i % 2 === 0).map(({ pids: _pids, ...rest }) => rest)
  }
}

function prefixed(phase: string, values: Numbers): Numbers {
  return Object.fromEntries(Object.entries(values).map(([k, v]) => [`${phase}.${k}`, v]))
}

interface PacedResult {
  lines: number
  seconds: number
  bytes: number
  rttMs: Array<number | null>
}
interface BurstResult {
  lines: number
  bytes: number
  totalMs: number | null
  writeMs: number
}

function probeMetrics(notes: Record<string, unknown>): Numbers {
  const out: Numbers = {}
  const paced = notes.paced as PacedResult[] | undefined
  if (paced) {
    const rtts = paced.flatMap((p) => p.rttMs)
    const ok = rtts.filter((r): r is number => r !== null).sort((a, b) => a - b)
    out['probe.pacedRttP50Ms'] = Math.round(median(ok) * 10) / 10
    out['probe.pacedRttP95Ms'] = Math.round((ok[Math.round(0.95 * (ok.length - 1))] ?? 0) * 10) / 10
    out['probe.pacedRttMaxMs'] = Math.round((ok[ok.length - 1] ?? 0) * 10) / 10
    out['probe.pacedTimeouts'] = rtts.length - ok.length
    out['probe.pacedProbes'] = rtts.length
    out['probe.pacedLinesPerSecPerTerminal'] = Math.round(
      median(paced.map((p) => p.lines / p.seconds))
    )
  }
  const burst = notes.burst as BurstResult | undefined
  if (burst) {
    out['probe.burstLines'] = burst.lines
    out['probe.burstMB'] = Math.round((burst.bytes / 1024 / 1024) * 100) / 100
    if (burst.totalMs !== null) {
      out['probe.burstTotalMs'] = Math.round(burst.totalMs)
      out['probe.burstMBps'] =
        Math.round((burst.bytes / 1024 / 1024 / (burst.totalMs / 1000)) * 100) / 100
    }
    out['probe.burstWriteMs'] = Math.round(burst.writeMs)
  }
  return out
}

export interface RunSetResult {
  outDir: string
  /** label → scenario → per-run metrics, ok runs only. */
  byClient: Map<string, Record<string, Numbers[]>>
  /** In-memory records (rawPhases kept); disk copies have them stripped. */
  records: RunRecord[]
}

/** A RunRecord in the classic PerfRunResult shape, for result.json + report.html. */
export function toPerfRunResult(record: RunRecord, appTarget: string): PerfRunResult {
  const meta = record.meta
  const result: PerfRunResult = {
    meta: {
      runId: `linux-${meta.client}-${meta.scenario}-run${meta.run}`,
      scenario: meta.scenario,
      startedAt: typeof meta.startedAt === 'string' ? meta.startedAt : new Date().toISOString(),
      params: (meta.flags as Record<string, string | number | boolean>) ?? {},
      seed: typeof meta.seed === 'number' ? meta.seed : 0,
      lane: 'browser',
      appTarget,
      gitCommit: typeof meta.gitCommit === 'string' ? meta.gitCommit : undefined,
      reactProfiling: false,
      status: meta.status === 'failed' ? 'failed' : 'ok',
      failure: record.errors.find((e) => e.startsWith('fatal')),
      machine: JSON.stringify(record.machine)
    },
    phases: (record.rawPhases ?? []).map((p) => ({
      name: p.name,
      startedAtMs: p.startedAtMs,
      durationMs: p.durationMs,
      page: p.page ?? undefined,
      gaps: p.gaps
    })),
    summary: {}
  }
  result.summary = summarize(result)
  return result
}

/**
 * Interleaved: run 1 of every scenario on every client, then run 2, so
 * drift and other units' load hit every cell alike. Even runs reverse the
 * client order, so neither client always goes first.
 */
export async function runClientComparison(opts: Options): Promise<RunSetResult> {
  mkdirSync(opts.outDir, { recursive: true })
  const machine = {
    cpu: os.cpus()[0]?.model,
    cores: os.cpus().length,
    memGB: Math.round(os.totalmem() / 1024 ** 3),
    kernel: os.release(),
    node: process.version
  }
  // One client keeps a flat layout; several get a directory each, so
  // runs-report can take them as separate columns.
  const dirOf = (client: ClientSpec) =>
    opts.clients.length > 1 ? path.join(opts.outDir, client.label) : opts.outDir
  const byClient = new Map<string, Record<string, Numbers[]>>()
  const records: RunRecord[] = []
  for (const client of opts.clients) {
    mkdirSync(dirOf(client), { recursive: true })
    byClient.set(client.label, {})
  }
  const writeAggregate = (client: ClientSpec) => {
    const byScenario = byClient.get(client.label) ?? {}
    const summary = {
      machine,
      client: client.label,
      gl: opts.gl,
      runs: opts.runs,
      scenarios: Object.fromEntries(
        Object.entries(byScenario).map(([s, runs]) => [
          s,
          { okRuns: runs.length, metrics: aggregate(runs) }
        ])
      )
    }
    writeFileSync(path.join(dirOf(client), 'aggregate.json'), JSON.stringify(summary, null, 1))
  }
  for (let run = 1; run <= opts.runs; run++) {
    const order = run % 2 === 0 ? [...opts.clients].reverse() : opts.clients
    for (const scenario of opts.scenarios) {
      for (const client of order) {
        console.log(`[perf] ${client.label} ${scenario} run ${run}/${opts.runs}`)
        const record = await runOnce(opts, machine, client, scenario, run)
        records.push(record)
        writeFileSync(
          path.join(dirOf(client), `${scenario}-run${run}.json`),
          JSON.stringify({ ...record, machine, rawPhases: undefined })
        )
        console.log(
          `[perf]   status=${record.meta.status} errors=${record.errors.length} preRunBusy=${record.meta.preRunBusyPct}% waited=${record.meta.quietWaitS}s load1=${record.meta.load1Before.toFixed(1)}`
        )
        for (const e of record.errors) console.log(`[perf]   ! ${e.split('\n')[0]}`)
        if (record.meta.status === 'ok') {
          const byScenario = byClient.get(client.label) ?? {}
          byScenario[scenario] ??= []
          byScenario[scenario].push(record.metrics)
        }
        writeAggregate(client)
      }
    }
  }
  console.log(`[perf] wrote ${opts.outDir}`)
  return { outDir: opts.outDir, byClient, records }
}

if (import.meta.main ?? process.argv[1] === import.meta.filename) {
  const opts = parseRunArgs(process.argv.slice(2))
  await runClientComparison(opts)
  process.exit(0)
}
