import { cleanup, renderHook } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { useChatActivityStore } from '@/stores/chat-activity-store'
import { type QuickChatView, useQuickChatStore } from '@/stores/quick-chat-store'
import { useWorkspaceStore } from '@/stores/workspace-store'
import type { LeafNode, PaneNode, SplitNode } from '@/types/workspace.types'
import { activeChatSessionIds, useShellUnreadTracker } from './use-shell-unread-tracker'

vi.mock('@/lib/chat-activity-sync', () => ({ startChatActivitySync: () => () => {} }))

function chatLeaf(id: string, sessionId: string | null, extra: LeafNode['tabs'] = []): LeafNode {
  const tabs: LeafNode['tabs'] = [...extra]
  if (sessionId) tabs.push({ type: 'agent-chat', id: `chat-${sessionId}`, sessionId })
  return {
    type: 'leaf',
    id,
    tabs,
    activeTabId: sessionId ? `chat-${sessionId}` : (tabs[0]?.id ?? null)
  }
}

function split(id: string, children: PaneNode[]): SplitNode {
  return {
    type: 'split',
    id,
    direction: 'horizontal',
    children,
    sizes: children.map(() => 100 / children.length)
  }
}

describe('activeChatSessionIds', () => {
  it('returns every leaf active chat on desktop', () => {
    const root = split('s', [
      chatLeaf('a', 'sa'),
      split('inner', [chatLeaf('b', 'sb'), chatLeaf('c', 'sc')])
    ])

    expect(
      activeChatSessionIds({ root, activePaneId: 'a', fullscreenPaneId: null }, false)
    ).toEqual(['sa', 'sb', 'sc'])
  })

  it('ignores a leaf whose active tab is not an agent chat', () => {
    const git: LeafNode['tabs'] = [{ type: 'git', id: 'git-1', cwd: '/x' }]
    const root = split('s', [chatLeaf('a', 'sa'), chatLeaf('b', null, git)])

    expect(
      activeChatSessionIds({ root, activePaneId: 'a', fullscreenPaneId: null }, false)
    ).toEqual(['sa'])
  })

  it('ignores chat tabs that are not the active tab of their leaf', () => {
    const background: LeafNode = {
      type: 'leaf',
      id: 'a',
      tabs: [
        { type: 'agent-chat', id: 'chat-bg', sessionId: 'bg' },
        { type: 'git', id: 'git-1', cwd: '/x' }
      ],
      activeTabId: 'git-1'
    }

    expect(
      activeChatSessionIds({ root: background, activePaneId: 'a', fullscreenPaneId: null }, false)
    ).toEqual([])
  })

  it('returns only the fullscreen pane chat on desktop', () => {
    const root = split('s', [chatLeaf('a', 'sa'), chatLeaf('b', 'sb')])

    expect(activeChatSessionIds({ root, activePaneId: 'a', fullscreenPaneId: 'b' }, false)).toEqual(
      ['sb']
    )
  })

  it('returns only the mobile active leaf chat on the mobile web shell', () => {
    const root = split('s', [chatLeaf('a', 'sa'), chatLeaf('b', 'sb')])

    expect(activeChatSessionIds({ root, activePaneId: 'b', fullscreenPaneId: null }, true)).toEqual(
      ['sb']
    )
  })

  it('falls back to the first leaf on mobile when activePaneId matches no leaf', () => {
    const root = split('s', [chatLeaf('a', 'sa'), chatLeaf('b', 'sb')])

    expect(
      activeChatSessionIds({ root, activePaneId: 'missing', fullscreenPaneId: null }, true)
    ).toEqual(['sa'])
  })

  it('returns an empty list for a single non-chat leaf', () => {
    const git: LeafNode['tabs'] = [{ type: 'git', id: 'git-1', cwd: '/x' }]

    expect(
      activeChatSessionIds(
        { root: chatLeaf('a', null, git), activePaneId: 'a', fullscreenPaneId: null },
        true
      )
    ).toEqual([])
  })
})

describe('useShellUnreadTracker viewing', () => {
  const initialWorkspace = useWorkspaceStore.getState()

  afterEach(() => {
    cleanup()
    useWorkspaceStore.setState(initialWorkspace, true)
    useQuickChatStore.setState({ view: { mode: 'closed' } })
    useChatActivityStore.getState().setViewing([])
  })

  function viewingWith(view: QuickChatView): readonly string[] {
    useWorkspaceStore.setState({
      root: split('s', [chatLeaf('a', 'sa'), chatLeaf('b', 'sb')]),
      activePaneId: 'a',
      fullscreenPaneId: null
    })
    useQuickChatStore.setState({ view })
    renderHook(() => useShellUnreadTracker())
    return useChatActivityStore.getState().viewing
  }

  it('views the panes’ chats when no quick view is open', () => {
    expect(viewingWith({ mode: 'closed' })).toEqual(['sa', 'sb'])
  })

  it('views only the open quick chat, which covers the panes', () => {
    expect(viewingWith({ mode: 'chat', sessionId: 'quick-1' })).toEqual(['quick-1'])
  })

  it('views no chat while the quick chat launcher covers the panes', () => {
    expect(viewingWith({ mode: 'launcher', id: 1 })).toEqual([])
  })
})
