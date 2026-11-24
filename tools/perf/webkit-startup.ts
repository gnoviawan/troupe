/**
 * Startup and idle memory of the shipped Linux desktop app (Tauri on
 * WebKitGTK), the client that users run today — the perf reference for the
 * Tauri window on Linux.
 *
 *   node tools/perf/webkit-startup.ts --bin <troupe> --runs 3
 *   bun run perf webkit-startup -- --bin <troupe> --runs 3
 *
 * WebKitGTK has no automation hook here, so readiness is read from pixels:
 * the run polls the X display until the seeded project's avatar (the
 * `blue` project colour) shows in the sidebar.
 */
import { type ChildProcess, spawn, spawnSync } from 'node:child_process'
import { mkdirSync, rmSync, writeFileSync } from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { setTimeout as sleep } from 'node:timers/promises'
import { fileURLToPath } from 'node:url'
import { parseArgs } from 'node:util'
import {
  type ProcSample,
  type RoleOf,
  startMachineSampler,
  startProcSampler,
  waitForQuiet
} from './sampling/proc-sampler.ts'
import { aggregate, type Numbers, summarizeProc } from './stats.ts'

const TOOLKIT_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)))
const REPO_ROOT = path.resolve(TOOLKIT_ROOT, '..', '..')

const IDENTIFIER = 'com.termul-manager.app.dev'
/** Project colour `blue` as the sidebar avatar paints it (measured in Chromium). */
const AVATAR_RGB = [60, 131, 246]
const AVATAR_MIN_PIXELS = 100

interface WebkitOptions {
  bin: string
  runs: number
  display: string
  width: number
  height: number
  settleS: number
  idleS: number
  out: string
  stateRoot: string
  quietBelow: number
  quietWait: number
}

export function parseWebkitArgs(argv: string[]): WebkitOptions {
  const { values } = parseArgs({
    args: argv,
    options: {
      bin: {
        type: 'string',
        default: path.join(REPO_ROOT, 'src-tauri', 'target', 'release', 'troupe')
      },
      runs: { type: 'string', default: '3' },
      display: { type: 'string', default: ':182' },
      width: { type: 'string', default: '1440' },
      height: { type: 'string', default: '900' },
      settle: { type: 'string', default: '20' },
      idle: { type: 'string', default: '20' },
      out: { type: 'string', default: path.join(TOOLKIT_ROOT, 'results', 'webkit-startup') },
      'state-root': { type: 'string', default: '/tmp/troupe-perf/webkit' },
      'quiet-below': { type: 'string', default: '150' },
      'quiet-wait': { type: 'string', default: '600' }
    }
  })
  return {
    bin: values.bin!,
    runs: Number(values.runs),
    display: values.display!,
    width: Number(values.width),
    height: Number(values.height),
    settleS: Number(values.settle),
    idleS: Number(values.idle),
    out: values.out!,
    stateRoot: values['state-root']!,
    quietBelow: Number(values['quiet-below']),
    quietWait: Number(values['quiet-wait'])
  }
}

const role: RoleOf = (_pid, cmd) => {
  if (cmd.includes('WebKitWebProcess')) return 'web-process'
  if (cmd.includes('WebKitNetworkProcess')) return 'network'
  if (cmd.includes('WebKitGPUProcess')) return 'gpu-process'
  if (cmd.includes('troupe') || cmd.includes('termul-manager')) return 'app'
  return 'other'
}

function grabRgb(opts: WebkitOptions): Buffer | null {
  const res = spawnSync(
    'ffmpeg',
    [
      '-loglevel',
      'error',
      '-f',
      'x11grab',
      '-video_size',
      `${opts.width}x${opts.height}`,
      '-i',
      opts.display,
      '-frames:v',
      '1',
      '-f',
      'rawvideo',
      '-pix_fmt',
      'rgb24',
      '-'
    ],
    { maxBuffer: opts.width * opts.height * 4 }
  )
  return res.status === 0 ? res.stdout : null
}

function avatarPixels(rgb: Buffer, width: number, height: number): number {
  let n = 0
  for (let y = 0; y < Math.min(500, height); y++) {
    for (let x = 0; x < 240; x++) {
      const i = (y * width + x) * 3
      if (
        Math.abs(rgb[i] - AVATAR_RGB[0]) < 12 &&
        Math.abs(rgb[i + 1] - AVATAR_RGB[1]) < 12 &&
        Math.abs(rgb[i + 2] - AVATAR_RGB[2]) < 12
      )
        n++
    }
  }
  return n
}

const painted = (rgb: Buffer) => rgb.some((v) => v > 16)

async function runOnce(opts: WebkitOptions, index: number) {
  const home = path.join(opts.stateRoot, `run${index}`)
  rmSync(home, { recursive: true, force: true })
  const dataDir = path.join(home, '.local/share', IDENTIFIER)
  const projectDir = path.join(home, 'perf-project')
  mkdirSync(dataDir, { recursive: true })
  mkdirSync(projectDir, { recursive: true })
  writeFileSync(
    path.join(dataDir, 'projects.json'),
    JSON.stringify({
      schemaVersion: 4,
      defaultProjectId: 'perf-project-0001',
      projects: [
        {
          id: 'perf-project-0001',
          name: 'perf-project',
          path: projectDir,
          color: 'blue',
          isArchived: false,
          mcpServers: []
        }
      ],
      imported: false
    })
  )
  const xvfb = spawn(
    'Xvfb',
    [opts.display, '-screen', '0', `${opts.width}x${opts.height}x24`, '-nolisten', 'tcp'],
    { stdio: 'ignore' }
  )
  await sleep(700)
  if (xvfb.exitCode !== null) throw new Error(`Xvfb ${opts.display} exited (display in use?)`)
  const machine = startMachineSampler()
  const t0 = performance.now()
  const app: ChildProcess = spawn(opts.bin, [], {
    stdio: ['ignore', 'ignore', 'pipe'],
    env: {
      ...process.env,
      DISPLAY: opts.display,
      HOME: home,
      XDG_DATA_HOME: path.join(home, '.local/share'),
      XDG_CONFIG_HOME: path.join(home, '.config'),
      XDG_CACHE_HOME: path.join(home, '.cache'),
      XDG_STATE_HOME: path.join(home, '.local/state')
    }
  })
  let stderr = ''
  app.stderr?.on('data', (c) => {
    stderr = (stderr + String(c)).slice(-4000)
  })
  const sampler = startProcSampler(app.pid!, role)
  const metrics: Numbers = {}
  const errors: string[] = []
  try {
    const deadline = t0 + 90_000
    while (performance.now() < deadline) {
      if (app.exitCode !== null) throw new Error(`troupe exited ${app.exitCode}: ${stderr}`)
      const rgb = grabRgb(opts)
      const at = Math.round(performance.now() - t0)
      if (rgb && painted(rgb) && metrics.firstPaintMs === undefined) metrics.firstPaintMs = at
      if (rgb && avatarPixels(rgb, opts.width, opts.height) >= AVATAR_MIN_PIXELS) {
        metrics.projectListMs = at
        break
      }
      await sleep(100)
    }
    if (metrics.projectListMs === undefined) errors.push('project row never appeared')
    const shot = path.join(opts.out, `webkit-run${index}.png`)
    spawnSync('ffmpeg', [
      '-loglevel',
      'error',
      '-y',
      '-f',
      'x11grab',
      '-video_size',
      `${opts.width}x${opts.height}`,
      '-i',
      opts.display,
      '-frames:v',
      '1',
      shot
    ])
    // The desktop app probes installed agents (login shell, `--version`
    // calls) right after boot; the idle window starts once that settles.
    await sleep(opts.settleS * 1000)
    const idleFrom = Date.now()
    await sleep(opts.idleS * 1000)
    const samples = sampler.stop()
    const idle = samples.filter((s: ProcSample) => s.t >= idleFrom)
    Object.assign(metrics, summarizeProc(idle, 'idle.proc.'))
    const busy = machine
      .stop()
      .filter((m) => m.t >= idleFrom)
      .map((m) => m.busy)
    metrics['idle.machine.cpuBusyPct'] = Math.round(
      busy.reduce((a, b) => a + b, 0) / Math.max(1, busy.length)
    )
    return { run: index, metrics, errors, processes: sampler.processes(), shot }
  } finally {
    app.kill('SIGTERM')
    await sleep(1500)
    if (app.exitCode === null) app.kill('SIGKILL')
    xvfb.kill()
  }
}

export async function webkitStartup(argv: string[]): Promise<void> {
  const opts = parseWebkitArgs(argv)
  mkdirSync(opts.out, { recursive: true })
  const webkit = spawnSync('dpkg-query', [
    '-W',
    // biome-ignore lint/suspicious/noTemplateCurlyInString: dpkg-query format, not a template
    '-f=${Version}',
    'libwebkit2gtk-4.1-0'
  ]).stdout.toString()
  const runs = []
  for (let i = 1; i <= opts.runs; i++) {
    const quiet = await waitForQuiet(opts.quietBelow, opts.quietWait)
    const load1Before = os.loadavg()[0]
    const record = { ...(await runOnce(opts, i)), quiet: { ...quiet, load1Before } }
    console.log(
      `[webkit] run ${i}: projectListMs=${record.metrics.projectListMs} pss=${record.metrics['idle.proc.total.pssMedianMB']} errors=${record.errors.length} preRunBusy=${quiet.busyPct}% load1=${load1Before.toFixed(1)}`
    )
    runs.push(record)
  }
  const summary = {
    machine: { cpu: os.cpus()[0]?.model, cores: os.cpus().length, kernel: os.release() },
    client: `Tauri desktop (${path.basename(opts.bin)}) on WebKitGTK ${webkit}`,
    viewport: `${opts.width}x${opts.height}`,
    runs,
    metrics: aggregate(runs.filter((r) => r.errors.length === 0).map((r) => r.metrics))
  }
  writeFileSync(path.join(opts.out, 'webkit-startup.json'), JSON.stringify(summary, null, 1))
  console.log(`[webkit] wrote ${opts.out}`)
}

if (import.meta.main ?? process.argv[1] === import.meta.filename) {
  await webkitStartup(process.argv.slice(2))
  process.exit(0)
}
