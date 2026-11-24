import { renderHook } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('@/lib/log-api', () => ({ logFrontendError: vi.fn() }))

import { adoptQuickChat } from '@/lib/no-project-chat'
import { setRouterNavigate } from '@/lib/router-navigate'
import { mockSessionIndexEntry } from '@/lib/test-utils/acp'
import { useAcpStore } from '@/stores/acp-store'
import { useQuickChatStore } from '@/stores/quick-chat-store'
import { useQuickChatRouteSync } from './use-quick-chat-route-sync'

function renderSync(pathname: string) {
  return renderHook(({ path }) => useQuickChatRouteSync(path), {
    initialProps: { path: pathname }
  })
}

describe('useQuickChatRouteSync', () => {
  beforeEach(() => {
    setRouterNavigate(vi.fn())
    window.location.hash = '#/'
    useAcpStore.setState({
      sessions: {},
      sessionIndex: [
        mockSessionIndexEntry({ id: 'q1', projectId: '__unassigned__', kind: 'quick' }),
        mockSessionIndexEntry({ id: 'q2', projectId: '__unassigned__', kind: 'quick' }),
        mockSessionIndexEntry({ id: 'p1', projectId: 'proj' })
      ]
    })
    useQuickChatStore.setState({ view: { mode: 'chat', sessionId: 'q1' } })
  })

  it('keeps the quick view when the route names the open quick chat', () => {
    renderSync('/c/q1')
    expect(useQuickChatStore.getState().view).toEqual({ mode: 'chat', sessionId: 'q1' })
  })

  it('closes the quick view on Back to a project chat', () => {
    const { rerender } = renderSync('/c/q1')
    rerender({ path: '/c/p1' })
    expect(useQuickChatStore.getState().view).toEqual({ mode: 'closed' })
  })

  it('closes the quick view on Back to the workspace root or a routed page', () => {
    const { rerender } = renderSync('/c/q1')
    rerender({ path: '/' })
    expect(useQuickChatStore.getState().view).toEqual({ mode: 'closed' })
    useQuickChatStore.setState({ view: { mode: 'chat', sessionId: 'q1' } })
    rerender({ path: '/snapshots' })
    expect(useQuickChatStore.getState().view).toEqual({ mode: 'closed' })
  })

  it('shows the other quick chat on Back between two quick chats', () => {
    const { rerender } = renderSync('/c/q1')
    rerender({ path: '/c/q2' })
    expect(useQuickChatStore.getState().view).toEqual({ mode: 'chat', sessionId: 'q2' })
  })

  it('does nothing when the quick view is closed', () => {
    useQuickChatStore.setState({ view: { mode: 'closed' } })
    const { rerender } = renderSync('/c/p1')
    rerender({ path: '/c/q1' })
    expect(useQuickChatStore.getState().view).toEqual({ mode: 'closed' })
  })

  it('keeps the quick chat launcher on the workspace root, which is its route', () => {
    useQuickChatStore.setState({ view: { mode: 'launcher' } })
    const { rerender } = renderSync('/')
    expect(useQuickChatStore.getState().view).toEqual({ mode: 'launcher' })
    // Forward to a quick chat shows it.
    rerender({ path: '/c/q2' })
    expect(useQuickChatStore.getState().view).toEqual({ mode: 'chat', sessionId: 'q2' })
  })

  it('closes the quick chat launcher for a routed page or a project chat', () => {
    useQuickChatStore.setState({ view: { mode: 'launcher' } })
    const { rerender } = renderSync('/')
    rerender({ path: '/snapshots' })
    expect(useQuickChatStore.getState().view).toEqual({ mode: 'closed' })
    useQuickChatStore.setState({ view: { mode: 'launcher' } })
    rerender({ path: '/c/p1' })
    expect(useQuickChatStore.getState().view).toEqual({ mode: 'closed' })
  })

  it('takes Back to a quick launch that started behind a new launcher to its chat', () => {
    const navigate = vi.fn()
    setRouterNavigate(navigate)
    // Send, then + before the agent started: the launcher replaced the
    // placeholder's view, and #/c/launch-q1 stays in history.
    useQuickChatStore.setState({ view: { mode: 'launcher' } })
    adoptQuickChat('launch-q1', 'q2', 'test')
    expect(useQuickChatStore.getState().view).toEqual({ mode: 'launcher' })

    const { rerender } = renderSync('/')
    rerender({ path: '/c/launch-q1' })
    expect(navigate).toHaveBeenCalledWith('/c/q2', { replace: true })
    rerender({ path: '/c/q2' })
    expect(useQuickChatStore.getState().view).toEqual({ mode: 'chat', sessionId: 'q2' })
  })
})
