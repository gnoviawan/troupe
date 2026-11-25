import { beforeEach, describe, expect, it, vi } from 'vitest'

let mockIsTauri = true

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn()
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn()
}))

vi.mock('@/lib/tauri-runtime', () => ({
  isTauriContext: () => mockIsTauri
}))

import { invoke } from '@tauri-apps/api/core'
import {
  browserListLocalServers,
  browserTabAnnotationCmd,
  browserTabInjectAgentation,
  browserTabScreenshot,
  browserTabSetZoom,
  browserTabStop
} from './browser-api'

describe('browser-api wrappers (desktop + non-Tauri parity)', () => {
  beforeEach(() => {
    mockIsTauri = true
    vi.clearAllMocks()
  })

  it('invokes browser_tab_inject_agentation with tabId', async () => {
    vi.mocked(invoke).mockResolvedValue({ success: true, data: undefined })
    const result = await browserTabInjectAgentation('tab-1')
    expect(invoke).toHaveBeenCalledWith('browser_tab_inject_agentation', { tabId: 'tab-1' })
    expect(result.success).toBe(true)
  })

  it('invokes browser_tab_set_zoom, browser_tab_stop, browser_tab_screenshot, and annotation commands in Tauri', async () => {
    vi.mocked(invoke).mockResolvedValue({ success: true, data: 1.25 })
    await browserTabSetZoom('tab-1', 1.25)
    expect(invoke).toHaveBeenCalledWith('browser_tab_set_zoom', {
      tabId: 'tab-1',
      zoomFactor: 1.25
    })

    await browserTabStop('tab-1')
    expect(invoke).toHaveBeenCalledWith('browser_tab_stop', { tabId: 'tab-1' })

    await browserTabScreenshot('tab-1')
    expect(invoke).toHaveBeenCalledWith('browser_tab_screenshot', { tabId: 'tab-1' })

    await browserTabAnnotationCmd('tab-1', { type: 'set_tool', tool: 'ruler' })
    expect(invoke).toHaveBeenCalledWith('browser_tab_annotation_cmd', {
      tabId: 'tab-1',
      command: { type: 'set_tool', tool: 'ruler' }
    })

    await browserListLocalServers()
    expect(invoke).toHaveBeenCalledWith('browser_list_local_servers')
  })

  it('gates desktop-only commands cleanly with NOT_TAURI outside Tauri runtime', async () => {
    mockIsTauri = false
    const zoom = await browserTabSetZoom('tab-1', 1.25)
    const stop = await browserTabStop('tab-1')
    const shot = await browserTabScreenshot('tab-1')
    const cmd = await browserTabAnnotationCmd('tab-1', { type: 'clear' })
    const servers = await browserListLocalServers()

    for (const res of [zoom, stop, shot, cmd, servers]) {
      expect(res.success).toBe(false)
      if (!res.success) expect(res.code).toBe('NOT_TAURI')
    }
    expect(invoke).not.toHaveBeenCalled()
  })
})
