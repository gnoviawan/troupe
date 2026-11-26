import { describe, expect, test } from 'bun:test'
import { mkdtempSync } from 'node:fs'
import { tmpdir } from 'node:os'
import path from 'node:path'
import { diffPngs } from '../diff'
import { readPng, writePng } from '../png'
import { renderReport } from '../report'
import type { Reviews } from '../reviews'
import { autoStatus, statusOf } from '../status'
import type { Client, ClientRun, StepDiff, Surface, SurfaceResult } from '../types'

const FIXTURES = path.join(import.meta.dir, '../fixtures')
const A = path.join(FIXTURES, 'frame-a.png')
// frame-b differs from frame-a in a 32x32 block: 1,024 pixels.
const B = path.join(FIXTURES, 'frame-b.png')
const dir = mkdtempSync(path.join(tmpdir(), 'parity-status-'))

/** frame-a with one pixel turned to its opposite: 1 of 4,096 pixels, 0.02 percent. */
const ONE_PIXEL = (() => {
  const image = readPng(A)
  const flip = image.data[0] > 127 ? 0 : 255
  image.data.set([flip, flip, flip, 255], 0)
  const file = path.join(dir, 'one-pixel.png')
  writePng(file, image)
  return file
})()

const stepDiff = (step: string, candidate: string): StepDiff => {
  const diffPng = path.join(dir, `${step}-${path.basename(candidate)}`)
  return { step, full: diffPngs(A, candidate, diffPng), diffPng }
}

const run = (client: Client = 'react', ok = true): ClientRun => ({
  client,
  ok,
  ms: 0,
  actions: [],
  checks: [],
  frames: []
})

const SURFACE: Surface = {
  id: 'rail.project-settings',
  area: 'rail',
  title: 'Settings',
  source: { journey: 'starter' },
  steps: []
}

const result = (diffs: StepDiff[], client: Client = 'react'): SurfaceResult => ({
  id: SURFACE.id,
  ranAt: '2026-11-23T00:00:00Z',
  auto: autoStatus(run(), run(client), diffs),
  reference: run(),
  candidate: run(client),
  diffs,
  reruns: []
})

describe('autoStatus', () => {
  test('marks identical frames on every step ported', () => {
    expect(autoStatus(run(), run(), [stepDiff('a', A), stepDiff('b', A)])).toBe('ported')
  })

  test('marks a one-pixel difference needs-review, though it is 0.02 percent of the frame', () => {
    const diffs = [stepDiff('a', A), stepDiff('b', ONE_PIXEL)]
    expect(diffs[1].full.perceptual).toEqual({ mismatched: 1, percent: (1 / 4096) * 100 })
    expect(autoStatus(run(), run(), diffs)).toBe('needs-review')
  })

  test('marks a difference on an earlier step needs-review when the last step matches', () => {
    expect(autoStatus(run(), run(), [stepDiff('a', B), stepDiff('b', A)])).toBe('needs-review')
  })

  test('marks a failed reference unverified and a failed candidate broken', () => {
    const diffs = [stepDiff('a', A)]
    expect(autoStatus(run('react', false), run(), diffs)).toBe('unverified')
    expect(autoStatus(run(), run('react', false), diffs)).toBe('broken')
  })
})

describe('statusOf', () => {
  const reviewed: Reviews = {
    tauri: { [SURFACE.id]: { status: 'ported', note: 'Same modal.', acceptsPx: 1024 } }
  }

  test('a review that saw at least as large a difference decides the status', () => {
    expect(statusOf(SURFACE, result([stepDiff('a', B)], 'tauri'), reviewed)).toBe('ported')
    expect(statusOf(SURFACE, result([stepDiff('a', ONE_PIXEL)], 'tauri'), reviewed)).toBe('ported')
  })

  test('a review lapses when the run differs by more than the reviewer saw', () => {
    const smaller: Reviews = {
      tauri: { [SURFACE.id]: { status: 'ported', note: 'One pixel.', acceptsPx: 1 } }
    }
    expect(statusOf(SURFACE, result([stepDiff('a', B)], 'tauri'), smaller)).toBe('needs-review')
  })

  test("a review of one client does not cover another client's run", () => {
    expect(statusOf(SURFACE, result([stepDiff('a', B)], 'react'), reviewed)).toBe('needs-review')
  })

  test('a result saved under an older rule is classified again', () => {
    const saved = { ...result([stepDiff('a', ONE_PIXEL)]), auto: 'ported' as const }
    expect(statusOf(SURFACE, saved, {})).toBe('needs-review')
  })

  test('a review cannot mark a broken run ported', () => {
    const broken = { ...result([stepDiff('a', B)], 'tauri'), candidate: run('tauri', false) }
    expect(statusOf(SURFACE, broken, reviewed)).toBe('broken')
  })
})

test('the report counts a planted one-pixel change as needs-review and names the review', () => {
  const other: Surface = { ...SURFACE, id: 'rail.new-project' }
  const report = renderReport(
    [SURFACE, other],
    {
      [SURFACE.id]: result([stepDiff('a', ONE_PIXEL)], 'tauri'),
      [other.id]: { ...result([stepDiff('a', ONE_PIXEL)], 'tauri'), id: other.id }
    },
    'tauri against react',
    { tauri: { [other.id]: { status: 'partial', note: 'Gap is wider.', acceptsPx: 1 } } }
  )
  expect(report).toContain(
    'Surfaces: 2 (ported 0, needs-review 1, partial 1, broken 0, blocked 0, unverified 0, pending-unit 0).'
  )
  expect(report).toContain('- `rail.new-project` reviewed partial: Gap is wider.')
  expect(report).toContain('| rail | partial (reviewed) |')
})
