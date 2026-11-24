/**
 * Per-run load and per-thread CPU of a multi-client result set, as Markdown.
 * `report` prints aggregate medians; this prints what explains them.
 *
 *   node tools/perf/runs-report.ts <set-dir> [phase ...]
 *
 * <set-dir> holds one directory per client (`run.ts --clients a,b`, or the
 * labels of `perf compare`). It also prints an exact rank-sum test per key
 * for every pair of clients, with each side's median and p95.
 */
import { readdirSync, readFileSync } from 'node:fs'
import path from 'node:path'
import { median } from './stats.ts'
import { percentile } from './types.ts'

export interface RunRecord {
  meta: {
    scenario: string
    run: number
    client: string
    status: string
    preRunBusyPct: number
    quietWaitS: number
    load1Before: number
  }
  metrics: Record<string, number>
  errors: string[]
  notes: { threads?: Record<string, Array<{ thread: string; cpuPct: number }>> }
}

/** One `<client>/<scenario>-run<N>.json` file per run under `<set-dir>/<client>/`. */
export function loadRunSet(setDir: string): RunRecord[] {
  const runs: RunRecord[] = []
  for (const client of readdirSync(setDir)) {
    const dir = path.join(setDir, client)
    for (const file of readdirSync(dir)) {
      if (/-run\d+\.json$/.test(file))
        runs.push(JSON.parse(readFileSync(path.join(dir, file), 'utf8')) as RunRecord)
    }
  }
  runs.sort(
    (a, b) =>
      a.meta.run - b.meta.run ||
      a.meta.scenario.localeCompare(b.meta.scenario) ||
      a.meta.client.localeCompare(b.meta.client)
  )
  return runs
}

/**
 * Two-sided p of the Mann-Whitney rank-sum statistic. Exact for small sets:
 * every way to split the pooled values into groups of the original sizes.
 */
export function rankSumP(a: number[], b: number[]): number {
  const pooled = [...a, ...b]
  const sorted = [...pooled].sort((x, y) => x - y)
  const rank = (v: number) => {
    const first = sorted.indexOf(v)
    const last = sorted.lastIndexOf(v)
    return (first + last) / 2 + 1
  }
  const ranks = pooled.map(rank)
  const n = pooled.length
  const expected = (a.length * (n + 1)) / 2
  const observed = Math.abs(a.reduce((sum, v) => sum + rank(v), 0) - expected)
  if (n > 14) {
    // Too many splits to count; the normal approximation is good at this size.
    const sd = Math.sqrt((a.length * b.length * (n + 1)) / 12)
    return erfc((observed - 0.5) / sd / Math.SQRT2)
  }
  let extreme = 0
  let total = 0
  const pick = (start: number, left: number, sum: number) => {
    if (left === 0) {
      total++
      if (Math.abs(sum - expected) >= observed - 1e-9) extreme++
      return
    }
    for (let i = start; i <= n - left; i++) pick(i + 1, left - 1, sum + ranks[i])
  }
  pick(0, a.length, 0)
  return extreme / total
}

/** Abramowitz and Stegun 7.1.26, absolute error under 1.5e-7. */
function erfc(x: number): number {
  const t = 1 / (1 + 0.3275911 * Math.abs(x))
  const poly =
    t *
    (0.254829592 + t * (-0.284496736 + t * (1.421413741 + t * (-1.453152027 + t * 1.061405429))))
  const e = poly * Math.exp(-x * x)
  return x >= 0 ? e : 2 - e
}

/** Measured before the first chat starts streaming in more than one chat: valid in a run that crashed later. */
const beforeCrash = (key: string) => key.startsWith('warmup.') || key === 'chat.firstStartMs'

const COMPARE: Record<string, string[]> = {
  launch: [
    'startup.processUpMs',
    'startup.projectListMs',
    'startup.launcherReadyMs',
    'measure.proc.total.pssMedianMB',
    'measure.proc.total.cpuMeanPct',
    'measure.screen.fps',
    'measure.bytesDownKBps'
  ],
  idle: [
    'startup.projectListMs',
    'startup.launcherReadyMs',
    'measure.proc.total.pssMedianMB',
    'measure.proc.total.cpuMeanPct',
    'measure.screen.fps'
  ],
  'stream-storm': [
    'chat.firstStartMs',
    'warmup.screen.fps',
    'warmup.proc.total.pssMedianMB',
    'warmup.proc.total.cpuMeanPct',
    'chat.startUnderLoadP50Ms',
    'chat.typeUnderLoadP50Ms',
    'chat.openUnderLoadP50Ms',
    'sustained-stream.screen.fps',
    'measure.screen.fps',
    'measure.proc.total.pssMedianMB',
    'measure.proc.total.cpuMeanPct',
    'measure.tree.reachable',
    'chatsOpen'
  ],
  'pty-flood': [
    'measure.screen.fps',
    'measure.screen.gapP95Ms',
    'measure.proc.total.pssMedianMB',
    'measure.proc.total.cpuMeanPct',
    'probe.pacedRttP50Ms',
    'probe.pacedRttP95Ms',
    'probe.burstMBps',
    'burst.screen.fps'
  ],
  soak: [
    'warmup.proc.total.pssMedianMB',
    'warmup.proc.total.cpuMeanPct',
    'soak.screen.fps',
    'soak.proc.total.pssMedianMB',
    'soak.proc.total.cpuMeanPct',
    'soak.machine.foreignCpuPct',
    'measure.screen.fps',
    'measure.proc.total.pssMedianMB',
    'measure.proc.total.cpuMeanPct',
    'chat.switches',
    'chatsOpen'
  ]
}

const fmtSide = (values: number[]) =>
  `median ${median(values)} / p95 ${percentile(values, 0.95)} of ${values.length}`

/** The whole report for a loaded run set, as Markdown text. */
export function formatRunsReport(runs: RunRecord[], phases: string[] = ['measure']): string {
  const out: string[] = []
  const log = (line = '') => out.push(line)

  log('### Load and errors per run\n')
  log(
    '| Run | Scenario | Client | Status | Pre-run busy | Gate wait | load1 | Foreign CPU, measure | Errors |'
  )
  log('|---|---|---|---|---|---|---|---|---|')
  for (const r of runs) {
    const m = r.meta
    // `chat 3: Error: x` and `chat 4: Error: x` are one kind of error.
    const kinds = new Set(
      r.errors.map((e) =>
        e
          .split('\n')[0]
          .replace(/^\w+ \d+: /, '')
          .slice(0, 80)
      )
    )
    const errors = r.errors.length ? `${r.errors.length} errors: ${[...kinds].join('; ')}` : 'none'
    log(
      `| ${m.run} | ${m.scenario} | ${m.client} | ${m.status} | ${m.preRunBusyPct}% | ${m.quietWaitS} s | ${m.load1Before.toFixed(1)} | ${r.metrics['measure.machine.foreignCpuPct'] ?? 'n/a'}% | ${errors} |`
    )
  }

  for (const phase of phases) {
    const groups = new Map<string, RunRecord[]>()
    for (const r of runs) {
      if (r.meta.status !== 'ok' || !r.notes.threads?.[phase]) continue
      const key = `${r.meta.scenario} / ${r.meta.client}`
      groups.set(key, [...(groups.get(key) ?? []), r])
    }
    if (!groups.size) continue
    log(`\n### Thread CPU in the \`${phase}\` phase, median of ok runs (% of one core)\n`)
    for (const [key, group] of groups) {
      const byThread = new Map<string, number[]>()
      for (const r of group) {
        for (const t of r.notes.threads?.[phase] ?? [])
          byThread.set(t.thread, [...(byThread.get(t.thread) ?? []), t.cpuPct])
      }
      const rows = [...byThread]
        // A thread missing from a run's top list counts as 0 there.
        .map(([thread, v]) => ({
          thread,
          cpu: median([...v, ...Array(group.length - v.length).fill(0)])
        }))
        .sort((a, b) => b.cpu - a.cpu)
        .slice(0, 6)
      log(
        `- ${key} (${group.length} runs): ${rows.map((r) => `\`${r.thread}\` ${r.cpu}`).join(', ')}`
      )
    }
  }

  const clients = [...new Set(runs.map((r) => r.meta.client))].sort()
  const pairs = clients.flatMap((a, i) => clients.slice(i + 1).map((b) => [a, b] as const))
  for (const [a, b] of pairs) {
    log(
      `\n### ${a} against ${b}, ok runs (all runs for keys measured before any crash), two-sided rank-sum p\n`
    )
    log(`| Scenario | Key | ${a} runs | ${b} runs | p |`)
    log('|---|---|---|---|---|')
    // Startup comes before any scenario step, so every run of every scenario counts.
    for (const key of ['startup.processUpMs', 'startup.projectListMs', 'startup.launcherReadyMs']) {
      const of = (client: string) =>
        runs
          .filter((r) => r.meta.client === client && r.metrics[key] !== undefined)
          .map((r) => r.metrics[key])
      const [va, vb] = [of(a), of(b)]
      if (!va.length || !vb.length) continue
      log(`| all | \`${key}\` | ${fmtSide(va)} | ${fmtSide(vb)} | ${rankSumP(va, vb).toFixed(3)} |`)
    }
    for (const [scenario, keys] of Object.entries(COMPARE)) {
      const of = (client: string, key: string) =>
        runs
          .filter(
            (r) =>
              r.meta.scenario === scenario &&
              r.meta.client === client &&
              (r.meta.status === 'ok' || beforeCrash(key))
          )
          .map((r) => r.metrics[key])
          .filter((v) => v !== undefined)
      for (const key of keys) {
        const [va, vb] = [of(a, key), of(b, key)]
        if (!va.length || !vb.length) continue
        const p = rankSumP(va, vb)
        log(`| ${scenario} | \`${key}\` | ${va.join(', ')} | ${vb.join(', ')} | ${p.toFixed(3)} |`)
      }
    }
  }
  return out.join('\n')
}

if (import.meta.main ?? process.argv[1] === import.meta.filename) {
  const [setDir, ...phaseArgs] = process.argv.slice(2)
  if (!setDir) throw new Error('usage: runs-report.ts <set-dir> [phase ...]')
  console.log(formatRunsReport(loadRunSet(setDir), phaseArgs.length ? phaseArgs : ['measure']))
}
