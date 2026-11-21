// Splits the test files into balanced shards and writes one shard's slice.
//   bun scripts/ci/shard.ts <index> <count> <out-dir> [timings.json ...]
// Every shard computes the whole partition from the same inputs and checks
// that it covers each file exactly once. Writes <out-dir>/files.txt for
// `bun test --files-from` and <out-dir>/timings.json, the slice's own
// timings, for `bun test --timings --update-timings`.
import { mkdirSync } from 'node:fs'
import { join } from 'node:path'
import { listTestFiles } from './test-files'

/** Milliseconds per test file, in bun's `--timings` file format. */
type TimingsFile = { version: 1; files: Record<string, number> }
type Slice = { files: string[]; predictedMs: number }

/**
 * Greedy longest-first: each file, slowest first, goes to the shard with the
 * least predicted time. Ties go to the earlier path and the lower shard, so
 * every shard computes the same partition. Files with no timing count as the
 * median.
 */
export function partition(
  files: readonly string[],
  timings: ReadonlyMap<string, number>,
  count: number
): Slice[] {
  const known = files.flatMap((file) => timings.get(file) ?? []).sort((a, b) => a - b)
  const median = known.length > 0 ? (known[Math.floor(known.length / 2)] ?? 0) : 1
  const cost = (file: string): number => timings.get(file) ?? median
  const slices: Slice[] = Array.from({ length: count }, () => ({ files: [], predictedMs: 0 }))
  const order = [...files].sort((a, b) => cost(b) - cost(a) || (a < b ? -1 : a > b ? 1 : 0))
  for (const file of order) {
    const lightest = slices.reduce((min, slice) =>
      slice.predictedMs < min.predictedMs ? slice : min
    )
    lightest.files.push(file)
    lightest.predictedMs += cost(file)
  }
  for (const slice of slices) slice.files.sort()
  return slices
}

/** Returns one line per file that is not in exactly one slice. */
export function coverageErrors(files: readonly string[], slices: readonly Slice[]): string[] {
  const seen = new Map<string, number>()
  for (const slice of slices)
    for (const file of slice.files) seen.set(file, (seen.get(file) ?? 0) + 1)
  const errors = files.flatMap((file) =>
    seen.get(file) === 1 ? [] : [`${file} is in ${seen.get(file) ?? 0} shards`]
  )
  const listed = new Set(files)
  for (const file of seen.keys()) if (!listed.has(file)) errors.push(`${file} is not a test file`)
  return errors
}

async function readTimings(paths: readonly string[]): Promise<Map<string, number>> {
  const timings = new Map<string, number>()
  for (const path of paths) {
    const parsed = (await Bun.file(path).json()) as Partial<TimingsFile>
    if (parsed.version !== 1 || typeof parsed.files !== 'object' || parsed.files === null) {
      throw new Error(`${path} is not a bun --timings file`)
    }
    for (const [file, ms] of Object.entries(parsed.files)) {
      if (typeof ms !== 'number') throw new Error(`${path}: ${file} has no millisecond count`)
      timings.set(file, ms)
    }
  }
  return timings
}

if (import.meta.main) {
  const [indexArg, countArg, outDir, ...timingsPaths] = process.argv.slice(2)
  const index = Number(indexArg)
  const count = Number(countArg)
  if (
    !outDir ||
    !Number.isInteger(count) ||
    count < 1 ||
    !Number.isInteger(index) ||
    index < 1 ||
    index > count
  ) {
    console.error('usage: bun scripts/ci/shard.ts <index> <count> <out-dir> [timings.json ...]')
    process.exit(2)
  }

  const files = await listTestFiles()
  const timings = await readTimings(timingsPaths)
  const slices = partition(files, timings, count)
  const errors = coverageErrors(files, slices)
  for (const error of errors) console.log(`::error::${error}`)
  if (files.length === 0 || errors.length > 0) process.exit(1)

  const timed = files.filter((file) => timings.has(file)).length
  console.log(
    `${files.length} test files, ${timed} with timings, in ${count} shards, each file once`
  )
  slices.forEach((slice, i) => {
    const mark = i + 1 === index ? '  <- this shard' : ''
    console.log(
      `shard ${i + 1}: ${slice.files.length} files, predicted ${(slice.predictedMs / 1000).toFixed(1)} s${mark}`
    )
  })

  const own = slices[index - 1] as Slice
  const ownTimings: TimingsFile = {
    version: 1,
    files: Object.fromEntries(
      own.files.flatMap((file) => {
        const ms = timings.get(file)
        return ms === undefined ? [] : [[file, ms] as const]
      })
    )
  }
  mkdirSync(outDir, { recursive: true })
  await Bun.write(join(outDir, 'files.txt'), `${own.files.join('\n')}\n`)
  await Bun.write(join(outDir, 'timings.json'), `${JSON.stringify(ownTimings, null, 2)}\n`)
}
