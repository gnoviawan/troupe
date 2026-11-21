// Audits the sharded bun test jobs of one PR Validation run.
//   bun tools/ci-audit/test-parity.ts <run-id> [--baseline <run-id>] [--repo owner/name]
// Downloads every `bun-junit*` artifact with `gh run download`, then prints
// each Test shard's Test step and job seconds, the run's time to result for
// Checks and the shards, the totals, and any file run by no shard or by more
// than one. With --baseline, also lists every file whose test counts differ
// from the baseline run's.
import { mkdtempSync, readdirSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

type Counts = { tests: number; failed: number; skipped: number }
type Side = Map<string, Counts>
type Job = {
  name: string
  conclusion: string
  startedAt: string
  completedAt: string
  steps: { name: string; startedAt: string; completedAt: string }[]
}
type Run = { createdAt: string; jobs: Job[] }

function parseArgs(argv: string[]): { runId: string; baseline?: string; repo: string[] } {
  const usage =
    'usage: bun tools/ci-audit/test-parity.ts <run-id> [--baseline <run-id>] [--repo owner/name]'
  const [runId, ...rest] = argv
  const options: Record<string, string> = {}
  for (let i = 0; i < rest.length; i += 2) {
    const [flag, value] = [rest[i], rest[i + 1]]
    if ((flag !== '--baseline' && flag !== '--repo') || !value) {
      console.error(usage)
      process.exit(2)
    }
    options[flag] = value
  }
  if (
    !runId ||
    !/^\d+$/.test(runId) ||
    (options['--baseline'] && !/^\d+$/.test(options['--baseline']))
  ) {
    console.error(usage)
    process.exit(2)
  }
  return {
    runId,
    baseline: options['--baseline'],
    repo: options['--repo'] ? ['--repo', options['--repo']] : []
  }
}

// Bun reports an error thrown outside any test as a testcase named
// "(unhandled error)". It counts as a failure of that file, not as a test.
function parseBunJunit(xml: string): Side {
  const side: Side = new Map()
  for (const match of xml.matchAll(/<testcase\b([^>]*?)(?:\/>|>([\s\S]*?)<\/testcase>)/g)) {
    const attrs = match[1] ?? ''
    const body = match[2] ?? ''
    const file = /\bfile="([^"]+)"/.exec(attrs)?.[1]
    if (!file) continue
    const counts = side.get(file) ?? { tests: 0, failed: 0, skipped: 0 }
    side.set(file, counts)
    const failed = /<(failure|error)\b/.test(body)
    if (/\bname="\(unhandled error\)"/.test(attrs)) {
      if (failed) counts.failed += 1
      continue
    }
    counts.tests += 1
    if (failed) counts.failed += 1
    else if (/<skipped\b/.test(body)) counts.skipped += 1
  }
  for (const match of xml.matchAll(/<testsuite\b[^>]*\bfile="([^"]+)"/g)) {
    const file = match[1]
    if (file && !side.has(file)) side.set(file, { tests: 0, failed: 0, skipped: 0 })
  }
  return side
}

/** One Side per downloaded junit artifact, keyed by artifact name. */
async function downloadJunit(runId: string, repo: string[]): Promise<Map<string, Side>> {
  const dir = mkdtempSync(join(tmpdir(), `test-parity-${runId}-`))
  await Bun.$`gh run download ${runId} ${repo} -p ${'bun-junit*'} -D ${dir}`.quiet()
  const sides = new Map<string, Side>()
  for (const artifact of readdirSync(dir).sort()) {
    sides.set(artifact, parseBunJunit(await Bun.file(join(dir, artifact, 'bun-junit.xml')).text()))
  }
  return sides
}

const seconds = (from: string, to: string): number => (Date.parse(to) - Date.parse(from)) / 1000

function total(sides: Iterable<Side>): Counts {
  const sum = { tests: 0, failed: 0, skipped: 0 }
  for (const side of sides) {
    for (const counts of side.values()) {
      sum.tests += counts.tests
      sum.failed += counts.failed
      sum.skipped += counts.skipped
    }
  }
  return sum
}

function merge(sides: Iterable<Side>): { merged: Side; runs: Map<string, number> } {
  const merged: Side = new Map()
  const runs = new Map<string, number>()
  for (const side of sides) {
    for (const [file, counts] of side) {
      merged.set(file, counts)
      runs.set(file, (runs.get(file) ?? 0) + 1)
    }
  }
  return { merged, runs }
}

const format = (counts: Counts | undefined): string =>
  counts ? `${counts.tests} tests, ${counts.failed} failed, ${counts.skipped} skipped` : 'missing'

const { runId, baseline, repo } = parseArgs(process.argv.slice(2))
const run: Run = await Bun.$`gh run view ${runId} ${repo} --json createdAt,jobs`.json()
const shards = await downloadJunit(runId, repo)

console.log(`run ${runId}`)
const testJobs = run.jobs
  .filter((job) => /^Test \(\d+\/\d+\)$/.test(job.name))
  .sort((a, b) => a.name.localeCompare(b.name, undefined, { numeric: true }))
for (const job of testJobs) {
  const step = job.steps.find((candidate) => candidate.name === 'Test')
  const stepSeconds = step ? seconds(step.startedAt, step.completedAt) : Number.NaN
  console.log(
    `${job.name}: Test step ${stepSeconds} s, job ${seconds(job.startedAt, job.completedAt)} s, ${job.conclusion || 'in progress'}`
  )
}
const resultJobs = run.jobs.filter((job) => job.name === 'Checks' || testJobs.includes(job))
const done = resultJobs
  .map((job) => job.completedAt)
  .sort()
  .at(-1)
if (done) console.log(`time to result (Checks and Test shards): ${seconds(run.createdAt, done)} s`)

const totals = total(shards.values())
const { merged, runs } = merge(shards.values())
console.log(
  `${shards.size} junit reports; ${merged.size} distinct files; ${totals.tests} tests, ${totals.failed} failed, ${totals.skipped} skipped`
)
const repeated = [...runs].filter(([, count]) => count > 1)
console.log(`files in more than one report: ${repeated.length}`)
for (const [file, count] of repeated) console.log(`  ${file}: ${count}`)

if (baseline) {
  const { merged: base } = merge((await downloadJunit(baseline, repo)).values())
  const files = [...new Set([...base.keys(), ...merged.keys()])].sort()
  const missing = files.filter((file) => !base.has(file) || !merged.has(file))
  const differing = files.filter((file) => {
    const a = base.get(file)
    const b = merged.get(file)
    return a && b && (a.tests !== b.tests || a.failed !== b.failed || a.skipped !== b.skipped)
  })
  console.log(`\nbaseline ${baseline}: ${base.size} files, ${format(total([base]))}`)
  console.log(`files missing on one side: ${missing.length}`)
  for (const file of missing)
    console.log(`  ${file}: baseline ${format(base.get(file))}; run ${format(merged.get(file))}`)
  console.log(`files whose counts differ: ${differing.length}`)
  for (const file of differing)
    console.log(`  ${file}: baseline ${format(base.get(file))}; run ${format(merged.get(file))}`)
}
