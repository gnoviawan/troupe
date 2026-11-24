import { execFileSync } from 'node:child_process'
import { readdirSync, readFileSync } from 'node:fs'
import path from 'node:path'
import { setTimeout as sleep } from 'node:timers/promises'

/**
 * Linux /proc sampler for one client process tree. It reads only kernel
 * counters, so it measures Chromium, WebKitGTK and a Rune client the same
 * way.
 *
 * RSS counts shared pages once per process, so a multi-process browser's
 * RSS sum overstates its footprint. PSS splits shared pages between the
 * processes that map them, so the PSS sum is the honest total. Both are
 * kept. CPU is in percent of one core (top's convention), from utime+stime.
 *
 * `procRoot` exists for tests: point it at a fixture directory holding
 * `<pid>/{stat,statm,smaps_rollup,cmdline}` files instead of real /proc.
 * Every reader defaults to the live kernel interface.
 */

export type RoleOf = (pid: number, cmdline: string) => string

export interface ProcSample {
  /** Epoch ms of the sample. */
  t: number
  /** Per role: summed RSS/PSS bytes and CPU percent of one core. */
  roles: Record<string, { rss: number; pss: number; cpu: number; count: number }>
  total: { rss: number; pss: number; cpu: number; count: number }
  pids: Record<number, { rss: number; pss: number; cpu: number }>
}

export interface ProcSampler {
  stop(): ProcSample[]
  /** Role per pid as first seen. */
  processes(): Array<{ pid: number; role: string; cmd: string }>
}

const CLK_TCK = Number(execFileSync('getconf', ['CLK_TCK']).toString().trim()) || 100
const PAGE = Number(execFileSync('getconf', ['PAGESIZE']).toString().trim()) || 4096

/** `/proc/<pid>/stat`: ppid and utime+stime ticks, parsed past the comm field. */
export function parseProcStat(raw: string): { ppid: number; ticks: number } | null {
  const fields = raw.slice(raw.lastIndexOf(')') + 2).split(' ')
  const ppid = Number(fields[1])
  const ticks = Number(fields[11]) + Number(fields[12])
  if (!Number.isFinite(ppid) || !Number.isFinite(ticks)) return null
  return { ppid, ticks }
}

/** `/proc/<pid>/statm`: RSS in bytes (field 1 is resident pages). */
export function parseStatm(raw: string, pageSize = PAGE): number {
  return Number(raw.split(' ')[1]) * pageSize
}

/** `/proc/<pid>/smaps_rollup`: Pss in bytes. */
export function parseSmapsRollup(raw: string): number {
  const m = /^Pss:\s+(\d+) kB/m.exec(raw)
  return m ? Number(m[1]) * 1024 : 0
}

function readStat(procRoot: string, pid: number): { ppid: number; ticks: number } | null {
  try {
    return parseProcStat(readFileSync(path.join(procRoot, String(pid), 'stat'), 'utf8'))
  } catch {
    return null
  }
}

function readMem(procRoot: string, pid: number): { rss: number; pss: number } {
  let rss = 0
  let pss = 0
  try {
    rss = parseStatm(readFileSync(path.join(procRoot, String(pid), 'statm'), 'utf8'))
    const rollup = readFileSync(path.join(procRoot, String(pid), 'smaps_rollup'), 'utf8')
    pss = parseSmapsRollup(rollup)
  } catch {
    // process exited between listing and reading
  }
  return { rss, pss }
}

export function readCmdline(pid: number, procRoot = '/proc'): string {
  try {
    return readFileSync(path.join(procRoot, String(pid), 'cmdline'), 'utf8')
      .replace(/\0/g, ' ')
      .trim()
  } catch {
    return ''
  }
}

export function descendants(rootPid: number, procRoot = '/proc'): number[] {
  const children = new Map<number, number[]>()
  for (const name of readdirSync(procRoot)) {
    const pid = Number(name)
    if (!Number.isInteger(pid)) continue
    const stat = readStat(procRoot, pid)
    if (!stat) continue
    const list = children.get(stat.ppid) ?? []
    list.push(pid)
    children.set(stat.ppid, list)
  }
  const out = [rootPid]
  for (let i = 0; i < out.length; i++) out.push(...(children.get(out[i]) ?? []))
  return out
}

export function startProcSampler(
  rootPid: number,
  roleOf: RoleOf,
  cadenceMs = 500,
  procRoot = '/proc'
): ProcSampler {
  const samples: ProcSample[] = []
  const lastTicks = new Map<number, number>()
  const known = new Map<number, { role: string; cmd: string }>()
  let lastWall = Date.now()
  let first = true

  const tick = () => {
    const now = Date.now()
    const dt = (now - lastWall) / 1000
    const roles: ProcSample['roles'] = {}
    const total = { rss: 0, pss: 0, cpu: 0, count: 0 }
    const pids: ProcSample['pids'] = {}
    for (const pid of descendants(rootPid, procRoot)) {
      const stat = readStat(procRoot, pid)
      if (!stat) continue
      let info = known.get(pid)
      if (!info) {
        const cmd = readCmdline(pid, procRoot)
        info = { role: roleOf(pid, cmd), cmd: cmd.slice(0, 200) }
        known.set(pid, info)
      }
      const prev = lastTicks.get(pid) ?? (first ? stat.ticks : 0)
      lastTicks.set(pid, stat.ticks)
      const cpu = first || dt <= 0 ? 0 : ((stat.ticks - prev) / CLK_TCK / dt) * 100
      const mem = readMem(procRoot, pid)
      pids[pid] = { ...mem, cpu }
      roles[info.role] ??= { rss: 0, pss: 0, cpu: 0, count: 0 }
      const bucket = roles[info.role]
      for (const agg of [bucket, total]) {
        agg.rss += mem.rss
        agg.pss += mem.pss
        agg.cpu += cpu
        agg.count += 1
      }
    }
    if (!first) samples.push({ t: now, roles, total, pids })
    first = false
    lastWall = now
  }
  tick()
  const timer = setInterval(tick, cadenceMs)
  return {
    stop: () => {
      clearInterval(timer)
      tick()
      return samples
    },
    processes: () => [...known].map(([pid, info]) => ({ pid, ...info }))
  }
}

/**
 * Adds a `page-renderer` role: of the pids with role `renderer`, the one that
 * used the most CPU over the run. Chromium also keeps a spare renderer warm,
 * so the role sum alone is not the page.
 */
export function withPageRenderer(
  samples: ProcSample[],
  roleOfPid: Map<number, string>
): number | null {
  const cpuByPid = new Map<number, number>()
  for (const s of samples) {
    for (const [pid, v] of Object.entries(s.pids)) {
      if (roleOfPid.get(Number(pid)) !== 'renderer') continue
      cpuByPid.set(Number(pid), (cpuByPid.get(Number(pid)) ?? 0) + v.cpu)
    }
  }
  const page = [...cpuByPid].sort((a, b) => b[1] - a[1])[0]?.[0]
  if (page === undefined) return null
  for (const s of samples) {
    const v = s.pids[page]
    if (v) s.roles['page-renderer'] = { ...v, count: 1 }
  }
  return page
}

export const chromiumRole: RoleOf = (_pid, cmd) => {
  if (cmd.includes('--top-chrome-webui')) return 'chrome-ui'
  const m = /--type=([a-z-]+)/.exec(cmd)
  if (!m) return 'browser'
  if (m[1] === 'utility') {
    const sub = /--utility-sub-type=([\w.]+)/.exec(cmd)
    return sub?.[1].includes('network') ? 'network' : 'utility'
  }
  return m[1]
}

export interface MachineSample {
  t: number
  /** Busy CPU of the whole machine in percent of one core (800 = 8 cores busy). */
  busy: number
}

function readBusyTicks(): { busy: number; total: number } {
  const fields = readFileSync('/proc/stat', 'utf8')
    .split('\n')[0]
    .trim()
    .split(/\s+/)
    .slice(1, 9)
    .map(Number)
  const total = fields.reduce((a, b) => a + b, 0)
  return { busy: total - fields[3] - fields[4], total }
}

/**
 * Whole-machine CPU, so a run can show how much of the box something other
 * than the client and server used. Other units share this machine.
 */
export function startMachineSampler(cadenceMs = 500): { stop(): MachineSample[] } {
  const samples: MachineSample[] = []
  let last = readBusyTicks()
  let lastWall = Date.now()
  const tick = () => {
    const now = readBusyTicks()
    const wall = Date.now()
    const seconds = (wall - lastWall) / 1000
    if (seconds > 0)
      samples.push({ t: wall, busy: ((now.busy - last.busy) / CLK_TCK / seconds) * 100 })
    last = now
    lastWall = wall
  }
  const timer = setInterval(tick, cadenceMs)
  return {
    stop: () => {
      clearInterval(timer)
      tick()
      return samples
    }
  }
}

/**
 * Other units share this machine. Before a run, poll 5 s windows of
 * whole-machine CPU until one falls under the threshold or time runs out.
 * The run goes ahead either way; the record says how quiet it was.
 */
export async function waitForQuiet(
  belowPct: number,
  waitS: number
): Promise<{ waitedS: number; busyPct: number }> {
  const started = Date.now()
  for (;;) {
    const sampler = startMachineSampler()
    await sleep(5000)
    const busy = sampler.stop().map((m) => m.busy)
    const busyPct = Math.round(busy.reduce((a, b) => a + b, 0) / busy.length)
    const waitedS = Math.round((Date.now() - started) / 1000)
    if (busyPct < belowPct || waitedS >= waitS) return { waitedS, busyPct }
  }
}
