import { renderHook } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { useAdoptedLaunchSession } from './use-adopted-launch-session'

function renderAdopted(sessionId: string) {
  return renderHook(({ id }) => useAdoptedLaunchSession(id), { initialProps: { id: sessionId } })
}

describe('useAdoptedLaunchSession (#1941)', () => {
  it('is false for the chat the panel mounted on', () => {
    expect(renderAdopted('launch-1').result.current).toBe(false)
    expect(renderAdopted('sess-1').result.current).toBe(false)
  })

  it('is true from the first render after a launch placeholder became the session', () => {
    const { result, rerender } = renderAdopted('launch-1')
    rerender({ id: 'sess-1' })
    expect(result.current).toBe(true)
    // It stays true for that session on later renders.
    rerender({ id: 'sess-1' })
    expect(result.current).toBe(true)
  })

  it('is false when the panel moves between real sessions, or on to another chat', () => {
    const { result, rerender } = renderAdopted('sess-a')
    rerender({ id: 'sess-b' })
    expect(result.current).toBe(false)

    rerender({ id: 'launch-2' })
    rerender({ id: 'sess-c' })
    expect(result.current).toBe(true)
    rerender({ id: 'sess-d' })
    expect(result.current).toBe(false)
  })
})
