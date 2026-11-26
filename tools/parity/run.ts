/**
 * `bun run parity:matrix [--client react] [--against react] [--profile desktop|phone|gallery]
 *   [--only id,id] [--area a,b] [--client-dist DIR] [--against-dist DIR] [--port N] [--out DIR] [--state DIR]
 *   [--server-bin FILE] [--reruns N] [--report-only]`
 *
 * Seeds one troupe-server (`world.ts`), then drives every selected surface
 * of the profile (`desktop` by default, or the 390x844 touch `phone`)
 * in the reference client (`--against`, or the surface's own reference) and
 * the candidate client (`--client`), each from a fresh device on the
 * dashboard. Records every action and check, grabs one frame per step,
 * diffs each step's frames, and merges the result into `<out>/results.json`
 * (a partial run updates only the surfaces it ran). A surface whose run
 * failed is rerun up to `--reruns` times and listed under flakes. Writes
 * `<out>/report.md` and prints a line per surface.
 *
 * The Rune clients (`rune-web`, `rune-desktop`) run only the surfaces the
 * Rune shell draws (`RUNE_SHELL_SURFACES`); every other surface is pending
 * on the unit that ports it, and a run of pending surfaces alone starts no
 * server. `--profile gallery` compares the design-system registry with the
 * Rune gallery instead (`gallery.ts`).
 */

import { mkdirSync, readFileSync, renameSync, writeFileSync } from 'node:fs'
import { rm } from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import { parseArgs } from 'node:util'
import { type Browser, chromium } from 'playwright'
import { cropPng, cropRects, diffPngs, type PartRects, type Rect } from './diff'
import { REFERENCE_ARGS } from './drivers/chromium'
import type { ClientLauncher, Driver, Points } from './drivers/driver'
import { errorLine } from './drivers/driver'
import { reactClient } from './drivers/react'
import { runePending } from './drivers/rune-apps'
import { runeDesktopClient } from './drivers/rune-desktop'
import { runeWebArgs, runeWebClient } from './drivers/rune-web'
import { tauriClient } from './drivers/tauri'
import { runGallery } from './gallery'
import { maxPixels, renderReport, summaryLine } from './report'
import { pickPort, REPO_DIR } from './server'
import { autoStatus } from './status'
import { SURFACES } from './surfaces'
import { killTerminals } from './terminals'
import {
  CLIENTS,
  type Client,
  type ClientRun,
  PROFILES,
  type Profile,
  type Rerun,
  type SavedResults,
  type StepDiff,
  type Surface,
  type SurfaceResult
} from './types'
import {
  closeSpareAgents,
  createWorld,
  prepareDevice,
  resetSharedSettings,
  type World
} from './world'

const ROOT = process.env.TROUPE_PARITY_ROOT ?? '/tmp/troupe-parity'

const { values } = parseArgs({
  args: Bun.argv.slice(2),
  options: {
    client: { type: 'string', default: 'react' },
    against: { type: 'string', default: 'react' },
    profile: { type: 'string', default: 'desktop' },
    only: { type: 'string' },
    area: { type: 'string' },
    'client-dist': { type: 'string', default: path.join(REPO_DIR, 'dist-web') },
    'against-dist': { type: 'string', default: path.join(REPO_DIR, 'dist-web') },
    port: { type: 'string' },
    out: { type: 'string', default: path.join(ROOT, 'out') },
    state: { type: 'string', default: path.join(ROOT, 'state') },
    'server-bin': { type: 'string' },
    reruns: { type: 'string', default: '2' },
    'report-only': { type: 'boolean', default: false }
  }
})

function parseProfile(name: string): Profile {
  if (!(PROFILES as readonly string[]).includes(name))
    throw new Error(`unknown profile ${name}; one of ${PROFILES.join(', ')}`)
  return name as Profile
}

const PROFILE = parseProfile(values.profile as string)
const OUT = path.resolve(values.out as string)
/** The desktop profile keeps the plain names; another profile suffixes them, so both can share `--out`. */
const SUFFIX = PROFILE === 'desktop' ? '' : `-${PROFILE}`
const STATE = path.resolve(values.state as string)
const RESULTS = path.join(OUT, `results${SUFFIX}.json`)
const MAX_RERUNS = Number(values.reruns)
if (!Number.isInteger(MAX_RERUNS) || MAX_RERUNS < 0) {
  throw new Error(`--reruns must be a whole number of 0 or more, not ${values.reruns}`)
}
type Role = 'reference' | 'candidate'

function parseClient(name: string): Client {
  if (!(CLIENTS as readonly string[]).includes(name))
    throw new Error(`unknown client ${name}; one of ${CLIENTS.join(', ')}`)
  return name as Client
}

const CLIENT = parseClient(values.client as string)
const AGAINST = parseClient(values.against as string)
const DESKTOP_APPS: readonly Client[] = ['tauri', 'rune-desktop']
if (PROFILE === 'phone' && (DESKTOP_APPS.includes(CLIENT) || DESKTOP_APPS.includes(AGAINST))) {
  throw new Error('the phone profile runs only web clients; a desktop app has no phone shell')
}
/** A Rune client on either side: only the surfaces the Rune shell draws can run. */
const RUNE_SIDE = [CLIENT, AGAINST].some((c) => c.startsWith('rune-'))
/** Why `surface` cannot run in this comparison yet: its own unit, else the Rune shell's. */
const pendingOf = (surface: Surface) =>
  surface.pending ?? (RUNE_SIDE ? runePending(surface) : undefined)
/** The surfaces of this run's profile. */
const PROFILE_SURFACES = SURFACES.filter((s) => (s.profile ?? 'desktop') === PROFILE)
const slug = (id: string) => id.replace(/[^a-z0-9-]/gi, '-')

function selected(): Surface[] {
  const only = values.only?.split(',').filter(Boolean)
  const areas = values.area?.split(',').filter(Boolean)
  const ids = new Set(PROFILE_SURFACES.map((s) => s.id))
  for (const id of only ?? []) if (!ids.has(id)) throw new Error(`unknown ${PROFILE} surface ${id}`)
  return PROFILE_SURFACES.filter(
    (s) => (!only || only.includes(s.id)) && (!areas || areas.includes(s.area))
  )
}

interface Steps {
  run: ClientRun
  rois: (Rect | null)[]
  parts: (PartRects | null)[]
}

async function runSteps(
  surface: Surface,
  role: Role,
  client: Client,
  driver: Driver
): Promise<Steps> {
  const started = performance.now()
  const run: ClientRun = { client, ok: true, ms: 0, actions: [], checks: [], frames: [] }
  const rois: (Rect | null)[] = []
  const parts: (PartRects | null)[] = []
  try {
    await driveSteps(surface, role, driver, run, rois, parts)
  } catch (error) {
    run.ok = false
    run.error = errorLine(error)
  }
  run.ms = Math.round(performance.now() - started)
  return { run, rois, parts }
}

async function driveSteps(
  surface: Surface,
  role: Role,
  driver: Driver,
  run: ClientRun,
  rois: (Rect | null)[],
  parts: (PartRects | null)[]
): Promise<void> {
  for (const step of surface.steps) {
    for (const [i, action] of step.actions.entries()) {
      const log = await driver.act(step.name, i, action)
      run.actions.push(log)
      if (!log.ok) run.ok = false
    }
    for (const check of step.expect) {
      if (!driver.check) break
      const checkStarted = performance.now()
      const log = await driver.check(step.name, check)
      log.ms = Math.round(performance.now() - checkStarted)
      run.checks.push(log)
      if (!log.pass) run.ok = false
    }
    const dir = path.join(OUT, slug(surface.id), step.name)
    mkdirSync(dir, { recursive: true })
    const file = path.join(dir, `${role}.png`)
    await driver.grab(file)
    run.frames.push(file)
    rois.push(surface.roi && driver.measure ? await driver.measure(surface.roi) : null)
    parts.push(driver.parts ? await driver.parts() : null)
  }
}

function failedSteps(client: Client, error: unknown, started: number): Steps {
  const run: ClientRun = {
    client,
    ok: false,
    error: errorLine(error),
    ms: Math.round(performance.now() - started),
    actions: [],
    checks: [],
    frames: []
  }
  return { run, rois: [], parts: [] }
}

async function runClient(
  launcher: ClientLauncher,
  role: Role,
  world: World,
  surface: Surface,
  points: Points,
  attemptNumber: number
): Promise<Steps> {
  const started = performance.now()
  try {
    const killed = await killTerminals(world)
    if (killed > 0) console.error(`[parity] closed ${killed} terminals an earlier run left open`)
    const stopped = await closeSpareAgents(world)
    if (stopped > 0) console.error(`[parity] stopped ${stopped} agents an earlier run left running`)
    // A fresh device per attempt: device-scoped layout (open tabs) would carry a failed attempt's state.
    const deviceId = `matrix-${slug(surface.id)}-${role}-${attemptNumber}`
    await prepareDevice(world, deviceId)
    await resetSharedSettings(world)
    await surface.prepare?.(world, deviceId)
    const stateDir = path.join(STATE, 'clients', slug(surface.id), role)
    return await launcher.session({ world, surface, deviceId, points, stateDir }, (driver) =>
      runSteps(surface, role, launcher.client, driver)
    )
  } catch (error) {
    return failedSteps(launcher.client, error, started)
  }
}

function diffSurface(surface: Surface, reference: Steps, candidate: Steps): StepDiff[] {
  const out: StepDiff[] = []
  for (const [i, step] of surface.steps.entries()) {
    const refPng = reference.run.frames[i]
    const candPng = candidate.run.frames[i]
    if (!refPng || !candPng) continue
    const dir = path.dirname(refPng)
    const parts = reference.parts[i] ?? candidate.parts[i] ?? undefined
    const diffPng = path.join(dir, 'diff.png')
    const entry: StepDiff = {
      step: step.name,
      full: diffPngs(refPng, candPng, diffPng, parts),
      diffPng
    }
    const rect = reference.rois[i] ?? candidate.rois[i] ?? surface.roiRect
    if (rect && rect.width >= 4 && rect.height >= 4) {
      const files = {
        ref: path.join(dir, 'roi-reference.png'),
        cand: path.join(dir, 'roi-candidate.png')
      }
      const clamped = cropPng(refPng, rect, files.ref)
      cropPng(candPng, rect, files.cand)
      entry.roi = {
        rect: clamped,
        diff: diffPngs(
          files.ref,
          files.cand,
          path.join(dir, 'roi-diff.png'),
          parts && cropRects(parts, clamped)
        )
      }
    }
    out.push(entry)
  }
  return out
}

/** Why an attempt failed, from its first failed action, check or crash. */
function failureCause(result: SurfaceResult): string {
  for (const [role, run] of [
    ['reference', result.reference],
    ['candidate', result.candidate]
  ] as const) {
    if (!run) continue
    const action = run.actions.find((a) => !a.ok)
    if (action) return `${role}: ${action.action}: ${action.error}`
    const check = run.checks.find((c) => !c.pass)
    if (check) return `${role}: ${check.expect} "${check.text}" failed after ${check.ms} ms`
    if (run.error) return `${role}: ${run.error}`
  }
  return result.auto
}

async function attempt(
  surface: Surface,
  world: World,
  launchers: Record<Role, ClientLauncher>,
  attemptNumber: number
): Promise<SurfaceResult> {
  const points: Points = new Map()
  // A client that cannot find targets itself acts where the other one found them.
  const order: Role[] = launchers.reference.resolvesTargets
    ? ['reference', 'candidate']
    : ['candidate', 'reference']
  const steps = {} as Record<Role, Steps>
  for (const role of order)
    steps[role] = await runClient(launchers[role], role, world, surface, points, attemptNumber)
  const diffs = diffSurface(surface, steps.reference, steps.candidate)
  return {
    id: surface.id,
    ranAt: new Date().toISOString(),
    auto: autoStatus(steps.reference.run, steps.candidate.run, diffs),
    reference: steps.reference.run,
    candidate: steps.candidate.run,
    diffs,
    reruns: []
  }
}

/** The result of a surface that waits on another unit or has no reference. */
function notRun(surface: Surface, pending = surface.pending): SurfaceResult {
  return {
    id: surface.id,
    ranAt: new Date().toISOString(),
    auto: pending ? 'pending-unit' : 'blocked',
    diffs: [],
    reruns: [],
    pending,
    blocked: surface.blocked
  }
}

async function runSurface(
  surface: Surface,
  world: World,
  launcher: (client: Client, role: Role) => ClientLauncher
): Promise<SurfaceResult> {
  if (surface.blocked) return notRun(surface)
  const pending = pendingOf(surface)
  if (pending) return notRun(surface, pending)
  const launchers = {
    reference: launcher(surface.reference ?? AGAINST, 'reference'),
    candidate: launcher(CLIENT, 'candidate')
  }
  const reruns: Rerun[] = []
  for (let n = 0; ; n++) {
    const result = await attempt(surface, world, launchers, n)
    const failed = result.auto === 'broken' || result.auto === 'unverified'
    if (!failed || n >= MAX_RERUNS) return { ...result, reruns }
    const cause = failureCause(result)
    console.error(`[parity] ${surface.id} attempt ${n} failed (${cause}); rerunning`)
    reruns.push({ attempt: n, cause })
  }
}

/** What this run compares: results of another comparison are never merged into it. */
function comparison(): string {
  const side = (client: Client, dist: string) =>
    client === 'react' ? `react (${path.resolve(dist)})` : client
  return `${side(CLIENT, values['client-dist'] as string)} against ${side(AGAINST, values['against-dist'] as string)}, ${PROFILE} profile`
}

/** The results an earlier run saved in `results.json`, or null. */
function readResults(): SavedResults | null {
  try {
    return JSON.parse(readFileSync(RESULTS, 'utf8')) as SavedResults
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') return null
    throw error
  }
}

/** Replace `results.json` whole, so a crash mid-write never leaves half a file. */
function saveResults(saved: SavedResults): void {
  writeFileSync(`${RESULTS}.tmp`, JSON.stringify(saved, null, 2))
  renameSync(`${RESULTS}.tmp`, RESULTS)
}

function writeReport(saved: SavedResults): void {
  const file = path.join(OUT, `report${SUFFIX}.md`)
  writeFileSync(file, renderReport(PROFILE_SURFACES, saved.surfaces, saved.comparison))
  console.error(`[parity] wrote ${file}`)
}

async function main(): Promise<void> {
  mkdirSync(OUT, { recursive: true })
  if (PROFILE === 'gallery') {
    const only = values.only?.split(',').filter(Boolean)
    await runGallery({ client: CLIENT, against: AGAINST, out: OUT, only })
    return
  }
  const previous = readResults()
  if (values['report-only']) {
    if (!previous) throw new Error(`${RESULTS} is missing: run the matrix first`)
    writeReport(previous)
    return
  }
  const saved: SavedResults =
    previous?.comparison === comparison() ? previous : { comparison: comparison(), surfaces: {} }
  if (previous && previous !== saved) {
    console.error(`[parity] ${RESULTS} holds ${previous.comparison}; starting a new comparison`)
  }
  const surfaces = selected()
  if (surfaces.every((s) => s.blocked || pendingOf(s))) {
    for (const surface of surfaces) {
      const result = surface.blocked ? notRun(surface) : notRun(surface, pendingOf(surface))
      saved.surfaces[surface.id] = result
      console.log(summaryLine(result))
    }
    saveResults(saved)
    const blocked = surfaces.filter((s) => s.blocked).length
    const units = [...new Set(surfaces.flatMap((s) => (s.blocked ? [] : [pendingOf(s)?.unit])))]
    console.log(
      `[parity] ${surfaces.length} surfaces: ${surfaces.length - blocked} pending (${units.join(', ')}), ${blocked} blocked`
    )
    writeReport(saved)
    return
  }
  const port = await pickPort(values.port ? Number(values.port) : undefined)
  const root = path.join(STATE, 'world')
  await rm(root, { recursive: true, force: true })
  console.error(`[parity] seeding server on ${port} under ${root}`)
  const world = await createWorld({ port, root, binary: values['server-bin'] })
  let browser: Browser | null = null
  let runeWeb: ClientLauncher | null = null
  const started = performance.now()
  try {
    browser = await chromium.launch({ args: runeWebArgs(REFERENCE_ARGS) })
    const shared = browser
    // One launcher for the whole run: it serves every session from one proxy, and a fresh
    // one per surface left a port in 8190-8199 taken each time until none was free.
    runeWeb = runeWebClient({ browser: shared })
    const runeWebLauncher = runeWeb
    const launchers: Record<Client, (role: Role) => ClientLauncher> = {
      react: (role) =>
        reactClient({
          browser: shared,
          dist: values[role === 'reference' ? 'against-dist' : 'client-dist'] as string
        }),
      tauri: () => tauriClient(),
      'rune-web': () => runeWebLauncher,
      'rune-desktop': () => runeDesktopClient()
    }
    const launcher = (client: Client, role: Role) => launchers[client](role)
    for (const surface of surfaces) {
      const result = await runSurface(surface, world, launcher)
      saved.surfaces[surface.id] = result
      saveResults(saved)
      console.log(summaryLine(result))
    }
  } finally {
    await runeWeb?.close?.()
    await browser?.close()
    world.ws.close()
    await world.server.stop()
  }
  const ran = surfaces.map((s) => saved.surfaces[s.id]).filter((r): r is SurfaceResult => !!r)
  const zero = (kind: 'exact' | 'perceptual') => ran.filter((r) => maxPixels(r, kind) === 0).length
  const minutes = ((performance.now() - started) / 60_000).toFixed(1)
  const load = os
    .loadavg()
    .map((n) => n.toFixed(2))
    .join(' ')
  console.log(
    `[parity] ${ran.length} surfaces in ${minutes} min: zero difference exact ${zero('exact')}, perceptual ${zero('perceptual')}; ${ran.filter((r) => r.reruns.length).length} rerun; load average ${load}`
  )
  writeReport(saved)
}

await main()
