import { expect, test } from 'bun:test'
import { AREA_WEIGHTS, type Scored, unweightedScore, weightedScore } from '../weights'

test('area weights sum to 100', () => {
  expect(Object.values(AREA_WEIGHTS).reduce((sum, w) => sum + w, 0)).toBe(100)
})

const FIXTURE: Scored[] = [
  { area: 'chat', status: 'ported' },
  { area: 'chat', status: 'partial' },
  { area: 'composer', status: 'ported' },
  { area: 'terminal', status: 'broken' },
  { area: 'settings', status: 'ported' },
  { area: 'settings', status: 'pending-unit' }
]

test('unweighted score is the mean status score', () => {
  // (1 + 0.5 + 1 + 0 + 1 + 0) / 6
  expect(unweightedScore(FIXTURE)).toBeCloseTo(58.333, 3)
})

test('usage-weighted score weights each area mean by the area weight', () => {
  // (17 * 0.75 + 14 * 1 + 14 * 0 + 4 * 0.5) / (17 + 14 + 14 + 4) = 28.75 / 49
  expect(weightedScore(FIXTURE)).toBeCloseTo(58.673, 3)
})

test('an unreviewed difference scores nothing', () => {
  const rows: Scored[] = [
    { area: 'chat', status: 'ported' },
    { area: 'chat', status: 'needs-review' }
  ]
  expect(unweightedScore(rows)).toBe(50)
  expect(weightedScore([{ area: 'chat', status: 'needs-review' }])).toBe(0)
})
