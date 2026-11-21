// Splits recent pull_request CI runs into waiting, setup and running time per job,
// and reports time-to-result per run at p50 and p95.
//
//   bun tools/ci-audit/audit.ts [--repo owner/name] [--limit 150] [--workflow pr-validation.yml] [--tsv jobs.tsv]
//
// Waiting is job queued -> job started. Setup is checkout, toolchain, install,
// cache restore and post-job steps. Running is every other step.
// Time-to-result is run created -> last job finished, for runs that tested code
// (cancelled runs and description-edit runs are excluded).
import { $ } from 'bun'

type Step = { name: string; started_at: string | null; completed_at: string | null }
type Job = {
  name: string
  created_at: string
  started_at: string | null
  completed_at: string | null
  conclusion: string | null
  labels: string[]
  steps?: Step[]
}
type Run = {
  id: number
  name: string
  event: string
  created_at: string
  conclusion: string | null
}

export type JobTiming = {
  run: number
  workflow: string
  job: string
  conclusion: string | null
  waitMs: number
  setupMs: number
  runningMs: number
  windows: boolean
}

export type RunTiming = { run: number; rust: boolean; resultMs: number }

const SETUP_STEP =
  /^(Set up job|Complete job|Post |Checkout|Setup |Set up |Install |Cache |Restore |Download )/i

export function isSetupStep(name: string): boolean {
  return SETUP_STEP.test(name)
}

const span = (from: string | null, to: string | null) =>
  from && to ? Math.max(0, Date.parse(to) - Date.parse(from)) : 0

export function timeJob(run: Run, job: Job): JobTiming | null {
  if (job.conclusion === 'skipped' || !job.started_at || !job.completed_at) return null
  let setupMs = 0
  let runningMs = 0
  for (const step of job.steps ?? []) {
    const ms = span(step.started_at, step.completed_at)
    if (isSetupStep(step.name)) setupMs += ms
    else runningMs += ms
  }
  return {
    run: run.id,
    workflow: run.name,
    job: job.name,
    conclusion: job.conclusion,
    waitMs: span(job.created_at, job.started_at),
    setupMs,
    runningMs,
    windows: job.labels.some((label) => label.includes('windows'))
  }
}

const CODE_JOB = /^(Lint, Typecheck & Test|Rust Checks|Web Client Build)$/

export function timeRun(run: Run, jobs: Job[]): RunTiming | null {
  if (run.conclusion === 'cancelled') return null
  const ran = jobs.filter((job) => job.conclusion !== 'skipped' && job.completed_at)
  if (!ran.some((job) => CODE_JOB.test(job.name))) return null
  const last = Math.max(...ran.map((job) => Date.parse(job.completed_at as string)))
  return {
    run: run.id,
    rust: ran.some((job) => job.name === 'Rust Checks'),
    resultMs: last - Date.parse(run.created_at)
  }
}

export function percentile(values: number[], p: number): number {
  if (values.length === 0) return Number.NaN
  const sorted = [...values].sort((a, b) => a - b)
  return sorted[Math.min(sorted.length - 1, Math.floor((p / 100) * sorted.length))]
}

function flag(name: string, fallback: string): string {
  const i = process.argv.indexOf(`--${name}`)
  return i === -1 ? fallback : process.argv[i + 1]
}

async function api<T>(path: string): Promise<T> {
  return JSON.parse(await $`gh api ${path}`.quiet().text()) as T
}

async function main() {
  const repo = flag('repo', 'gnoviawan/troupe')
  const limit = Number(flag('limit', '150'))
  const workflow = flag('workflow', 'pr-validation.yml')
  const tsv = flag('tsv', '')

  const runs: Run[] = []
  for (let page = 1; runs.length < limit; page++) {
    const { workflow_runs } = await api<{ workflow_runs: Run[] }>(
      `repos/${repo}/actions/workflows/${workflow}/runs?event=pull_request&status=completed&per_page=100&page=${page}`
    )
    if (workflow_runs.length === 0) break
    runs.push(...workflow_runs)
  }
  runs.length = Math.min(runs.length, limit)

  const jobs: JobTiming[] = []
  const results: RunTiming[] = []
  let errors = 0
  await Promise.all(
    runs.map(async (run, i) => {
      await Bun.sleep(i * 40)
      try {
        const page = await api<{ jobs: Job[] }>(
          `repos/${repo}/actions/runs/${run.id}/jobs?per_page=100&filter=latest`
        )
        for (const job of page.jobs) {
          const timing = timeJob(run, job)
          if (timing) jobs.push(timing)
        }
        const result = timeRun(run, page.jobs)
        if (result) results.push(result)
      } catch (error) {
        errors++
        console.error(`run ${run.id}: ${error}`)
      }
    })
  )

  const s = (ms: number) => (Number.isNaN(ms) ? '-' : (ms / 1000).toFixed(0))
  const p = (values: number[]) => `${s(percentile(values, 50))}/${s(percentile(values, 95))}`

  console.log(
    `runs=${runs.length} jobs=${jobs.length} errors=${errors} window=${runs.at(-1)?.created_at}..${runs[0]?.created_at}`
  )
  console.log('seconds as p50/p95')
  console.log(['n', 'wait', 'setup', 'running', 'busy_min', 'job'].join('\t'))
  const byJob = Map.groupBy(jobs, (job) => `${job.workflow} / ${job.job}`)
  const busy = (group: JobTiming[]) =>
    group.reduce((sum, job) => sum + job.setupMs + job.runningMs, 0)
  for (const [name, group] of [...byJob].sort((a, b) => busy(b[1]) - busy(a[1]))) {
    console.log(
      [
        group.length,
        p(group.map((job) => job.waitMs)),
        p(group.map((job) => job.setupMs)),
        p(group.map((job) => job.runningMs)),
        (busy(group) / 60_000).toFixed(0),
        name
      ].join('\t')
    )
  }
  console.log(
    `all jobs: wait ${p(jobs.map((j) => j.waitMs))} setup ${p(jobs.map((j) => j.setupMs))} running ${p(jobs.map((j) => j.runningMs))}`
  )
  const rust = results.filter((r) => r.rust).map((r) => r.resultMs)
  const other = results.filter((r) => !r.rust).map((r) => r.resultMs)
  console.log(
    `time-to-result: all ${p(results.map((r) => r.resultMs))} (n=${results.length}) | no-rust ${p(other)} (n=${other.length}) | rust ${p(rust)} (n=${rust.length})`
  )
  const minutes = jobs.reduce(
    (sum, job) => sum + Math.ceil((job.setupMs + job.runningMs) / 60_000) * (job.windows ? 2 : 1),
    0
  )
  console.log(`runner minutes (rounded up per job, windows x2): ${minutes}`)

  if (tsv) {
    const header = 'run\tworkflow\tjob\tconclusion\twait_s\tsetup_s\trunning_s\twindows'
    const lines = jobs.map((j) =>
      [
        j.run,
        j.workflow,
        j.job,
        j.conclusion,
        j.waitMs / 1e3,
        j.setupMs / 1e3,
        j.runningMs / 1e3,
        j.windows
      ].join('\t')
    )
    await Bun.write(tsv, [header, ...lines].join('\n'))
  }
}

if (import.meta.main) await main()
