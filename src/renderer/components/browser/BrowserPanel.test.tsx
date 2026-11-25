import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { useRef } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { TooltipProvider } from '@/components/ui/tooltip'
import type { AgentationActionPayload, AgentationCreatedPayload } from '@/lib/browser-api'
import { useBrowserSessionStore } from '@/stores/browser-session-store'
import { useProjectStore } from '@/stores/project-store'
import { BrowserPanel } from './BrowserPanel'

const { annotationListeners, actionListeners, mockOpenTerminalAtCwd } = vi.hoisted(() => ({
  annotationListeners: [] as Array<(p: AgentationCreatedPayload) => void>,
  actionListeners: [] as Array<(p: AgentationActionPayload) => void>,
  mockOpenTerminalAtCwd: vi.fn().mockResolvedValue({ status: 'opened', terminalId: 'term-1' })
}))

vi.mock('@/hooks/use-browser-webview', () => ({
  useBrowserWebview: () => ({ containerRef: useRef<HTMLDivElement>(null) })
}))

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
  browserTabOpenDevtools: vi.fn().mockResolvedValue({ success: true }),
  browserTabInjectAgentation: vi.fn().mockResolvedValue({ success: true }),
  onAgentationAnnotationCreated: (cb: (p: AgentationCreatedPayload) => void) => {
    annotationListeners.push(cb)
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
      annotationListeners[0]?.({
        id: 'ann-1',
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
      expect(pending[0].kind).toBe('guide')
      expect(pending[0].axis).toBe('vertical')
      expect(pending[0].positionPx).toBe(32)
      expect(pending[0].cropBase64).toBe('AA==')
    })
  })
})
