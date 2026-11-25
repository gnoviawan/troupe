import { beforeEach, describe, expect, it, vi } from 'vitest'

let mockIsTauri = true

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn()
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {})
}))

vi.mock('@/lib/tauri-runtime', () => ({
  isTauriContext: () => mockIsTauri
}))

import { invoke } from '@tauri-apps/api/core'
import {
  browserAnnotationsSync,
  browserCheckLocalPort,
  browserListLocalServers,
  browserSaveAnnotationCrop,
  browserTabAnnotationCmd,
  browserTabCreate,
  browserTabDestroy,
  browserTabGoBack,
  browserTabGoForward,
  browserTabHide,
  browserTabInjectAgentation,
  browserTabNavigate,
  browserTabOpenDevtools,
  browserTabReload,
  browserTabResize,
  browserTabScreenshot,
  browserTabSetZoom,
  browserTabShow,
  browserTabStop
} from './browser-api'

describe('browser-api wrappers (desktop + non-Tauri parity)', () => {
  beforeEach(() => {
    mockIsTauri = true
    vi.clearAllMocks()
  })

  it('invokes browser commands through canonical invokeIpc in Tauri', async () => {
    vi.mocked(invoke).mockResolvedValue({ success: true, data: undefined })

    await browserTabInjectAgentation('tab-1')
    expect(invoke).toHaveBeenCalledWith('browser_tab_inject_agentation', { tabId: 'tab-1' })

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

    await browserAnnotationsSync('delete', ['ann-1'])
    expect(invoke).toHaveBeenCalledWith('browser_annotations_sync', {
      action: 'delete',
      ids: ['ann-1']
    })

    await browserSaveAnnotationCrop('ann-1', 'AA==')
    expect(invoke).toHaveBeenCalledWith('browser_save_annotation_crop', {
      annotationId: 'ann-1',
      base64Png: 'AA=='
    })

    await browserListLocalServers()
    expect(invoke).toHaveBeenCalledWith('browser_list_local_servers', undefined)

    await browserCheckLocalPort(3000)
    expect(invoke).toHaveBeenCalledWith('browser_check_local_port', { port: 3000 })
  })

  it('gates every desktop-only browser command cleanly with NOT_TAURI outside Tauri runtime', async () => {
    mockIsTauri = false
    const results = await Promise.all([
      browserTabCreate('tab-1', 'https://example.com', { x: 0, y: 0, width: 800, height: 600 }),
      browserTabNavigate('tab-1', 'https://example.com'),
      browserTabResize('tab-1', { x: 0, y: 0, width: 800, height: 600 }),
      browserTabShow('tab-1'),
      browserTabHide('tab-1'),
      browserTabDestroy('tab-1'),
      browserTabGoBack('tab-1'),
      browserTabGoForward('tab-1'),
      browserTabReload('tab-1'),
      browserTabStop('tab-1'),
      browserTabSetZoom('tab-1', 1.25),
      browserTabScreenshot('tab-1'),
      browserTabAnnotationCmd('tab-1', { type: 'clear' }),
      browserAnnotationsSync('resolve', ['ann-1']),
      browserSaveAnnotationCrop('ann-1', 'AA=='),
      browserListLocalServers(),
      browserCheckLocalPort(3000),
      browserTabOpenDevtools('tab-1'),
      browserTabInjectAgentation('tab-1')
    ])

    for (const res of results) {
      expect(res.success).toBe(false)
      if (!res.success) expect(res.code).toBe('NOT_TAURI')
    }
    expect(invoke).not.toHaveBeenCalled()
  })
})
