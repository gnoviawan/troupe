import { describe, expect, it } from 'vitest'
import { formatRunsReport, rankSumP } from '../runs-report.ts'
import { aggregate, median } from '../stats.ts'

describe('rankSumP', () => {
  // Textbook separable case (n = m = 3): every A value outranks every B.
  // The Wilcoxon rank-sum table gives a two-sided p of 0.1 — the smallest
  // attainable for this size.
  it('returns 0.1 for fully separated 3v3 samples', () => {
    expect(rankSumP([7, 8, 9], [1, 2, 3])).toBeCloseTo(0.1, 9)
    expect(rankSumP([1, 2, 3], [7, 8, 9])).toBeCloseTo(0.1, 9)
  })

  it('returns 1 for identical samples', () => {
    expect(rankSumP([1, 2, 3], [1, 2, 3])).toBeCloseTo(1, 9)
  })

  it('interleaved samples stay insignificant', () => {
    // {2,4,6} vs {1,3,5}: A sums to 12, expected 10.5 — well inside the band.
    const p = rankSumP([2, 4, 6], [1, 3, 5])
    expect(p).toBeGreaterThan(0.5)
  })

  it('falls back to the normal approximation above n=14 and stays sane', () => {
    // Fully separated 8v8: the exact two-sided p is ~0.0002.
    const p = rankSumP([9, 10, 11, 12, 13, 14, 15, 16], [1, 2, 3, 4, 5, 6, 7, 8])
    expect(p).toBeLessThan(0.01)
    expect(p).toBeGreaterThan(0)
  })
})

describe('median and aggregate', () => {
  it('median picks the upper-middle element for even counts', () => {
    expect(median([3, 1, 2])).toBe(2)
    expect(median([4, 1, 3, 2])).toBe(3)
    expect(median([])).toBe(0)
  })

  it('aggregate reports median/min/max/n per key', () => {
    const agg = aggregate([{ a: 1, b: 10 }, { a: 3 }, { a: 2, b: 30 }])
    expect(agg.a).toEqual({ median: 2, min: 1, max: 3, n: 3 })
    expect(agg.b).toEqual({ median: 30, min: 10, max: 30, n: 2 })
  })
})

describe('formatRunsReport', () => {
  const run = (client: string, scenario: string, n: number, value: number) => ({
    meta: {
      scenario,
      run: n,
      client,
      status: 'ok',
      preRunBusyPct: 10,
      quietWaitS: 5,
      load1Before: 1
    },
    metrics: { 'startup.projectListMs': value, 'measure.proc.total.pssMedianMB': value },
    errors: [],
    notes: {}
  })

  it('prints medians, p95 and a rank-sum p per client pair', () => {
    const runs = [
      run('alpha', 'launch', 1, 100),
      run('alpha', 'launch', 2, 110),
      run('beta', 'launch', 1, 300),
      run('beta', 'launch', 2, 310)
    ]
    const report = formatRunsReport(runs, [])
    expect(report).toContain('alpha against beta')
    expect(report).toContain('two-sided rank-sum p')
    expect(report).toContain('median 110 / p95 110 of 2')
    expect(report).toContain('median 310 / p95 310 of 2')
    // Fully separated 2v2 startup values → exact p = 2/C(4,2) = 0.333.
    expect(report).toContain('0.333')
  })
})
