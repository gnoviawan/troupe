/**
 * The matrix report: scores, the per-area table, one row per surface with its
 * last step's numbers, the flakes the run reran, and every driven interaction
 * with its literal result. `renderReport` is pure; `run.ts` writes it to
 * `<out>/report.md` from `results.json`.
 */

import { type DiffResult, PARTS } from './diff'
import { REVIEWS, type Reviews } from './reviews'
import { reviewOf, statusOf } from './status'
import type { AreaId, ClientRun, Status, Surface, SurfaceResult } from './types'
import { AREA_WEIGHTS, type Scored, unweightedScore, weightedScore } from './weights'

const STATUSES: Status[] = [
  'ported',
  'needs-review',
  'partial',
  'broken',
  'blocked',
  'unverified',
  'pending-unit'
]

const pct = (n: number) => `${n.toFixed(1)}%`
const cell = (text: string) => text.replace(/\\/g, '\\\\').replace(/\|/g, '\\|').replace(/\n/g, ' ')

const unmeasured = (status: Status) =>
  status === 'pending-unit' || status === 'blocked' || status === 'unverified'

function passed(run?: ClientRun): string {
  if (!run) return 'n/a'
  const total = run.checks.length + run.actions.length
  const ok = run.checks.filter((c) => c.pass).length + run.actions.filter((a) => a.ok).length
  return run.error && total === 0 ? 'crash' : `${ok}/${total}`
}

const counts = (d: DiffResult) => `${pct(d.exact.percent)} / ${d.perceptual.percent.toFixed(2)}%`
const partCells = (d: DiffResult) =>
  PARTS.map((p) => `${d.parts[p].perceptual.percent.toFixed(2)}%`)

/** The most mismatched pixels, counted `kind`, of any step of the surface's full frames. */
export function maxPixels(
  result: SurfaceResult | undefined,
  kind: 'exact' | 'perceptual'
): number | null {
  if (!result?.diffs.length) return null
  return Math.max(...result.diffs.map((d) => d.full[kind].mismatched))
}

/** One line per surface, for the console. */
export function summaryLine(result: SurfaceResult): string {
  const last = result.diffs.at(-1)
  const failed = (run?: ClientRun) =>
    run
      ? [
          ...run.actions.filter((a) => !a.ok).map((a) => `${a.action}: ${a.error}`),
          ...run.checks.filter((c) => !c.pass).map((c) => `${c.expect} "${c.text}"`),
          ...(run.error ? [run.error] : [])
        ].join('; ')
      : ''
  const diff = last
    ? `max exact ${maxPixels(result, 'exact')} px, perceptual ${maxPixels(result, 'perceptual')} px; last ${counts(last.full)}` +
      (last.roi ? ` roi ${counts(last.roi.diff)}` : '') +
      ` text/icons/boxes ${partCells(last.roi?.diff ?? last.full).join(' ')}`
    : ''
  const reruns = result.reruns.length ? ` reruns ${result.reruns.length}` : ''
  return `${result.auto.padEnd(12)} ${result.id.padEnd(30)} ${diff}${reruns}  ref[${failed(result.reference)}] cand[${failed(result.candidate)}]`
}

export function renderReport(
  surfaces: Surface[],
  results: Record<string, SurfaceResult>,
  comparison: string,
  reviews: Reviews = REVIEWS
): string {
  const rows = surfaces.map((surface) => {
    const result = results[surface.id]
    return {
      surface,
      result,
      area: surface.area,
      status: statusOf(surface, result, reviews),
      review: reviewOf(surface, result, reviews)
    }
  })
  const measured = rows.filter((r) => !unmeasured(r.status))
  const zero = (kind: 'exact' | 'perceptual') =>
    rows.filter((r) => maxPixels(r.result, kind) === 0).length
  const out: string[] = [
    `# Parity matrix: ${comparison}`,
    '',
    `Surfaces: ${rows.length} (${STATUSES.map((s) => `${s} ${rows.filter((r) => r.status === s).length}`).join(', ')}).`,
    `Zero difference on every step: exact ${zero('exact')} of ${rows.length}, perceptual ${zero('perceptual')} of ${rows.length}.`,
    '',
    '| Score | All surfaces | Measured surfaces only |',
    '| --- | ---: | ---: |',
    `| Unweighted | ${pct(unweightedScore(rows))} | ${pct(unweightedScore(measured))} |`,
    `| Usage-weighted | ${pct(weightedScore(rows))} | ${pct(weightedScore(measured))} |`,
    '',
    '## By area',
    '',
    '| Area | Weight | Surfaces | Ported | Needs review | Partial | Broken | Not measured | Area score |',
    '| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |'
  ]
  const byArea = new Map<AreaId, Scored[]>()
  for (const row of rows) byArea.set(row.area, [...(byArea.get(row.area) ?? []), row])
  for (const [area, list] of [...byArea].sort((a, b) => AREA_WEIGHTS[b[0]] - AREA_WEIGHTS[a[0]])) {
    const n = (status: Status) => list.filter((r) => r.status === status).length
    out.push(
      `| ${area} | ${AREA_WEIGHTS[area]} | ${list.length} | ${n('ported')} | ${n('needs-review')} | ${n('partial')} | ${n('broken')} | ${list.filter((r) => unmeasured(r.status)).length} | ${pct(unweightedScore(list))} |`
    )
  }

  out.push(
    '',
    '## Surfaces',
    '',
    'Status is `ported` only when no step differs perceptually, or when a review in `tools/parity/reviews.ts` covers the difference (marked reviewed). Any other difference reads `needs-review`. Reference and candidate count passed actions plus checks over the total. Mismatch is exact / perceptual for the last step, over the full frame (1280x800 on desktop, 390x844 on the phone) and over the region the surface names. Text, icons and boxes are the perceptual mismatch within the pixels of each part, over the region when there is one. Max exact and max perceptual are the most mismatched pixels in the full frame of any step.',
    '',
    '| Surface | Area | Status | Reference | Candidate | Full | Region | Text | Icons | Boxes | Max exact px | Max perceptual px |',
    '| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |'
  )
  for (const { surface, result, status, review } of rows) {
    const last = result?.diffs.at(-1)
    const parts = last ? partCells(last.roi?.diff ?? last.full) : ['', '', '']
    out.push(
      `| \`${surface.id}\` ${cell(surface.title)} | ${surface.area} | ${review ? `${status} (reviewed)` : status} | ${passed(result?.reference)} | ${passed(result?.candidate)} | ${last ? counts(last.full) : ''} | ${last?.roi ? counts(last.roi.diff) : ''} | ${parts.join(' | ')} | ${maxPixels(result, 'exact') ?? ''} | ${maxPixels(result, 'perceptual') ?? ''} |`
    )
  }

  out.push('', '## Differences', '')
  const differing = rows.filter((r) => (maxPixels(r.result, 'perceptual') ?? 0) > 0)
  if (differing.length === 0) out.push('No step of any surface differs perceptually.')
  for (const { surface, result, review } of differing) {
    if (review) out.push(`- \`${surface.id}\` reviewed ${review.status}: ${cell(review.note)}`)
    for (const d of result?.diffs ?? []) {
      if (!d.full.changed) continue
      const { x, y, width, height } = d.full.changed
      const [text, icons, boxes] = partCells(d.full)
      out.push(
        `- \`${surface.id}\` \`${d.step}\`: ${d.full.perceptual.mismatched} px in the ${width}x${height} region at ${x},${y} (text ${text}, icons ${icons}, boxes ${boxes})`
      )
    }
  }

  const flaky = rows.filter((r) => r.result?.reruns.length)
  out.push('', '## Flakes', '')
  if (flaky.length === 0) out.push('No surface needed a rerun.')
  else {
    out.push('| Surface | Attempt | Cause | Final status |', '| --- | ---: | --- | --- |')
    for (const { surface, result, status } of flaky) {
      for (const rerun of result?.reruns ?? []) {
        out.push(`| \`${surface.id}\` | ${rerun.attempt} | ${cell(rerun.cause)} | ${status} |`)
      }
    }
  }

  out.push('', '## Driven interactions and literal results', '')
  for (const { surface, result } of rows) {
    if (!result?.reference) {
      const pending = surface.pending ?? result?.pending
      out.push(
        `- \`${surface.id}\`: ${pending ? `pending ${pending.unit}: ${pending.reason}` : (surface.blocked ?? 'not run')}`
      )
      continue
    }
    out.push(`- \`${surface.id}\`:`)
    for (const step of surface.steps) {
      const of = <T extends { step: string }>(list?: T[]) =>
        list?.filter((x) => x.step === step.name) ?? []
      const candActions = of(result.candidate?.actions)
      const candChecks = of(result.candidate?.checks)
      const refChecks = of(result.reference.checks)
      const parts = of(result.reference.actions).map((a, i) => {
        const c = candActions[i]
        const cand = c ? `${c.ok ? 'ok' : `FAIL ${c.error}`} via ${c.via}` : 'not run'
        return `${a.action} (reference ${a.ok ? 'ok' : `FAIL ${a.error}`}; candidate ${cand})`
      })
      const checks = refChecks.length ? refChecks : candChecks
      for (const [i, check] of checks.entries()) {
        const ref = refChecks[i]
        const cand = candChecks[i]
        const say = (c?: typeof check) =>
          c ? `${c.pass ? 'pass' : 'FAIL'} ${c.ms ?? 0} ms` : 'not read'
        parts.push(`${check.expect} "${check.text}": reference ${say(ref)}, candidate ${say(cand)}`)
      }
      out.push(`  - \`${step.name}\`: ${parts.length ? cell(parts.join('; ')) : 'frame only'}`)
    }
    if (result.reference.error) out.push(`  - reference error: ${cell(result.reference.error)}`)
    if (result.candidate?.error) out.push(`  - candidate error: ${cell(result.candidate.error)}`)
  }
  return `${out.join('\n')}\n`
}
