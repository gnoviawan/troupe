/**
 * The gallery profile: every `design-system` registry entry against the Rune
 * gallery scene of the same id, one frame each. The entry list is
 * `apps/rune/crates/gallery/registry.json`, which the gallery's own test keeps
 * equal to its scene table. The reference (`--against react`) is the
 * design-system canvas; the candidate is the Rune gallery's wasm build
 * (`rune-web`) or its native `--snap` (`rune-desktop`). The run writes
 * `results-gallery.json`, `report-gallery.md`, and per scene the two frames,
 * the diff and `pair.png` (reference | candidate | diff) under
 * `<out>/gallery/<id>/`.
 */

import { mkdirSync, readFileSync, renameSync, writeFileSync } from 'node:fs'
import path from 'node:path'
import { chromium } from 'playwright'
import registry from '../../apps/rune/crates/gallery/registry.json'
import { diffPngs, type PartRects } from './diff'
import { REFERENCE_ARGS } from './drivers/chromium'
import { designSystemGallery } from './drivers/design-system'
import { errorLine, type GalleryEntry, type GallerySession } from './drivers/driver'
import { runeDesktopGallery } from './drivers/rune-desktop'
import { runeWebArgs, runeWebGallery } from './drivers/rune-web'
import { blankImage, blit, readPng, writePng } from './png'
import { renderReport, summaryLine } from './report'
import { autoStatus, statusOf } from './status'
import type { Client, ClientRun, SavedResults, Status, Surface, SurfaceResult } from './types'

export const GALLERY_ENTRIES: readonly GalleryEntry[] = registry

/**
 * One surface per registry entry, keyed by its registry id. They score in the
 * `theme` area: the gallery is the theme and the base components.
 */
export const GALLERY_SURFACES: readonly Surface[] = GALLERY_ENTRIES.map((entry) => ({
  id: entry.id,
  area: 'theme',
  title: entry.name,
  source: { journey: 'design-system registry' },
  profile: 'gallery',
  steps: [{ name: 'scene', actions: [], expect: [] }]
}))

type Open = (browser: import('playwright').Browser) => Promise<GallerySession>

const GALLERIES: Record<Client, Open | null> = {
  react: designSystemGallery,
  'rune-web': runeWebGallery,
  'rune-desktop': () => runeDesktopGallery(),
  tauri: null
}

function opener(client: Client): Open {
  const open = GALLERIES[client]
  if (!open)
    throw new Error(
      `${client} has no gallery; the gallery profile runs react, rune-web and rune-desktop`
    )
  return open
}

const slug = (id: string) => id.replace(/[^a-z0-9.-]/gi, '-')

interface Shot {
  run: ClientRun
  parts: PartRects | null
}

async function shoot(
  session: GallerySession,
  client: Client,
  entry: GalleryEntry,
  file: string
): Promise<Shot> {
  const started = performance.now()
  const run: ClientRun = {
    client,
    ok: true,
    ms: 0,
    actions: [],
    checks: [],
    frames: [],
    renderer: session.renderer
  }
  let parts: PartRects | null = null
  try {
    const shot = await session.shoot(entry, file)
    parts = shot.parts
    run.actions.push({ step: 'scene', action: `show ${entry.id}`, via: shot.via, ok: true })
    run.frames.push(file)
  } catch (error) {
    run.ok = false
    run.error = errorLine(error)
    run.actions.push({
      step: 'scene',
      action: `show ${entry.id}`,
      via: 'none',
      ok: false,
      error: run.error
    })
  }
  run.ms = Math.round(performance.now() - started)
  return { run, parts }
}

/** reference | candidate | diff, 8 px apart, for the person who reviews the scene. */
function writePair(files: string[], out: string): void {
  const images = files.map(readPng)
  const height = Math.max(...images.map((i) => i.height))
  const width = images.reduce((sum, i) => sum + i.width, 0) + 8 * (images.length - 1)
  const sheet = blankImage(width, height, 255)
  let x = 0
  for (const image of images) {
    blit(image, sheet, 0, 0, image.width, image.height, x, 0)
    x += image.width + 8
  }
  writePng(out, sheet)
}

function compare(
  entry: GalleryEntry,
  dir: string,
  reference: Shot,
  candidate: Shot
): SurfaceResult {
  const result: SurfaceResult = {
    id: entry.id,
    ranAt: new Date().toISOString(),
    auto: 'unverified',
    reference: reference.run,
    candidate: candidate.run,
    diffs: [],
    reruns: []
  }
  const [refPng, candPng] = [reference.run.frames[0], candidate.run.frames[0]]
  if (refPng && candPng) {
    const diffPng = path.join(dir, 'diff.png')
    try {
      const parts = reference.parts ?? candidate.parts ?? undefined
      result.diffs.push({ step: 'scene', full: diffPngs(refPng, candPng, diffPng, parts), diffPng })
      writePair([refPng, candPng, diffPng], path.join(dir, 'pair.png'))
    } catch (error) {
      candidate.run.ok = false
      candidate.run.error = errorLine(error)
    }
  }
  result.auto = autoStatus(reference.run, candidate.run, result.diffs)
  return result
}

export interface GalleryRun {
  client: Client
  against: Client
  out: string
  /** Registry ids to run; every entry when absent. */
  only?: string[]
}

export async function runGallery(opts: GalleryRun): Promise<Record<string, SurfaceResult>> {
  const resultsFile = path.join(opts.out, 'results-gallery.json')
  const comparison = `${opts.client} against ${opts.against}, gallery profile`
  const ids = new Set(GALLERY_ENTRIES.map((e) => e.id))
  for (const id of opts.only ?? []) if (!ids.has(id)) throw new Error(`unknown gallery entry ${id}`)
  const entries = GALLERY_ENTRIES.filter((e) => !opts.only || opts.only.includes(e.id))
  let saved: SavedResults = { comparison, surfaces: {} }
  try {
    const previous = JSON.parse(readFileSync(resultsFile, 'utf8')) as SavedResults
    if (previous.comparison === comparison) saved = previous
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error
  }
  const started = performance.now()
  const browser = await chromium.launch({ args: runeWebArgs(REFERENCE_ARGS) })
  const sessions: GallerySession[] = []
  try {
    const reference = await opener(opts.against)(browser)
    sessions.push(reference)
    const candidate = await opener(opts.client)(browser)
    sessions.push(candidate)
    console.error(
      `[parity] gallery: reference ${reference.renderer}, candidate ${candidate.renderer}`
    )
    for (const entry of entries) {
      const dir = path.join(opts.out, 'gallery', slug(entry.id))
      mkdirSync(dir, { recursive: true })
      const ref = await shoot(reference, opts.against, entry, path.join(dir, 'reference.png'))
      const cand = await shoot(candidate, opts.client, entry, path.join(dir, 'candidate.png'))
      const result = compare(entry, dir, ref, cand)
      saved.surfaces[entry.id] = result
      writeFileSync(`${resultsFile}.tmp`, JSON.stringify(saved, null, 2))
      renameSync(`${resultsFile}.tmp`, resultsFile)
      console.log(summaryLine(result))
    }
  } finally {
    for (const session of sessions.reverse()) await session.close()
    await browser.close()
  }
  const report = path.join(opts.out, 'report-gallery.md')
  writeFileSync(report, renderReport([...GALLERY_SURFACES], saved.surfaces, saved.comparison))
  const statuses = entries.map((e) =>
    statusOf(GALLERY_SURFACES.find((s) => s.id === e.id) as Surface, saved.surfaces[e.id])
  )
  const byStatus = new Map<Status, number>()
  for (const status of statuses) byStatus.set(status, (byStatus.get(status) ?? 0) + 1)
  const minutes = ((performance.now() - started) / 60_000).toFixed(1)
  console.log(
    `[parity] gallery: ${entries.length} scenes in ${minutes} min: ${[...byStatus].map(([s, n]) => `${s} ${n}`).join(', ')}`
  )
  console.error(`[parity] wrote ${report}`)
  return saved.surfaces
}
