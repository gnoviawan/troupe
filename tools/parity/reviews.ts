/**
 * Verdicts a person wrote after looking at every step's frame pair of a
 * surface (`bun run parity <id>` writes the pairs). A run that differs
 * perceptually reads `needs-review` until a review here covers it. Reviews
 * are kept per candidate client, since a verdict on one client says nothing
 * about another.
 */

import type { Client } from './types'

export interface Review {
  status: 'ported' | 'partial' | 'broken'
  /** What the reviewer saw that differs, in the words a user would use. */
  note: string
  /**
   * The most perceptually mismatched pixels of any step that the reviewer
   * looked at. A later run that differs by more is a change nobody has seen,
   * so the review stops applying to it.
   */
  acceptsPx: number
}

export type Reviews = Partial<Record<Client, Record<string, Review>>>

export const REVIEWS: Reviews = {}
