import { beforeEach, describe, expect, it, vi } from 'vitest'
import { useBrowserSessionStore } from '@/stores/browser-session-store'
import { useWorkspaceStore } from '@/stores/workspace-store'

const { mockIsTauriContext, mockLog, mockLeaveQuickChat } = vi.hoisted(() => ({
  mockIsTauriContext: vi.fn(() => true),
  mockLog: vi.fn(() => Promise.resolve()),
  mockLeaveQuickChat: vi.fn()
}))

vi.mock('@/lib/tauri-runtime', () => ({ isTauriContext: mockIsTauriContext }))
vi.mock('@/lib/log-api', () => ({ logFrontendError: mockLog }))
vi.mock('@/lib/no-project-chat', () => ({ leaveQuickChatView: mockLeaveQuickChat }))

import {
  browsableUrlForContextMenu,
  browsableUrlFrom,
  canOpenInTroupeBrowser,
  openUrlInTroupeBrowser
} from './open-in-troupe-browser'

describe('browsableUrlFrom', () => {
  it.each([
    ['http://localhost:4321', 'http://localhost:4321/'],
    ['https://example.com/a?b=1', 'https://example.com/a?b=1'],
    ['localhost:4321', 'http://localhost:4321/'],
    ['localhost:3000/docs', 'http://localhost:3000/docs'],
    ['127.0.0.1:8080', 'http://127.0.0.1:8080/'],
    ['0.0.0.0:5173', 'http://0.0.0.0:5173/'],
    ['[::1]:8000', 'http://[::1]:8000/'],
    ['[::1]:8000.', 'http://[::1]:8000/'],
    ['([::1]:8000).', 'http://[::1]:8000/'],
    ['app.localhost:3000', 'http://app.localhost:3000/'],
    ['  (http://localhost:4321).  ', 'http://localhost:4321/'],
    ['`localhost:4321`', 'http://localhost:4321/'],
    ['https://example.com/a).', 'https://example.com/a']
  ])('reads %s as %s', (text, url) => {
    expect(browsableUrlFrom(text)).toBe(url)
  })

  it.each([
    [''],
    [null],
    ['package.json'],
    ['example.com'],
    ['hello world'],
    ['see http://localhost:4321'],
    ['file:///etc/passwd'],
    ['javascript:alert(1)'],
    ['mailto:a@b.c'],
    ['ftp://example.com']
  ])('rejects %s', (text) => {
    expect(browsableUrlFrom(text)).toBeNull()
  })
})

describe('browsableUrlForContextMenu', () => {
  it('prefers the link under the pointer over the selection', () => {
    const anchor = document.createElement('a')
    anchor.setAttribute('href', 'http://localhost:4321')
    const inner = document.createElement('span')
    anchor.append(inner)
    expect(browsableUrlForContextMenu(inner, 'localhost:9999')).toBe('http://localhost:4321/')
  })

  it('falls back to the selection when the link is not http(s)', () => {
    const anchor = document.createElement('a')
    anchor.setAttribute('href', '#section')
    expect(browsableUrlForContextMenu(anchor, 'localhost:9999')).toBe('http://localhost:9999/')
  })

  it('keeps a trailing parenthesis that is part of the href', () => {
    const anchor = document.createElement('a')
    anchor.setAttribute('href', 'https://en.wikipedia.org/wiki/Function_(mathematics)')
    expect(browsableUrlForContextMenu(anchor, '')).toBe(
      'https://en.wikipedia.org/wiki/Function_(mathematics)'
    )
  })

  it('returns null with no link and no URL selected', () => {
    expect(browsableUrlForContextMenu(document.createElement('div'), 'some text')).toBeNull()
    expect(browsableUrlForContextMenu(null, '')).toBeNull()
  })
})

describe('openUrlInTroupeBrowser', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mockIsTauriContext.mockReturnValue(true)
    useBrowserSessionStore.setState({ tabs: new Map() })
    useWorkspaceStore.setState(() => ({
      root: { type: 'leaf', id: 'pane-root', tabs: [], activeTabId: null },
      activePaneId: 'pane-root',
      fullscreenPaneId: null
    }))
  })

  it('opens the URL in a new browser tab in the active pane', () => {
    expect(openUrlInTroupeBrowser('http://localhost:4321/')).toBe(true)

    const tabs = Array.from(useBrowserSessionStore.getState().tabs.values())
    expect(tabs).toHaveLength(1)
    expect(tabs[0]?.url).toBe('http://localhost:4321/')
    const root = useWorkspaceStore.getState().root
    if (root.type !== 'leaf') throw new Error('Expected root pane to be a leaf')
    expect(root.tabs).toHaveLength(1)
    expect(root.tabs[0]).toMatchObject({ type: 'browser', browserTabId: tabs[0]?.id })
    expect(mockLeaveQuickChat).toHaveBeenCalledOnce()
    // The log names the origin only, never the path or query.
    expect(mockLog).toHaveBeenCalledWith(
      expect.objectContaining({
        level: 'info',
        message: expect.stringContaining('http://localhost:4321')
      })
    )
  })

  it('logs only the origin of the URL', () => {
    openUrlInTroupeBrowser('http://localhost:4321/reset?token=secret')
    const messages = mockLog.mock.calls.map(
      (call) => (call as unknown as [{ message: string }])[0].message
    )
    expect(messages.join('\n')).not.toContain('secret')
  })

  it('opens nothing outside the desktop app', () => {
    mockIsTauriContext.mockReturnValue(false)
    expect(canOpenInTroupeBrowser()).toBe(false)
    expect(openUrlInTroupeBrowser('http://localhost:4321/')).toBe(false)
    expect(useBrowserSessionStore.getState().tabs.size).toBe(0)
    expect(mockLog).toHaveBeenCalledWith(expect.objectContaining({ level: 'warn' }))
  })

  it('opens nothing without an active pane', () => {
    useWorkspaceStore.setState({ activePaneId: null })
    expect(openUrlInTroupeBrowser('http://localhost:4321/')).toBe(false)
    expect(useBrowserSessionStore.getState().tabs.size).toBe(0)
    expect(mockLeaveQuickChat).not.toHaveBeenCalled()
  })
})
