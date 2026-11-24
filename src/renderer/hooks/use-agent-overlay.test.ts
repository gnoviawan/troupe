import { act, renderHook } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const { mockLoad } = vi.hoisted(() => ({ mockLoad: vi.fn() }))
vi.mock('@/lib/agents/agent-overlay', () => ({ loadAgentOverlay: mockLoad }))

import { useAgentOverlayLoader } from './use-agent-overlay'

const MINUTE = 60_000
const HOUR = 60 * MINUTE

describe('useAgentOverlayLoader', () => {
  beforeEach(() => {
    mockLoad.mockReset()
    vi.useFakeTimers()
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  it('loads on mount and again every hour', async () => {
    mockLoad.mockResolvedValue(true)
    const { unmount } = renderHook(() => useAgentOverlayLoader())
    await act(() => vi.advanceTimersByTimeAsync(59 * MINUTE))
    expect(mockLoad).toHaveBeenCalledTimes(1)
    await act(() => vi.advanceTimersByTimeAsync(MINUTE))
    expect(mockLoad).toHaveBeenCalledTimes(2)
    unmount()
    await act(() => vi.advanceTimersByTimeAsync(2 * HOUR))
    expect(mockLoad).toHaveBeenCalledTimes(2)
  })

  it('retries a minute after the first failure, then falls back to hourly', async () => {
    mockLoad.mockResolvedValue(false)
    const { unmount } = renderHook(() => useAgentOverlayLoader())
    await act(() => vi.advanceTimersByTimeAsync(MINUTE))
    expect(mockLoad).toHaveBeenCalledTimes(2)
    await act(() => vi.advanceTimersByTimeAsync(59 * MINUTE))
    expect(mockLoad).toHaveBeenCalledTimes(2)
    await act(() => vi.advanceTimersByTimeAsync(MINUTE))
    expect(mockLoad).toHaveBeenCalledTimes(3)
    unmount()
  })
})
