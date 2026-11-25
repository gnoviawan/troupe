import { fireEvent, render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { TooltipProvider } from '@/components/ui/tooltip'
import { useBrowserSessionStore } from '@/stores/browser-session-store'
import { BrowserPanel } from './BrowserPanel'

vi.mock('@/hooks/use-browser-webview', () => ({
  useBrowserWebview: () => ({ containerRef: { current: null } })
}))

vi.mock('@/lib/browser-api', () => ({
  browserListLocalServers: vi.fn().mockResolvedValue({
    success: true,
    data: [
      { host: 'localhost:5173', port: 5173, url: 'http://localhost:5173', label: 'Vite dev server' }
    ]
  }),
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

describe('BrowserPanel', () => {
  beforeEach(() => {
    useBrowserSessionStore.setState({ tabs: new Map(), recentUrls: [] })
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

  it('renders offline error view when loadError is set and clears on Retry', () => {
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
    fireEvent.click(screen.getByText('Retry'))
    expect(useBrowserSessionStore.getState().getTab('tab-1')?.loadError).toBeNull()
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
})
