import { describe, expect, it } from 'vitest'
import { errorResetSeconds, isAdapterPlanLimit, rejectedRateLimit } from './rate-limit'

const ANTIGRAVITY = 'acp-registry:antigravity-acp'
const CODEX = 'acp-registry:codex-acp'
const antigravityLimit = {
  errorKind: 'quota_exhausted',
  agent: 'antigravity',
  resetsAt: 1_800_000_000_000
}

describe('adapter plan limits', () => {
  it('routes an Antigravity limit only to an Antigravity chat', () => {
    expect(isAdapterPlanLimit(antigravityLimit, ANTIGRAVITY)).toBe(true)
    expect(isAdapterPlanLimit(antigravityLimit, CODEX)).toBe(false)
    expect(isAdapterPlanLimit(antigravityLimit, null)).toBe(false)
  })

  it('routes a Codex limit to the Codex chat, never to an Antigravity chat', () => {
    expect(isAdapterPlanLimit({ errorKind: 'quota_exhausted' }, CODEX)).toBe(true)
    expect(isAdapterPlanLimit({ codexErrorInfo: 'usageLimitExceeded' }, CODEX)).toBe(true)
    expect(isAdapterPlanLimit({ errorKind: 'quota_exhausted' }, ANTIGRAVITY)).toBe(false)
    expect(isAdapterPlanLimit({ codexErrorInfo: 'usageLimitExceeded' }, ANTIGRAVITY)).toBe(false)
  })

  it('takes the reset time from an Antigravity error, in seconds', () => {
    expect(errorResetSeconds(antigravityLimit)).toBe(1_800_000_000)
    expect(errorResetSeconds({ ...antigravityLimit, resetsAt: null })).toBeNull()
    expect(errorResetSeconds({ errorKind: 'quota_exhausted', resetsAt: 5 })).toBeNull()
    expect(rejectedRateLimit('Antigravity reached its usage limit', antigravityLimit)).toEqual({
      status: 'rejected',
      resetsAt: 1_800_000_000,
      kind: null,
      percent: null,
      overageInUse: false
    })
    expect(rejectedRateLimit('You hit your 5-hour limit').resetsAt).toBeNull()
  })
})
