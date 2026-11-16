import { beforeEach, describe, expect, it, vi } from 'vitest'

const navigate = vi.hoisted(() => vi.fn())

vi.mock('@/lib/log-api', () => ({ logFrontendError: vi.fn() }))

import { setRouterNavigate } from '@/lib/router-navigate'
import { mockSessionIndexEntry } from '@/lib/test-utils/acp'
import { useAcpStore } from '@/stores/acp-store'
import { useQuickChatStore } from '@/stores/quick-chat-store'
import {
  leaveQuickChatView,
  noProjectChatKind,
  openNoProjectChat,
  showNoProjectChat
} from './no-project-chat'

describe('no-project-chat', () => {
  beforeEach(() => {
    navigate.mockReset()
    setRouterNavigate(navigate)
    window.location.hash = '#/'
    useQuickChatStore.setState({ view: { mode: 'closed' } })
    useAcpStore.setState({
      sessions: {},
      sessionIndex: [
        mockSessionIndexEntry({ id: 'q', projectId: '__unassigned__', kind: 'quick' }),
        mockSessionIndexEntry({ id: 'old', projectId: '__unassigned__' }),
        mockSessionIndexEntry({ id: 'p', projectId: 'p1' })
      ]
    })
  })

  it('classifies quick, older and project chats', () => {
    const state = useAcpStore.getState()
    expect(noProjectChatKind(state, 'q')).toBe('quick')
    expect(noProjectChatKind(state, 'old')).toBe('older')
    expect(noProjectChatKind(state, 'p')).toBeNull()
    expect(noProjectChatKind(state, 'unknown')).toBeNull()
  })

  it('shows a quick chat on its chat route and an older chat read-only', () => {
    showNoProjectChat('q', 'quick', 'test')
    expect(useQuickChatStore.getState().view).toEqual({ mode: 'chat', sessionId: 'q' })
    expect(navigate).toHaveBeenCalledWith('/c/q')
    expect(openNoProjectChat('old', 'test')).toBe(true)
    expect(useQuickChatStore.getState().view).toEqual({
      mode: 'chat',
      sessionId: 'old',
      readOnly: true
    })
  })

  it('leaves a project chat alone', () => {
    expect(openNoProjectChat('p', 'test')).toBe(false)
    expect(useQuickChatStore.getState().view).toEqual({ mode: 'closed' })
    expect(navigate).not.toHaveBeenCalled()
  })

  it('closes the quick view and leaves its chat route', () => {
    useQuickChatStore.setState({ view: { mode: 'chat', sessionId: 'q' } })
    window.location.hash = '#/c/q'
    leaveQuickChatView('test')
    expect(useQuickChatStore.getState().view).toEqual({ mode: 'closed' })
    expect(navigate).toHaveBeenCalledWith('/')
  })
})
