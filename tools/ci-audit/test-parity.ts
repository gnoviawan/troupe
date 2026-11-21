// Compares the vitest and bun test runs of one PR Validation run, file by file.
//   bun tools/ci-audit/test-parity.ts <run-id> [--repo owner/name]
// Downloads the `vitest-report` (vitest JSON) and `bun-junit` (bun junit XML)
// artifacts with `gh run download`, then prints both Test step wall times,
// files missing on either side, and every file whose test counts differ.
import { mkdtempSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

type Counts = { tests: number; failed: number; skipped: number }
type Side = Map<string, Counts>

type VitestReport = {
  testResults: { name: string; status: string; assertionResults: { status: string }[] }[]
}
type RunJobs = {
  jobs: {
    name: string
    conclusion: string
    steps: { name: string; startedAt: string; completedAt: string }[]
  }[]
}

const SIDES = [
  { label: 'vitest', job: 'Test', artifact: 'vitest-report' },
  { label: 'bun', job: 'Test (bun)', artifact: 'bun-junit' }
] as const

function parseArgs(argv: string[]): { runId: string; repo: string[] } {
  const [runId, flag, repo] = argv
  if (!runId || !/^\d+$/.test(runId) || (flag !== undefined && (flag !== '--repo' || !repo))) {
    console.error('usage: bun tools/ci-audit/test-parity.ts <run-id> [--repo owner/name]')
    process.exit(2)
  }
  return { runId, repo: repo ? ['--repo', repo] : [] }
}

function commonDir(paths: string[]): string {
  let prefix = paths[0] ?? ''
  for (const path of paths) while (!path.startsWith(prefix)) prefix = prefix.slice(0, -1)
  return prefix.slice(0, prefix.lastIndexOf('/') + 1)
}

function parseVitest(report: VitestReport): Side {
  const root = commonDir(report.testResults.map((result) => result.name))
  const side: Side = new Map()
  for (const result of report.testResults) {
    const statuses = result.assertionResults.map((assertion) => assertion.status)
    const failed = statuses.filter((status) => status === 'failed').length
    side.set(result.name.slice(root.length), {
      tests: statuses.length,
      // A file that fails outside any test (import or hook error) has no failed test.
      failed: failed === 0 && result.status === 'failed' ? 1 : failed,
      skipped: statuses.filter((status) => ['skipped', 'pending', 'todo'].includes(status)).length
    })
  }
  return side
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

function stepSeconds(jobs: RunJobs, jobName: string): string {
  const job = jobs.jobs.find((candidate) => candidate.name === jobName)
  const step = job?.steps.find((candidate) => candidate.name === 'Test')
  if (!job || !step) return 'n/a'
  const seconds = (Date.parse(step.completedAt) - Date.parse(step.startedAt)) / 1000
  return `${seconds}s (job ${job.conclusion || 'in progress'})`
}

function total(side: Side): Counts {
  const sum = { tests: 0, failed: 0, skipped: 0 }
  for (const counts of side.values()) {
    sum.tests += counts.tests
    sum.failed += counts.failed
    sum.skipped += counts.skipped
  }
  return sum
}

const format = (counts: Counts | undefined): string =>
  counts ? `${counts.tests} tests, ${counts.failed} failed, ${counts.skipped} skipped` : 'missing'

const { runId, repo } = parseArgs(process.argv.slice(2))
const dir = mkdtempSync(join(tmpdir(), `test-parity-${runId}-`))
const artifactFlags = SIDES.flatMap((side) => ['-n', side.artifact])
await Bun.$`gh run download ${runId} ${repo} ${artifactFlags} -D ${dir}`
const jobs: RunJobs = await Bun.$`gh run view ${runId} ${repo} --json jobs`.json()

const vitest = parseVitest(await Bun.file(join(dir, 'vitest-report', 'vitest-report.json')).json())
const bun = parseBunJunit(await Bun.file(join(dir, 'bun-junit', 'bun-junit.xml')).text())
const sides: Record<(typeof SIDES)[number]['label'], Side> = { vitest, bun }

console.log(`run ${runId}`)
for (const side of SIDES) {
  console.log(
    `${side.label.padEnd(7)} Test step ${stepSeconds(jobs, side.job)}; ${sides[side.label].size} files; ${format(total(sides[side.label]))}`
  )
}

const files = [...new Set([...vitest.keys(), ...bun.keys()])].sort()
const missing = files.filter((file) => !vitest.has(file) || !bun.has(file))
const differing = files.filter((file) => {
  const a = vitest.get(file)
  const b = bun.get(file)
  return a && b && (a.tests !== b.tests || a.failed !== b.failed || a.skipped !== b.skipped)
})

console.log(`\nfiles missing on one side: ${missing.length}`)
for (const file of missing) {
  console.log(`  ${file}: vitest ${format(vitest.get(file))}; bun ${format(bun.get(file))}`)
}
console.log(`\nfiles whose counts differ: ${differing.length}`)
for (const file of differing) {
  console.log(`  ${file}: vitest ${format(vitest.get(file))}; bun ${format(bun.get(file))}`)
}
