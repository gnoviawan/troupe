/**
 * The status rule. A run marks a surface `ported` only when no step differs
 * perceptually anywhere in the frame: a 4 px padding change is a few hundred
 * pixels, far under any percentage a threshold could allow. Every other
 * measured difference reads `needs-review`, and only a person's verdict in
 * `reviews.ts` turns it into `ported`, `partial` or `broken`.
 */

import { REVIEWS, type Review, type Reviews } from './reviews'
import type { ClientRun, Status, StepDiff, Surface, SurfaceResult } from './types'

/** The most perceptually mismatched pixels of any step's full frame. */
export function maxPerceptualPx(diffs: StepDiff[]): number {
  return Math.max(0, ...diffs.map((d) => d.full.perceptual.mismatched))
}

/** What a run alone can say about a surface, before any review. */
export function autoStatus(reference: ClientRun, candidate: ClientRun, diffs: StepDiff[]): Status {
  if (!reference.ok) return 'unverified'
  if (!candidate.ok) return 'broken'
  if (diffs.length === 0) return 'unverified'
  return maxPerceptualPx(diffs) === 0 ? 'ported' : 'needs-review'
}

/**
 * A saved result under this rule. The `auto` a run stored can come from an
 * older rule, so a result that kept both runs is classified again.
 */
function measuredStatus(result: SurfaceResult): Status {
  return result.reference && result.candidate
    ? autoStatus(result.reference, result.candidate, result.diffs)
    : result.auto
}

/** The review that covers this run of `surface`, if a person wrote one and the run differs by no more than they saw. */
export function reviewOf(
  surface: Surface,
  result: SurfaceResult | undefined,
  reviews: Reviews = REVIEWS
): Review | undefined {
  if (!result?.candidate || measuredStatus(result) !== 'needs-review') return undefined
  const review = reviews[result.candidate.client]?.[surface.id]
  return review && maxPerceptualPx(result.diffs) <= review.acceptsPx ? review : undefined
}

/** The status the report shows and scores. */
export function statusOf(
  surface: Surface,
  result: SurfaceResult | undefined,
  reviews: Reviews = REVIEWS
): Status {
  if (surface.pending) return 'pending-unit'
  if (surface.blocked) return 'blocked'
  if (!result) return 'unverified'
  return reviewOf(surface, result, reviews)?.status ?? measuredStatus(result)
}
