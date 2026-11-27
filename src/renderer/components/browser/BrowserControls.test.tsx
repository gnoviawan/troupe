import { act, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { TooltipProvider } from '@/components/ui/tooltip'
import { useBrowserSessionStore } from '@/stores/browser-session-store'
import { BrowserControls } from './BrowserControls'

const { mockOpenUrlWithSystemBrowser, mockCopyBase64PngToClipboard } = vi.hoisted(() => ({
  mockOpenUrlWithSystemBrowser: vi.fn().mockResolvedValue({ success: true, data: undefined }),
  mockCopyBase64PngToClipboard: vi.fn().mockResolvedValue({ ok: true })
}))

vi.mock('@/lib/browser-screenshot', async () => {
  const actual = await vi.importActual<typeof import('@/lib/browser-screenshot')>(
    '@/lib/browser-screenshot'
  )
  return {
    ...actual,
    copyBase64PngToClipboard: (...args: unknown[]) => mockCopyBase64PngToClipboard(...args)
  }
})

vi.mock('@/lib/tauri-opener-api', () => ({
  openerApi: {
    openUrlWithSystemBrowser: (...args: unknown[]) => mockOpenUrlWithSystemBrowser(...args)
  }
}))

vi.mock('@/lib/browser-api', () => ({
  browserTabGoBack: vi.fn().mockResolvedValue({ success: true }),
  browserTabGoForward: vi.fn().mockResolvedValue({ success: true }),
  browserTabReload: vi.fn().mockResolvedValue({ success: true }),
  browserTabStop: vi.fn().mockResolvedValue({ success: true }),
  browserTabSetZoom: vi.fn().mockResolvedValue({ success: true, data: 1 }),
  browserTabScreenshot: vi
    .fn()
    .mockResolvedValue({ success: true, data: { base64: 'AA==', mimeType: 'image/png' } }),
  browserTabAnnotationCmd: vi.fn().mockResolvedValue({ success: true }),
  browserAnnotationsSync: vi.fn().mockResolvedValue({ success: true, data: 1 }),
  browserSaveAnnotationCrop: vi.fn().mockResolvedValue({ success: true, data: '/tmp/crop.png' }),
  browserTabOpenDevtools: vi.fn().mockResolvedValue({ success: true }),
  browserTabInjectAgentation: vi.fn().mockResolvedValue({ success: true })
}))

vi.mock('@/lib/browser-consent-api', () => ({
  stopBrowserAgent: vi.fn().mockResolvedValue(true)
}))

function Wrapper({ children }: { children: React.ReactNode }) {
  return <TooltipProvider>{children}</TooltipProvider>
}

function renderWithProvider(ui: React.ReactElement) {
  return render(ui, { wrapper: Wrapper })
}

describe('BrowserControls', () => {
  beforeEach(() => {
    useBrowserSessionStore.setState({ tabs: new Map(), recentUrls: [] })
  })

  afterEach(() => {
    vi.clearAllMocks()
    vi.unstubAllEnvs()
  })

  it('renders empty toolbar with disabled actions on new tab start page', () => {
    useBrowserSessionStore.getState().createTab('tab-1', '')
    renderWithProvider(<BrowserControls browserTabId="tab-1" />)
    expect(screen.getByLabelText('Back')).toHaveProperty('disabled', true)
    expect(screen.getByLabelText('Forward')).toHaveProperty('disabled', true)
    expect(screen.getByLabelText('Annotate page')).toHaveProperty('disabled', true)
  })

  it('enables Back and Forward once a URL is loaded and invokes native history commands', async () => {
    const { browserTabGoBack, browserTabGoForward } = await import('@/lib/browser-api')
    useBrowserSessionStore.getState().createTab('tab-1', 'https://example.com')
    renderWithProvider(<BrowserControls browserTabId="tab-1" />)
    const back = screen.getByLabelText('Back')
    const forward = screen.getByLabelText('Forward')
    expect(back).toHaveProperty('disabled', false)
    expect(forward).toHaveProperty('disabled', false)
    fireEvent.click(back)
    fireEvent.click(forward)
    expect(browserTabGoBack).toHaveBeenCalledWith('tab-1')
    expect(browserTabGoForward).toHaveBeenCalledWith('tab-1')
  })

  it('opens external URL through canonical openerApi.openUrlWithSystemBrowser', () => {
    useBrowserSessionStore.getState().createTab('tab-1', 'https://example.com/docs')
    renderWithProvider(<BrowserControls browserTabId="tab-1" />)
    fireEvent.click(screen.getByLabelText('More browser actions'))
    fireEvent.click(screen.getByText('Open in default browser'))
    expect(mockOpenUrlWithSystemBrowser).toHaveBeenCalledWith('https://example.com/docs')
  })

  it('reverts unsubmitted URL input on blur instead of navigating', () => {
    useBrowserSessionStore.getState().createTab('tab-1', 'https://example.com/page')
    renderWithProvider(<BrowserControls browserTabId="tab-1" />)
    const input = screen.getByLabelText('Address bar') as HTMLInputElement
    fireEvent.focus(input)
    fireEvent.change(input, { target: { value: 'https://unsaved.test' } })
    fireEvent.blur(input)
    expect(useBrowserSessionStore.getState().getTab('tab-1')?.url).toBe('https://example.com/page')
    expect(input.value).toBe('https://example.com/page')
  })

  it('swaps URL bar into AnnotationToolbar on Annotate click and injects overlay', async () => {
    const { browserTabInjectAgentation } = await import('@/lib/browser-api')
    useBrowserSessionStore.getState().createTab('tab-1', 'https://example.com')
    renderWithProvider(<BrowserControls browserTabId="tab-1" />)

    fireEvent.click(screen.getByLabelText('Annotate page'))
    expect(browserTabInjectAgentation).toHaveBeenCalledWith('tab-1')
    expect(screen.getByTestId('browser-annotation-toolbar')).toBeTruthy()
    expect(screen.getByTitle('Comment (C)')).toBeTruthy()
    expect(screen.getByTitle('Rectangle (R)')).toBeTruthy()
    expect(screen.getByTitle('Ruler (M)')).toBeTruthy()

    fireEvent.click(screen.getByLabelText('Exit annotate mode'))
    expect(useBrowserSessionStore.getState().getTab('tab-1')?.annotateMode).toBe(false)
  })

  it('cycles Desktop -> Tablet -> Mobile -> Desktop on Device toggle click', () => {
    useBrowserSessionStore.getState().createTab('tab-1', 'https://example.com')
    renderWithProvider(<BrowserControls browserTabId="tab-1" />)
    const toggle = screen.getByTestId('browser-device-toggle')

    fireEvent.click(toggle)
    expect(useBrowserSessionStore.getState().getTab('tab-1')?.viewportPreset).toBe('tablet')
    fireEvent.click(toggle)
    expect(useBrowserSessionStore.getState().getTab('tab-1')?.viewportPreset).toBe('mobile')
    fireEvent.click(toggle)
    expect(useBrowserSessionStore.getState().getTab('tab-1')?.viewportPreset).toBe('desktop')
  })

  it('shows zoom pill only when zoomed away from 100% in desktop mode', () => {
    useBrowserSessionStore.getState().createTab('tab-1', 'https://example.com')
    renderWithProvider(<BrowserControls browserTabId="tab-1" />)
    expect(screen.queryByLabelText(/Page zoom/)).toBeNull()

    act(() => {
      useBrowserSessionStore.getState().setZoomFactor('tab-1', 1.25)
    })
    expect(screen.getByLabelText('Page zoom 125%')).toBeTruthy()

    act(() => {
      useBrowserSessionStore.getState().setViewportPreset('tab-1', 'mobile')
    })
    expect(screen.queryByLabelText('Page zoom 125%')).toBeNull()
  })

  it('shows 2px loading bar and Stop button while loading', () => {
    useBrowserSessionStore.getState().createTab('tab-1', 'https://example.com')
    useBrowserSessionStore.getState().setLoading('tab-1', true)
    renderWithProvider(<BrowserControls browserTabId="tab-1" />)

    expect(screen.getByTestId('browser-loading-bar')).toBeTruthy()
    expect(screen.getByLabelText('Stop loading')).toBeTruthy()
  })

  it('hides Debug Console in More menu when PROD is true', () => {
    vi.stubEnv('PROD', true)
    useBrowserSessionStore.getState().createTab('tab-1', 'https://example.com')
    renderWithProvider(<BrowserControls browserTabId="tab-1" />)
    fireEvent.click(screen.getByLabelText('More browser actions'))
    expect(screen.queryByLabelText('Open debug console')).toBeNull()
  })

  it('reverts annotateMode when overlay injection fails', async () => {
    const { browserTabInjectAgentation } = await import('@/lib/browser-api')
    vi.mocked(browserTabInjectAgentation).mockResolvedValueOnce({
      success: false,
      error: 'Injection failed'
    })
    useBrowserSessionStore.getState().createTab('tab-1', 'https://example.com')
    renderWithProvider(<BrowserControls browserTabId="tab-1" />)

    fireEvent.click(screen.getByLabelText('Annotate page'))
    await act(async () => {
      await Promise.resolve()
    })

    expect(useBrowserSessionStore.getState().getTab('tab-1')?.annotateMode).toBe(false)
  })

  it('handles browser shortcuts without hijacking workspace zoom shortcuts', async () => {
    const { browserTabReload, browserTabAnnotationCmd } = await import('@/lib/browser-api')
    useBrowserSessionStore.getState().createTab('tab-1', 'https://example.com')
    renderWithProvider(<BrowserControls browserTabId="tab-1" isVisible />)

    // Cmd+= is owned by use-workspace-keyboard (avoids double-zoom collision)
    fireEvent.keyDown(window, { key: '=', metaKey: true })
    expect(useBrowserSessionStore.getState().getTab('tab-1')?.zoomFactor).toBe(1)

    fireEvent.keyDown(window, { key: 'r', metaKey: true, shiftKey: true })
    expect(browserTabReload).toHaveBeenCalledWith('tab-1')

    act(() => {
      useBrowserSessionStore.getState().setAnnotateMode('tab-1', true)
    })
    fireEvent.keyDown(window, { key: 'm' })
    expect(useBrowserSessionStore.getState().getTab('tab-1')?.annotateTool).toBe('ruler')
    expect(browserTabAnnotationCmd).toHaveBeenCalledWith('tab-1', {
      type: 'set_tool',
      tool: 'ruler'
    })
  })

  it('copies visible screenshot to clipboard without queuing into pending annotations', async () => {
    const { browserTabScreenshot } = await import('@/lib/browser-api')
    useBrowserSessionStore.getState().createTab('tab-1', 'https://absenin.id')
    renderWithProvider(<BrowserControls browserTabId="tab-1" isVisible />)

    fireEvent.click(screen.getByLabelText('Screenshot options'))
    fireEvent.click(screen.getByText('Screenshot visible'))

    await act(async () => {
      await Promise.resolve()
      await Promise.resolve()
    })

    expect(browserTabScreenshot).toHaveBeenCalledWith('tab-1')
    expect(mockCopyBase64PngToClipboard).toHaveBeenCalledWith('AA==')
    expect(useBrowserSessionStore.getState().getTab('tab-1')?.pendingAnnotations).toHaveLength(0)
    expect(screen.getByTestId('browser-screenshot-copied-pill').textContent).toContain('Copied')
  })

  it('temporarily hides and restores annotation overlay around Visible screenshot when annotating', async () => {
    const { browserTabAnnotationCmd, browserTabScreenshot } = await import('@/lib/browser-api')
    const callOrder: string[] = []
    vi.mocked(browserTabAnnotationCmd).mockImplementation(async (_id, cmd) => {
      callOrder.push(`cmd:${cmd.type}:${'hidden' in cmd ? String(cmd.hidden) : ''}`)
      return { success: true }
    })
    vi.mocked(browserTabScreenshot).mockImplementation(async () => {
      callOrder.push('screenshot')
      return { success: true, data: { base64: 'AA==', mimeType: 'image/png' } }
    })

    useBrowserSessionStore.getState().createTab('tab-1', 'https://absenin.id')
    useBrowserSessionStore.getState().setAnnotateMode('tab-1', true)
    renderWithProvider(<BrowserControls browserTabId="tab-1" isVisible />)

    fireEvent.click(screen.getByLabelText('Screenshot visible viewport'))

    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 60))
    })

    expect(callOrder).toEqual([
      'cmd:set_capture_hidden:true',
      'screenshot',
      'cmd:set_capture_hidden:false'
    ])
    expect(screen.getByTestId('browser-screenshot-copied-pill').textContent).toContain('Copied')
  })

  it('does not show stale Copied pill on remount when lastScreenshotCopiedAt is expired', () => {
    useBrowserSessionStore.getState().createTab('tab-1', 'https://absenin.id')
    useBrowserSessionStore.setState((state) => {
      const next = new Map(state.tabs)
      const tab = next.get('tab-1')
      if (tab) {
        next.set('tab-1', { ...tab, lastScreenshotCopiedAt: Date.now() - 5000 })
      }
      return { tabs: next }
    })
    renderWithProvider(<BrowserControls browserTabId="tab-1" isVisible />)
    expect(screen.queryByTestId('browser-screenshot-copied-pill')).toBeNull()
  })

  it('gives the Eye toggle explicit annotations & guides labels and sends set_pins_hidden', async () => {
    const { browserTabAnnotationCmd } = await import('@/lib/browser-api')
    useBrowserSessionStore.getState().createTab('tab-1', 'https://absenin.id')
    useBrowserSessionStore.getState().setAnnotateMode('tab-1', true)
    renderWithProvider(<BrowserControls browserTabId="tab-1" isVisible />)

    const hideBtn = screen.getByLabelText('Hide annotations & guides')
    fireEvent.click(hideBtn)
    expect(browserTabAnnotationCmd).toHaveBeenCalledWith('tab-1', {
      type: 'set_pins_hidden',
      hidden: true
    })
    expect(screen.getByLabelText('Show annotations & guides')).toBeTruthy()
  })
})
