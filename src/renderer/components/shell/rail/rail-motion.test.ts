import { act, renderHook } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { toggleRailVisibility } from '../use-rail-toggle'
import {
  allowNextRailMotion,
  RAIL_NESTED_ICON_TRAVEL_CLASS,
  RAIL_NESTED_ROW_TRAVEL_CLASS,
  RAIL_SETTLE_MS,
  RAIL_TRAVEL_CLASS,
  RAIL_WIDTH_MS,
  RAIL_WIDTH_TRANSITION_CLASS,
  skipNextRailMotion,
  useRailMotion
} from './rail-motion'

vi.mock('@/lib/log-api', () => ({ logFrontendError: vi.fn() }))

describe('useRailMotion', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    // The shortcut skip window is module state; the fake clock restarts at 0 each test.
    allowNextRailMotion()
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  const setup = (expanded: boolean, reducedMotion = false) =>
    renderHook(({ expanded, reducedMotion }) => useRailMotion(expanded, reducedMotion), {
      initialProps: { expanded, reducedMotion }
    })

  it('starts idle in the persisted density', () => {
    expect(setup(true).result.current).toEqual({ phase: 'idle', density: 'expanded' })
    expect(setup(false).result.current).toEqual({ phase: 'idle', density: 'collapsed' })
  })

  it('keeps the expanded rows while collapsing, then settles collapsed', () => {
    const { result, rerender } = setup(true)
    rerender({ expanded: false, reducedMotion: false })
    expect(result.current).toEqual({ phase: 'collapsing', density: 'expanded' })

    act(() => vi.advanceTimersByTime(RAIL_WIDTH_MS))
    expect(result.current).toEqual({ phase: 'settling', density: 'collapsed' })

    act(() => vi.advanceTimersByTime(RAIL_SETTLE_MS))
    expect(result.current).toEqual({ phase: 'idle', density: 'collapsed' })
  })

  it('switches to the expanded rows at once and ends idle', () => {
    const { result, rerender } = setup(false)
    rerender({ expanded: true, reducedMotion: false })
    expect(result.current).toEqual({ phase: 'expanding', density: 'expanded' })

    act(() => vi.advanceTimersByTime(RAIL_WIDTH_MS))
    expect(result.current).toEqual({ phase: 'idle', density: 'expanded' })
  })

  it('reverses a collapse that is in progress', () => {
    const { result, rerender } = setup(true)
    rerender({ expanded: false, reducedMotion: false })
    act(() => vi.advanceTimersByTime(RAIL_WIDTH_MS / 2))
    rerender({ expanded: true, reducedMotion: false })
    expect(result.current).toEqual({ phase: 'expanding', density: 'expanded' })

    // The collapse timer does not fire into the new phase.
    act(() => vi.advanceTimersByTime(RAIL_WIDTH_MS / 2))
    expect(result.current.phase).toBe('expanding')
    act(() => vi.advanceTimersByTime(RAIL_WIDTH_MS))
    expect(result.current).toEqual({ phase: 'idle', density: 'expanded' })
  })

  it('changes in one frame under reduced motion', () => {
    const { result, rerender } = setup(true, true)
    rerender({ expanded: false, reducedMotion: true })
    expect(result.current).toEqual({ phase: 'idle', density: 'collapsed' })
    rerender({ expanded: true, reducedMotion: true })
    expect(result.current).toEqual({ phase: 'idle', density: 'expanded' })
  })

  it('changes in one frame after a keyboard shortcut, and animates the next pointer toggle', () => {
    const { result, rerender } = setup(true)
    skipNextRailMotion()
    rerender({ expanded: false, reducedMotion: false })
    expect(result.current).toEqual({ phase: 'idle', density: 'collapsed' })

    // The skip window closes; a later toggle animates again.
    act(() => vi.advanceTimersByTime(1000))
    rerender({ expanded: true, reducedMotion: false })
    expect(result.current).toEqual({ phase: 'expanding', density: 'expanded' })
  })

  it('animates a pointer toggle inside the shortcut skip window', () => {
    const { result, rerender } = setup(true)
    skipNextRailMotion()
    allowNextRailMotion()
    rerender({ expanded: false, reducedMotion: false })
    expect(result.current).toEqual({ phase: 'collapsing', density: 'expanded' })
  })

  it('a button toggle clears a shortcut skip; a shortcut keeps it', () => {
    const update = async (): Promise<void> => {}
    const button = setup(true)
    skipNextRailMotion()
    toggleRailVisibility(true, update, 'button')
    button.rerender({ expanded: false, reducedMotion: false })
    expect(button.result.current.phase).toBe('collapsing')

    const shortcut = setup(true)
    toggleRailVisibility(true, update, 'shortcut')
    shortcut.rerender({ expanded: false, reducedMotion: false })
    expect(shortcut.result.current.phase).toBe('idle')
  })
})

describe('Rail motion classes', () => {
  it('uses the one strong ease-out for the width and every travel', () => {
    const curve = 'cubic-bezier(0.23,1,0.32,1)'
    expect(RAIL_WIDTH_TRANSITION_CLASS).toContain(`ease-[${curve}]`)
    expect(RAIL_TRAVEL_CLASS).toContain(`rail-travel-out_200ms_${curve}_both`)
    expect(RAIL_TRAVEL_CLASS).toContain(`rail-travel-in_200ms_${curve}`)
    expect(RAIL_NESTED_ROW_TRAVEL_CLASS).toContain(`rail-row-travel-out_200ms_${curve}_both`)
    expect(RAIL_NESTED_ROW_TRAVEL_CLASS).toContain(`rail-row-travel-in_200ms_${curve}`)
    for (const cls of [RAIL_TRAVEL_CLASS, RAIL_NESTED_ROW_TRAVEL_CLASS]) {
      expect(cls).toContain('motion-reduce:animate-none')
    }
  })

  it('moves a project chat row 17px and its icon 4px more, onto x = 28', () => {
    expect(RAIL_NESTED_ROW_TRAVEL_CLASS).toContain('[--rail-travel-x:-17px]')
    expect(RAIL_NESTED_ICON_TRAVEL_CLASS).toContain('[--rail-travel-x:-4px]')
  })
})
