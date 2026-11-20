import { act, renderHook, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { seedComposerDraft, useComposerDraft } from './use-composer-draft'

const persistence = vi.hoisted(() => ({
  read: vi.fn(),
  write: vi.fn(),
  writeDebounced: vi.fn(),
  delete: vi.fn()
}))
vi.mock('@/lib/api', () => ({ persistenceApi: persistence }))

beforeEach(() => {
  vi.clearAllMocks()
  persistence.read.mockResolvedValue({ success: true, data: null })
  persistence.write.mockResolvedValue({ success: true })
  persistence.writeDebounced.mockResolvedValue({ success: true })
  persistence.delete.mockResolvedValue({ success: true })
})

describe('seedComposerDraft', () => {
  it('opens the chat composer on the seeded draft without sending it, and persists it', async () => {
    seedComposerDraft('p1', 'chat-1', 'Work on issue #12')
    expect(persistence.write).toHaveBeenCalledWith('chat-draft/p1/chat-1', 'Work on issue #12')

    const { result } = renderHook(() =>
      useComposerDraft({ projectId: 'p1', sessionId: 'chat-1', seedNonce: undefined })
    )
    await waitFor(() => expect(result.current[0]).toBe('Work on issue #12'))
    expect(persistence.read).not.toHaveBeenCalled()
  })

  it('never deletes the persisted seed while the composer picks it up', async () => {
    seedComposerDraft('p1', 'chat-3', 'Fix issue #3')
    const { result } = renderHook(() =>
      useComposerDraft({ projectId: 'p1', sessionId: 'chat-3', seedNonce: undefined })
    )
    await waitFor(() => expect(result.current[0]).toBe('Fix issue #3'))
    await waitFor(() =>
      expect(persistence.writeDebounced).toHaveBeenCalledWith(
        'chat-draft/p1/chat-3',
        'Fix issue #3'
      )
    )
    expect(persistence.delete).not.toHaveBeenCalled()

    act(() => result.current[1](''))
    expect(persistence.delete).toHaveBeenCalledWith('chat-draft/p1/chat-3')
  })

  it('applies a draft seeded for the real session after the placeholder re-keys', async () => {
    const { result, rerender } = renderHook(
      ({ sessionId }) => useComposerDraft({ projectId: 'p1', sessionId, seedNonce: undefined }),
      { initialProps: { sessionId: 'ph-1' } }
    )
    await waitFor(() => expect(persistence.read).toHaveBeenCalledTimes(1))

    act(() => seedComposerDraft('p1', 'chat-2', 'Fix the crash'))
    rerender({ sessionId: 'chat-2' })

    await waitFor(() => expect(result.current[0]).toBe('Fix the crash'))
  })
})

describe('launch placeholder → session (#1941)', () => {
  it('keeps the typed draft and stores it under the session once the placeholder becomes it', async () => {
    const { result, rerender } = renderHook(
      ({ sessionId }) => useComposerDraft({ projectId: 'p1', sessionId, seedNonce: undefined }),
      { initialProps: { sessionId: 'launch-1' } }
    )
    await waitFor(() => expect(result.current[2]).toBe(true))
    act(() => result.current[1]('draft at 300ms'))
    persistence.read.mockClear()

    rerender({ sessionId: 'sess-1' })

    expect(result.current[0]).toBe('draft at 300ms')
    await waitFor(() => expect(result.current[2]).toBe(true))
    // No read of the new key: it holds nothing and the text is already here.
    expect(persistence.read).not.toHaveBeenCalled()
    expect(persistence.write).toHaveBeenCalledWith('chat-draft/p1/sess-1', 'draft at 300ms')
    await waitFor(() => expect(persistence.delete).toHaveBeenCalledWith('chat-draft/p1/launch-1'))
  })

  it("keeps the placeholder's stored draft when the session's write fails", async () => {
    const { result, rerender } = renderHook(
      ({ sessionId }) => useComposerDraft({ projectId: 'p1', sessionId, seedNonce: undefined }),
      { initialProps: { sessionId: 'launch-2' } }
    )
    await waitFor(() => expect(result.current[2]).toBe(true))
    act(() => result.current[1]('draft to keep'))
    persistence.write.mockResolvedValue({ success: false, error: 'disk full' })

    await act(async () => rerender({ sessionId: 'sess-2' }))

    expect(result.current[0]).toBe('draft to keep')
    expect(persistence.write).toHaveBeenCalledWith('chat-draft/p1/sess-2', 'draft to keep')
    expect(persistence.delete).not.toHaveBeenCalledWith('chat-draft/p1/launch-2')
  })

  it('reads the stored draft as usual when one chat replaces another', async () => {
    const { result, rerender } = renderHook(
      ({ sessionId }) => useComposerDraft({ projectId: 'p1', sessionId, seedNonce: undefined }),
      { initialProps: { sessionId: 'sess-a' } }
    )
    await waitFor(() => expect(result.current[2]).toBe(true))
    act(() => result.current[1]('text of a'))
    persistence.read.mockResolvedValue({ success: true, data: 'text of b' })

    rerender({ sessionId: 'sess-b' })

    await waitFor(() => expect(result.current[0]).toBe('text of b'))
    expect(persistence.read).toHaveBeenCalledWith('chat-draft/p1/sess-b')
    expect(persistence.write).not.toHaveBeenCalledWith('chat-draft/p1/sess-b', 'text of a')
  })
})

describe('draft hydration state', () => {
  it('reports not hydrated until the stored draft has loaded', async () => {
    let resolveRead: (value: { success: boolean; data: string }) => void = () => {}
    persistence.read.mockReturnValue(
      new Promise((resolve) => {
        resolveRead = resolve
      })
    )
    const { result } = renderHook(() =>
      useComposerDraft({ projectId: 'p1', sessionId: 'chat-4', seedNonce: undefined })
    )
    expect(result.current[2]).toBe(false)
    await act(async () => resolveRead({ success: true, data: 'older draft' }))
    expect(result.current[0]).toBe('older draft')
    expect(result.current[2]).toBe(true)
  })
})
