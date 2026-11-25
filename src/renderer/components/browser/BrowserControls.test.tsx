import { act, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { TooltipProvider } from '@/components/ui/tooltip'
import { useBrowserSessionStore } from '@/stores/browser-session-store'
import { BrowserControls } from './BrowserControls'

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

  it('respects canGoBack and canGoForward disabled states', () => {
    useBrowserSessionStore.getState().createTab('tab-1', 'https://example.com')
    useBrowserSessionStore.getState().setNavCapabilities('tab-1', true, false)
    renderWithProvider(<BrowserControls browserTabId="tab-1" />)
    expect(screen.getByLabelText('Back')).toHaveProperty('disabled', false)
    expect(screen.getByLabelText('Forward')).toHaveProperty('disabled', true)
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

  it('handles window-level keyboard shortcuts when browser tab is visible', async () => {
    const { browserTabReload, browserTabAnnotationCmd } = await import('@/lib/browser-api')
    useBrowserSessionStore.getState().createTab('tab-1', 'https://example.com')
    renderWithProvider(<BrowserControls browserTabId="tab-1" isVisible />)

    fireEvent.keyDown(window, { key: '=', metaKey: true })
    expect(useBrowserSessionStore.getState().getTab('tab-1')?.zoomFactor).toBe(1.1)

    fireEvent.keyDown(window, { key: '0', metaKey: true })
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
})
