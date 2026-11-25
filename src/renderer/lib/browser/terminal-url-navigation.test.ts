import { beforeEach, describe, expect, it, vi } from 'vitest'
import { useAppSettingsStore } from '@/stores/app-settings-store'
import { useBrowserSessionStore } from '@/stores/browser-session-store'
import { useWorkspaceStore } from '@/stores/workspace-store'
import { DEFAULT_APP_SETTINGS, type LinkOpenMode } from '@/types/settings'

const { mockOpenUrlWithSystemBrowser, tauriRef } = vi.hoisted(() => ({
  mockOpenUrlWithSystemBrowser: vi.fn(),
  tauriRef: { current: true }
}))

vi.mock('@/lib/api', () => ({
  openerApi: {
    openUrlWithSystemBrowser: mockOpenUrlWithSystemBrowser
  }
}))
vi.mock('@/lib/tauri-runtime', () => ({ isTauriContext: () => tauriRef.current }))
vi.mock('@/lib/log-api', () => ({ logFrontendError: vi.fn() }))

import { openLink } from './open-link'
import { openTerminalUrl } from './terminal-url-navigation'

function setMode(mode: LinkOpenMode): void {
  useAppSettingsStore.setState((state) => ({
    ...state,
    settings: { ...state.settings, linkOpenMode: mode }
  }))
}

function rootTabs() {
  const root = useWorkspaceStore.getState().root
  if (root.type !== 'leaf') throw new Error('Expected root pane to be a leaf')
  return root.tabs
}

describe('openLink and openTerminalUrl', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    tauriRef.current = true
    useAppSettingsStore.setState({
      settings: { ...DEFAULT_APP_SETTINGS },
      isLoaded: true
    })
    useBrowserSessionStore.setState({ tabs: new Map() })
    useWorkspaceStore.setState(() => ({
      root: { type: 'leaf', id: 'pane-root', tabs: [], activeTabId: null },
      activePaneId: 'pane-root',
      fullscreenPaneId: null
    }))
    mockOpenUrlWithSystemBrowser.mockResolvedValue({ success: true, data: undefined })
  })

  it('opens links in the default browser when the setting is system', async () => {
    await openTerminalUrl('https://example.com')

    expect(mockOpenUrlWithSystemBrowser).toHaveBeenCalledWith('https://example.com')
    expect(useBrowserSessionStore.getState().tabs.size).toBe(0)
    expect(rootTabs()).toHaveLength(0)
  })

  it('opens links in a new Troupe browser tab when the setting is troupe', async () => {
    setMode('troupe')

    await openTerminalUrl('https://example.com')

    expect(mockOpenUrlWithSystemBrowser).not.toHaveBeenCalled()
    const tabs = Array.from(useBrowserSessionStore.getState().tabs.values())
    expect(tabs).toHaveLength(1)
    expect(tabs[0]?.url).toBe('https://example.com')
    expect(rootTabs()[0]).toMatchObject({ type: 'browser', browserTabId: tabs[0]?.id })
  })

  it('sends a mailto link to the default browser even when the setting is troupe', async () => {
    setMode('troupe')

    await openLink('mailto:hi@example.com')

    expect(mockOpenUrlWithSystemBrowser).toHaveBeenCalledWith('mailto:hi@example.com')
    expect(useBrowserSessionStore.getState().tabs.size).toBe(0)
  })

  it('uses the default browser in the web client, which has no Troupe browser', async () => {
    tauriRef.current = false
    setMode('troupe')

    await openLink('https://example.com')

    expect(mockOpenUrlWithSystemBrowser).toHaveBeenCalledWith('https://example.com')
  })

  it('throws when the default browser refuses the link', async () => {
    mockOpenUrlWithSystemBrowser.mockResolvedValueOnce({
      success: false,
      error: 'open failed',
      code: 'OPEN_URL_ERROR'
    })

    await expect(openTerminalUrl('https://example.com')).rejects.toThrow('open failed')
  })

  it('treats an unknown stored value as the default browser', async () => {
    setMode('invalid-mode' as LinkOpenMode)

    await openLink('https://example.com')

    expect(mockOpenUrlWithSystemBrowser).toHaveBeenCalledWith('https://example.com')
  })
})
