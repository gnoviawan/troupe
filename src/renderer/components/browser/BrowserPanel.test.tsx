import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { useRef } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { TooltipProvider } from '@/components/ui/tooltip'
import type {
  AgentationActionPayload,
  AgentationCreatedPayload,
  AgentationUpdatedPayload
} from '@/lib/browser-api'
import { useBrowserSessionStore } from '@/stores/browser-session-store'
import { useProjectStore } from '@/stores/project-store'
import { BrowserPanel } from './BrowserPanel'

const {
  annotationListeners,
  updatedListeners,
  actionListeners,
  mockOpenTerminalAtCwd,
  mockCopyBase64PngToClipboard,
  mockCropBase64Png,
  mockOcclusionPreview
} = vi.hoisted(() => ({
  annotationListeners: [] as Array<(p: AgentationCreatedPayload) => void>,
  updatedListeners: [] as Array<(p: AgentationUpdatedPayload) => void>,
  actionListeners: [] as Array<(p: AgentationActionPayload) => void>,
  mockOpenTerminalAtCwd: vi.fn().mockResolvedValue({ status: 'opened', terminalId: 'term-1' }),
  mockCopyBase64PngToClipboard: vi.fn().mockResolvedValue({ ok: true }),
  mockCropBase64Png: vi
    .fn()
    .mockResolvedValue({ base64: 'CROPPED==', dataUrl: 'data:image/png;base64,CROPPED==' }),
  mockOcclusionPreview: { current: null as string | null }
}))

vi.mock('@/hooks/use-browser-webview', () => ({
  useBrowserWebview: () => ({
    containerRef: useRef<HTMLDivElement>(null),
    occlusionPreviewUrl: mockOcclusionPreview.current
  })
}))

vi.mock('@/lib/browser-screenshot', async () => {
  const actual = await vi.importActual<typeof import('@/lib/browser-screenshot')>(
    '@/lib/browser-screenshot'
  )
  return {
    ...actual,
    cropBase64Png: (...args: unknown[]) => mockCropBase64Png(...args),
    copyBase64PngToClipboard: (...args: unknown[]) => mockCopyBase64PngToClipboard(...args)
  }
})

vi.mock('@/lib/terminal-spawn', () => ({
  openTerminalAtCwd: (...args: unknown[]) => mockOpenTerminalAtCwd(...args)
}))

vi.mock('@/lib/browser-api', () => ({
  browserListLocalServers: vi.fn().mockResolvedValue({
    success: true,
    data: [
      { host: 'localhost:5173', port: 5173, url: 'http://localhost:5173', label: 'Vite dev server' }
    ]
  }),
  browserCheckLocalPort: vi.fn().mockResolvedValue({ success: true, data: false }),
  browserTabGoBack: vi.fn().mockResolvedValue({ success: true }),
  browserTabGoForward: vi.fn().mockResolvedValue({ success: true }),
  browserTabReload: vi.fn().mockResolvedValue({ success: true }),
  browserTabStop: vi.fn().mockResolvedValue({ success: true }),
  browserTabSetZoom: vi.fn().mockResolvedValue({ success: true, data: 1 }),
  browserTabScreenshot: vi
    .fn()
    .mockResolvedValue({ success: true, data: { base64: 'AA==', mimeType: 'image/png' } }),
  browserTabAnnotationCmd: vi.fn().mockResolvedValue({ success: true }),
  browserTabPollAnnotationOutbox: vi.fn().mockResolvedValue({ success: true, data: 0 }),
  browserAnnotationsSync: vi.fn().mockResolvedValue({ success: true, data: 1 }),
  browserSaveAnnotationCrop: vi
    .fn()
    .mockResolvedValue({ success: true, data: '/tmp/annotation-crops/ann-1.png' }),
  browserTabOpenDevtools: vi.fn().mockResolvedValue({ success: true }),
  browserTabInjectAgentation: vi.fn().mockResolvedValue({ success: true }),
  onAgentationAnnotationCreated: (cb: (p: AgentationCreatedPayload) => void) => {
    annotationListeners.push(cb)
    return { unlisten: () => {} }
  },
  onAgentationAnnotationUpdated: (cb: (p: AgentationUpdatedPayload) => void) => {
    updatedListeners.push(cb)
    return { unlisten: () => {} }
  },
  onAgentationActionRequested: (cb: (p: AgentationActionPayload) => void) => {
    actionListeners.push(cb)
    return { unlisten: () => {} }
  }
}))

describe('BrowserPanel', () => {
  beforeEach(() => {
    annotationListeners.length = 0
    actionListeners.length = 0
    mockOpenTerminalAtCwd.mockClear()
    mockCopyBase64PngToClipboard.mockClear()
    mockCropBase64Png.mockClear()
    mockOcclusionPreview.current = null
    useBrowserSessionStore.setState({ tabs: new Map(), recentUrls: [] })
    useProjectStore.setState({
      activeProjectId: 'proj-1',
      projects: [{ id: 'proj-1', name: 'Troupe', path: '/repo' }] as never
    })
  })

  it('renders developer start page when URL is empty and navigates on localhost submission', () => {
    useBrowserSessionStore.getState().createTab('tab-1', '')
    render(
      <TooltipProvider>
        <BrowserPanel browserTabId="tab-1" isVisible />
      </TooltipProvider>
    )
    expect(screen.getByTestId('browser-start-page')).toBeTruthy()
    const input = screen.getByLabelText('Open URL or localhost port')
    fireEvent.change(input, { target: { value: 'localhost:5173' } })
    fireEvent.submit(input.closest('form') as HTMLFormElement)
    expect(useBrowserSessionStore.getState().getTab('tab-1')?.url).toBe('http://localhost:5173')
  })

  it('renders non-blocking loading pill without dimming overlay', () => {
    useBrowserSessionStore.getState().createTab('tab-1', 'https://docs.tauri.app')
    useBrowserSessionStore.getState().setLoading('tab-1', true)
    render(
      <TooltipProvider>
        <BrowserPanel browserTabId="tab-1" isVisible />
      </TooltipProvider>
    )
    expect(screen.getByTestId('browser-loading-pill')).toBeTruthy()
    expect(screen.queryByText('Loading...')).toBeNull()
  })

  it('renders offline view with Open terminal wired and clears error on Retry when port recovers', async () => {
    const { browserCheckLocalPort } = await import('@/lib/browser-api')
    useBrowserSessionStore.getState().createTab('tab-1', 'http://localhost:5173')
    useBrowserSessionStore.getState().setLoadError('tab-1', {
      code: 'ERR_CONNECTION_REFUSED',
      host: 'localhost:5173',
      port: 5173,
      message: 'No local server is listening on port 5173.'
    })
    render(
      <TooltipProvider>
        <BrowserPanel browserTabId="tab-1" isVisible />
      </TooltipProvider>
    )
    expect(screen.getByTestId('browser-offline-view')).toBeTruthy()
    fireEvent.click(screen.getByText('Open terminal'))
    expect(mockOpenTerminalAtCwd).toHaveBeenCalledWith('proj-1', '/repo')

    vi.mocked(browserCheckLocalPort).mockResolvedValueOnce({ success: true, data: true })
    fireEvent.click(screen.getByText('Retry'))
    await waitFor(() => {
      expect(useBrowserSessionStore.getState().getTab('tab-1')?.loadError).toBeNull()
    })
  })

  it('shows responsive viewport sub-bar and resize grips in mobile mode', () => {
    useBrowserSessionStore.getState().createTab('tab-1', 'http://localhost:5173')
    useBrowserSessionStore.getState().setViewportPreset('tab-1', 'mobile')
    render(
      <TooltipProvider>
        <BrowserPanel browserTabId="tab-1" isVisible />
      </TooltipProvider>
    )
    expect(screen.getByTestId('responsive-viewport-bar').getAttribute('data-open')).toBe('true')
    expect(screen.getByLabelText(/left handle/i)).toBeTruthy()
    expect(screen.getByLabelText(/right handle/i)).toBeTruthy()
  })

  it('displays clamped dimensions in ResponsiveViewportBar when the pane is smaller than the Tablet preset', () => {
    const rectSpy = vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue({
      x: 40,
      y: 80,
      width: 620,
      height: 680,
      top: 80,
      right: 660,
      bottom: 760,
      left: 40,
      toJSON: () => ({})
    })
    try {
      useBrowserSessionStore.getState().createTab('tab-1', 'http://localhost:5173')
      useBrowserSessionStore.getState().setViewportPreset('tab-1', 'tablet')
      render(
        <TooltipProvider>
          <BrowserPanel browserTabId="tab-1" isVisible />
        </TooltipProvider>
      )
      const dims = screen.getByTestId('responsive-viewport-dimensions')
      expect(dims.textContent).toBe('620 × 680')
      expect(dims.getAttribute('title')).toContain('Clamped to 620 × 680 (target 768 × 1024)')
    } finally {
      rectSpy.mockRestore()
    }
  })

  it('receives overlay annotations into pending batch queue with auto-crop attached', async () => {
    useBrowserSessionStore.getState().createTab('tab-1', 'http://localhost:5173/signup')
    useBrowserSessionStore.getState().setAnnotateMode('tab-1', true)
    render(
      <TooltipProvider>
        <BrowserPanel browserTabId="tab-1" isVisible />
      </TooltipProvider>
    )

    act(() => {
      // Event for another split pane on the exact same URL must be ignored
      annotationListeners[0]?.({
        id: 'ann-other',
        tabId: 'tab-2',
        url: 'http://localhost:5173/signup',
        kind: 'feedback',
        element: 'button',
        elementPath: 'button.other',
        comment: 'Other pane comment'
      })
      annotationListeners[0]?.({
        id: 'ann-1',
        tabId: 'tab-1',
        url: 'http://localhost:5173/signup',
        kind: 'guide',
        element: 'Vertical guide · x = 32px',
        elementPath: 'div.project-url-card',
        comment: 'Align left edge at x = 32px',
        measurement: { axis: 'vertical', positionPx: 32, deltaPx: 8 }
      })
    })

    await waitFor(() => {
      const pending = useBrowserSessionStore.getState().getTab('tab-1')?.pendingAnnotations ?? []
      expect(pending.length).toBe(1)
      expect(pending[0].id).toBe('ann-1')
      expect(pending[0].kind).toBe('guide')
      expect(pending[0].axis).toBe('vertical')
      expect(pending[0].positionPx).toBe(32)
      expect(pending[0].cropBase64).toBe('AA==')
      expect(pending[0].cropPath).toBe('/tmp/annotation-crops/ann-1.png')
    })

    // Deleting a single pending item syncs both overlay clear and Rust SQLite delete
    const { browserTabAnnotationCmd, browserAnnotationsSync } = await import('@/lib/browser-api')
    fireEvent.click(screen.getByLabelText('View 1 pending comments'))
    fireEvent.click(screen.getByLabelText('Remove comment 1'))
    expect(browserTabAnnotationCmd).toHaveBeenCalledWith('tab-1', {
      type: 'clear',
      ids: ['ann-1']
    })
    expect(browserAnnotationsSync).toHaveBeenCalledWith('delete', ['ann-1'])
  })

  it('preserves measurement gapPx, drops resolved items on AnnotationUpdated, and restores overlay after post-load epoch change', async () => {
    const { browserTabInjectAgentation, browserTabAnnotationCmd } = await import(
      '@/lib/browser-api'
    )
    vi.mocked(browserTabInjectAgentation).mockClear()
    vi.mocked(browserTabAnnotationCmd).mockClear()
    updatedListeners.length = 0
    annotationListeners.length = 0

    useBrowserSessionStore.getState().createTab('tab-1', 'http://localhost:5173/signup')
    useBrowserSessionStore.getState().setAnnotateMode('tab-1', true)
    useBrowserSessionStore.getState().applyView({
      tabId: 'tab-1',
      url: 'http://localhost:5173/signup',
      title: 'Signup',
      loading: false,
      agentControlled: false,
      epoch: 1
    })

    render(
      <TooltipProvider>
        <BrowserPanel browserTabId="tab-1" isVisible />
      </TooltipProvider>
    )

    // Ruler measurement preserves gapPx without mapping to deltaPx
    act(() => {
      annotationListeners[0]?.({
        id: 'ann-gap',
        tabId: 'tab-1',
        url: 'http://localhost:5173/signup',
        kind: 'measurement',
        element: 'Spacing · 24px (header)',
        elementPath: 'header',
        comment: 'Keep 24px gap',
        measurement: { axis: 'vertical', positionPx: 24, gapPx: 24 }
      })
    })
    await waitFor(() => {
      const item = useBrowserSessionStore.getState().getTab('tab-1')?.pendingAnnotations[0]
      expect(item?.gapPx).toBe(24)
      expect(item?.deltaPx).toBeUndefined()
    })

    // External MCP resolve removes pending item and clears its pin
    act(() => {
      updatedListeners[0]?.({ id: 'ann-gap', tabId: 'tab-1', status: 'resolved' })
    })
    expect(useBrowserSessionStore.getState().getTab('tab-1')?.pendingAnnotations).toHaveLength(0)
    expect(browserTabAnnotationCmd).toHaveBeenCalledWith('tab-1', {
      type: 'clear',
      ids: ['ann-gap']
    })

    // Epoch increments while loading=true, then load finishes -> overlay must still restore
    act(() => {
      useBrowserSessionStore.getState().applyView({
        tabId: 'tab-1',
        url: 'http://localhost:5173/next',
        title: 'Next',
        loading: true,
        agentControlled: false,
        epoch: 2
      })
    })
    expect(browserTabInjectAgentation).not.toHaveBeenCalled()

    act(() => {
      useBrowserSessionStore.getState().applyView({
        tabId: 'tab-1',
        url: 'http://localhost:5173/next',
        title: 'Next',
        loading: false,
        agentControlled: false,
        epoch: 2
      })
    })
    await waitFor(() => expect(browserTabInjectAgentation).toHaveBeenCalledWith('tab-1'))
  })

  it('copies area screenshot to clipboard without queuing into Send to chat and exits standalone mode', async () => {
    const { browserTabAnnotationCmd } = await import('@/lib/browser-api')
    useBrowserSessionStore.getState().createTab('tab-1', 'https://absenin.id')
    useBrowserSessionStore.getState().setAnnotateMode('tab-1', true)
    useBrowserSessionStore.getState().setAnnotateTool('tab-1', 'screenshot-area')
    useBrowserSessionStore.getState().setStandaloneScreenshotArea('tab-1', true)

    render(
      <TooltipProvider>
        <BrowserPanel browserTabId="tab-1" isVisible />
      </TooltipProvider>
    )

    act(() => {
      actionListeners[0]?.({
        tabId: 'tab-1',
        output: JSON.stringify({
          kind: 'screenshot_area',
          tabId: 'tab-1',
          rect: { x: 10, y: 20, width: 200, height: 120 },
          dpr: 2,
          viewportWidth: 1200
        })
      })
    })

    await waitFor(() => expect(mockCopyBase64PngToClipboard).toHaveBeenCalledWith('CROPPED=='))
    expect(browserTabAnnotationCmd).toHaveBeenCalledWith('tab-1', {
      type: 'set_capture_hidden',
      hidden: true
    })
    expect(browserTabAnnotationCmd).toHaveBeenCalledWith('tab-1', {
      type: 'set_capture_hidden',
      hidden: false
    })
    expect(useBrowserSessionStore.getState().getTab('tab-1')?.pendingAnnotations).toHaveLength(0)
    expect(useBrowserSessionStore.getState().getTab('tab-1')?.annotateMode).toBe(false)
    expect(useBrowserSessionStore.getState().getTab('tab-1')?.lastScreenshotCopiedAt).toBeDefined()
    expect(browserTabAnnotationCmd).toHaveBeenCalledWith('tab-1', {
      type: 'set_active',
      active: false
    })
  })

  it('scopes area-capture cleanup by capture token so late completion cannot cancel a newer capture', async () => {
    const { browserTabScreenshot } = await import('@/lib/browser-api')
    let resolveFirstShot:
      | ((v: { success: true; data: { base64: string; mimeType: string } }) => void)
      | null = null
    vi.mocked(browserTabScreenshot).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveFirstShot = resolve
        })
    )

    useBrowserSessionStore.getState().createTab('tab-1', 'https://absenin.id')
    useBrowserSessionStore.getState().setAnnotateMode('tab-1', true)
    useBrowserSessionStore.getState().setAnnotateTool('tab-1', 'screenshot-area')
    useBrowserSessionStore.getState().setStandaloneScreenshotArea('tab-1', true)
    useBrowserSessionStore.getState().bumpScreenshotAreaToken('tab-1')

    render(
      <TooltipProvider>
        <BrowserPanel browserTabId="tab-1" isVisible />
      </TooltipProvider>
    )

    act(() => {
      actionListeners[0]?.({
        tabId: 'tab-1',
        output: JSON.stringify({
          kind: 'screenshot_area',
          tabId: 'tab-1',
          rect: { x: 10, y: 20, width: 100, height: 80 }
        })
      })
    })

    await waitFor(() => expect(resolveFirstShot).not.toBeNull())

    // User starts a second area capture before the first screenshot resolves
    act(() => {
      useBrowserSessionStore.getState().bumpScreenshotAreaToken('tab-1')
    })

    await act(async () => {
      resolveFirstShot?.({ success: true, data: { base64: 'AA==', mimeType: 'image/png' } })
      await Promise.resolve()
    })

    await waitFor(() => expect(mockCopyBase64PngToClipboard).toHaveBeenCalledWith('CROPPED=='))
    // Second capture remains active and undisturbed
    expect(useBrowserSessionStore.getState().getTab('tab-1')?.annotateMode).toBe(true)
    expect(useBrowserSessionStore.getState().getTab('tab-1')?.standaloneScreenshotArea).toBe(true)
  })

  it('removes guide annotations on annotations_removed action and clears pending annotations across cross-origin navigation', async () => {
    const { browserAnnotationsSync } = await import('@/lib/browser-api')
    useBrowserSessionStore.getState().createTab('tab-1', 'https://absenin.id')
    useBrowserSessionStore.getState().setAnnotateMode('tab-1', true)
    useBrowserSessionStore.getState().addPendingAnnotation('tab-1', {
      id: 'ann-g1',
      kind: 'guide',
      element: 'Vertical guide',
      elementPath: 'div.hero',
      comment: 'Align'
    })
    useBrowserSessionStore.getState().addPendingAnnotation('tab-1', {
      id: 'ann-keep',
      kind: 'feedback',
      element: 'h1',
      elementPath: 'h1',
      comment: 'Keep until navigation'
    })

    render(
      <TooltipProvider>
        <BrowserPanel browserTabId="tab-1" isVisible />
      </TooltipProvider>
    )

    act(() => {
      actionListeners[0]?.({
        tabId: 'tab-1',
        output: JSON.stringify({
          kind: 'annotations_removed',
          tabId: 'tab-1',
          ids: ['ann-g1']
        })
      })
    })

    expect(
      useBrowserSessionStore
        .getState()
        .getTab('tab-1')
        ?.pendingAnnotations.map((a) => a.id)
    ).toEqual(['ann-keep'])
    expect(browserAnnotationsSync).toHaveBeenCalledWith('delete', ['ann-g1'])

    // Navigating to another origin clears stale pending annotations from the previous site
    act(() => {
      useBrowserSessionStore.getState().updateUrl('tab-1', 'http://127.0.0.1:18765/')
    })
    expect(useBrowserSessionStore.getState().getTab('tab-1')?.pendingAnnotations).toHaveLength(0)
  })

  it('renders frozen occlusion snapshot image behind open dropdowns when provided by useBrowserWebview', () => {
    mockOcclusionPreview.current = 'data:image/png;base64,AA=='
    useBrowserSessionStore.getState().createTab('tab-1', 'https://absenin.id')
    render(
      <TooltipProvider>
        <BrowserPanel browserTabId="tab-1" isVisible />
      </TooltipProvider>
    )
    const img = screen.getByTestId('browser-occlusion-preview') as HTMLImageElement
    expect(img.getAttribute('src')).toBe('data:image/png;base64,AA==')
  })
})
