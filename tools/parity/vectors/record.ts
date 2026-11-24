/**
 * Test-vector recorder: runs a TypeScript store on scripted events and keeps
 * the input and the store's state after every step as JSON. The Rust model in
 * `apps/rune/crates/model` replays the same files (`tests/vectors.rs`).
 *
 * A store joins by implementing {@link StoreRecorder} and listing scenarios;
 * `record.test.ts` mocks the modules a store reaches for (router, log, host
 * API), records every store, and compares with `out/`. Rewrite `out/` with
 *   RECORD_VECTORS=1 bun run test tools/parity/vectors
 * A unit test can record its own case with {@link recordVector}.
 */

import { createHash } from 'node:crypto'
import { mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

/** One store action: `getState()[action](...args)` on a Zustand store. */
export interface VectorEvent {
  action: string
  args: unknown[]
}

export interface VectorStep {
  /** A state change, or a read when `query` is true. */
  event: VectorEvent
  query?: true
  /** Ids the store drew from `crypto.randomUUID()` during the step, in order. */
  ids: string[]
  /** Calls the store made outside itself (route, log, host command). */
  effects: unknown[]
  /** What the action returned; omitted when a state change returned nothing. */
  result?: unknown
  state: unknown
}

export interface Vector {
  store: string
  name: string
  /** Environment the store ran in (platform). The replay sets the same. */
  context?: Record<string, unknown>
  initial: unknown
  steps: VectorStep[]
}

export interface Scenario {
  name: string
  context?: Record<string, unknown>
  /** Applied to the store before the first step and recorded as `initial`. */
  initial?: Record<string, unknown>
  /** A step with `query: true` is a read and must not change the state. */
  steps: Array<VectorEvent & { query?: true }>
}

export interface StoreRecorder {
  store: string
  /** Repo-relative TypeScript files whose behaviour the vectors pin. */
  sources: string[]
  /** Put the store back to its initial state for `context`. */
  reset(context: Record<string, unknown>): void | Promise<void>
  /** Apply a scenario's `initial`. */
  seed(initial: Record<string, unknown>): void
  /** The store's data, without functions. */
  snapshot(): unknown
  dispatch(event: VectorEvent): unknown
}

/** What a mocked read returns, keyed by the read (e.g. `project-prefs`). */
export const reads = new Map<string, unknown>()

/** How the mocked prompt dispatches answer, in order; a vector's `context.dispatch`. */
export const dispatchOutcomes: unknown[] = []

/** The error code a mocked read fails with, keyed by the read. Wins over `reads`. */
export const readFailures = new Map<string, string>()

const sink = { effects: [] as unknown[], ids: [] as string[], counter: 0 }

/** Record a call the store made outside itself. Mocks call this. */
export function emit(effect: Record<string, unknown>): void {
  sink.effects.push(plain(effect))
}

/** Stand-in for `crypto.randomUUID()`: `id-1`, `id-2`, ... per vector. */
export function drawId(): `${string}-${string}-${string}-${string}-${string}` {
  sink.counter += 1
  const id = `id-${sink.counter}`
  sink.ids.push(id)
  return id as `${string}-${string}-${string}-${string}-${string}`
}

function plain(value: unknown): unknown {
  return value === undefined ? null : JSON.parse(JSON.stringify(value))
}

function drain(): { ids: string[]; effects: unknown[] } {
  const drained = { ids: sink.ids, effects: sink.effects }
  sink.ids = []
  sink.effects = []
  return drained
}

export async function recordVector(recorder: StoreRecorder, scenario: Scenario): Promise<Vector> {
  const context = scenario.context ?? {}
  sink.counter = 0
  await recorder.reset(context)
  if (scenario.initial) recorder.seed(scenario.initial)
  drain()
  const initial = plain(recorder.snapshot())
  const steps: VectorStep[] = []
  for (const { query, ...event } of scenario.steps) {
    const returned = await recorder.dispatch(event)
    const { ids, effects } = drain()
    steps.push({
      event,
      ...(query ? { query } : {}),
      ids,
      effects,
      ...(returned === undefined && !query ? {} : { result: plain(returned) }),
      state: plain(recorder.snapshot())
    })
  }
  return {
    store: recorder.store,
    name: scenario.name,
    ...(scenario.context ? { context: scenario.context } : {}),
    initial,
    steps
  }
}

export const REPO_ROOT = join(import.meta.dir, '..', '..', '..')
export const OUT_DIR = join(import.meta.dir, 'out')

/** SHA-1 of a source file with CRLF read as LF; `tests/vectors.rs` computes the same. */
export function sourceHash(repoPath: string): string {
  const text = readFileSync(join(REPO_ROOT, repoPath), 'utf8').replace(/\r\n/g, '\n')
  return createHash('sha1').update(text).digest('hex')
}

export interface Manifest {
  /** Per store: the hashes of its sources and its vector names, in `out/<store>.json`. */
  stores: Record<string, { sources: Record<string, string>; vectors: string[] }>
}

export function manifestEntry(
  recorder: StoreRecorder,
  vectors: Vector[]
): Manifest['stores'][string] {
  return {
    sources: Object.fromEntries(recorder.sources.map((path) => [path, sourceHash(path)])),
    vectors: vectors.map((vector) => vector.name)
  }
}

/** One store's vectors: `out/<store>.json` holds them as an array. */
export function storeText(vectors: Vector[]): string {
  return `${JSON.stringify(vectors, null, 1)}\n`
}

/**
 * Replace the vector files in `dir` (`out/` by default) with these stores'
 * vectors and their manifest. Subfolders, which other recorders own, stay.
 */
export function writeOut(manifest: Manifest, byStore: Map<string, Vector[]>, dir = OUT_DIR): void {
  mkdirSync(dir, { recursive: true })
  for (const file of readdirSync(dir)) {
    if (file.endsWith('.json')) rmSync(join(dir, file))
  }
  for (const [store, vectors] of byStore) {
    writeFileSync(join(dir, `${store}.json`), storeText(vectors))
  }
  writeFileSync(join(dir, 'manifest.json'), `${JSON.stringify(manifest, null, 2)}\n`)
}
