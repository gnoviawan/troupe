/**
 * Usage weights per area (they sum to 100), the score each status earns, and
 * the two scores a run reports. Area weighting keeps an area with many small
 * surfaces (settings, themes) from dominating the score.
 */

import type { AreaId, Status } from './types'

export const AREA_WEIGHTS: Record<AreaId, number> = {
  chat: 17,
  composer: 14,
  terminal: 14,
  rail: 9,
  git: 9,
  explorer: 7,
  editor: 7,
  'side-panel': 5,
  settings: 4,
  statusbar: 1,
  palette: 3,
  theme: 2,
  shortcuts: 1,
  dialogs: 1,
  notifications: 1,
  browser: 1,
  canvas: 0.5,
  ssh: 0.5,
  remote: 0.5,
  snapshots: 0.5,
  automations: 1,
  github: 1
}

/**
 * Ported counts fully, partial counts half, everything else counts zero. An
 * unreviewed difference counts zero: only a person's verdict in `reviews.ts`
 * can turn a difference into score, so no run raises the score by itself.
 */
export const STATUS_SCORE: Record<Status, number> = {
  ported: 1,
  'needs-review': 0,
  partial: 0.5,
  broken: 0,
  blocked: 0,
  unverified: 0,
  'pending-unit': 0
}

export interface Scored {
  area: AreaId
  status: Status
}

const mean = (rows: Scored[]) =>
  rows.length ? rows.reduce((sum, r) => sum + STATUS_SCORE[r.status], 0) / rows.length : 0

/** Mean status score over every row, 0 to 100. */
export function unweightedScore(rows: Scored[]): number {
  return mean(rows) * 100
}

/**
 * Mean status score within each area, weighted by the area's usage weight,
 * over the areas that have rows, 0 to 100.
 */
export function weightedScore(rows: Scored[]): number {
  const byArea = new Map<AreaId, Scored[]>()
  for (const row of rows) byArea.set(row.area, [...(byArea.get(row.area) ?? []), row])
  let total = 0
  let weight = 0
  for (const [area, list] of byArea) {
    total += AREA_WEIGHTS[area] * mean(list)
    weight += AREA_WEIGHTS[area]
  }
  return weight ? (total / weight) * 100 : 0
}
