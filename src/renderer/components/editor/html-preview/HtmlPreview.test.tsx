import { act, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { TooltipProvider } from '@/components/ui/tooltip'

const { api, logFrontendError } = vi.hoisted(() => ({
  api: { open: vi.fn(), update: vi.fn(), close: vi.fn() },
  logFrontendError: vi.fn()
}))

vi.mock('@/lib/html-preview-api', () => ({ htmlPreviewApi: api }))
vi.mock('@/lib/log-api', () => ({ logFrontendError }))
vi.mock('@/hooks/use-mobile-web-shell', () => ({ useMobileWebShell: () => false }))

import { HtmlPreview } from './HtmlPreview'

const SESSION = { token: 'tok1', entry: 'my page.html', baseUrl: 'http://127.0.0.1:4100' }

function renderPreview(content = '<p>hi</p>'): ReturnType<typeof render> {
  return render(
    <TooltipProvider>
      <HtmlPreview filePath="/site/my page.html" content={content} />
    </TooltipProvider>
  )
}

function post(source: MessageEventSource | null, data: unknown): void {
  act(() => {
    window.dispatchEvent(new MessageEvent('message', { data, source }))
  })
}

describe('HtmlPreview', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    api.open.mockResolvedValue({ success: true, data: SESSION })
    api.update.mockResolvedValue({ success: true, data: null })
    api.close.mockResolvedValue({ success: true, data: null })
  })

  it('loads the session page in a sandbox without allow-same-origin, after sending the buffer', async () => {
    renderPreview()

    const frame = await screen.findByTitle('Preview of my page.html')

    expect(frame.getAttribute('sandbox')).toBe(
      'allow-scripts allow-forms allow-modals allow-popups'
    )
    expect(frame.getAttribute('src')).toBe('http://127.0.0.1:4100/preview/tok1/my%20page.html?r=0')
    expect(api.open).toHaveBeenCalledWith('/site/my page.html')
    expect(api.update).toHaveBeenCalledTimes(1)
    const [token, sent] = api.update.mock.calls[0]
    expect(token).toBe('tok1')
    expect(sent.endsWith('</script><p>hi</p>')).toBe(true)
  })

  it('counts console messages from its own frame only', async () => {
    renderPreview()
    const frame = (await screen.findByTitle('Preview of my page.html')) as HTMLIFrameElement
    const own = frame.contentWindow
    expect(screen.getByRole('button', { name: 'Show console, 0 messages' })).toBeInTheDocument()

    post(own, { source: 'troupe-html-preview', level: 'log', text: 'ready' })
    expect(screen.getByRole('button', { name: 'Show console, 1 message' })).toBeInTheDocument()

    post(window, { source: 'troupe-html-preview', level: 'error', text: 'not mine' })
    post(own, { source: 'someone-else', level: 'error', text: 'wrong shape' })
    expect(screen.getByRole('button', { name: 'Show console, 1 message' })).toBeInTheDocument()

    post(own, { source: 'troupe-html-preview', level: 'error', text: 'boom' })
    const toggle = screen.getByRole('button', { name: 'Show console, 1 error' })
    expect(toggle).toHaveTextContent('1 error')
    expect(toggle).toHaveClass('text-destructive')
  })

  it('closes the session when it unmounts', async () => {
    const { unmount } = renderPreview()
    await screen.findByTitle('Preview of my page.html')

    unmount()

    expect(api.close).toHaveBeenCalledWith('tok1')
  })

  it('shows a quiet message and logs the code when the host cannot open a session', async () => {
    api.open.mockResolvedValue({
      success: false,
      error: 'canvas bundle listener is not running',
      code: 'BUNDLE_LISTENER_DOWN'
    })

    renderPreview()

    expect(
      await screen.findByText('The preview could not be loaded. Reload to try again.')
    ).toBeInTheDocument()
    expect(screen.queryByTitle('Preview of my page.html')).toBeNull()
    await waitFor(() =>
      expect(logFrontendError).toHaveBeenCalledWith({
        level: 'warn',
        source: 'htmlPreview.open',
        message: "html preview of 'my page.html' failed: BUNDLE_LISTENER_DOWN"
      })
    )
  })
})
