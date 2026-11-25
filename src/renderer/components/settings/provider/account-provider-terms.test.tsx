import { fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { AutoSwitchRiskDialog, SecondAccountNotice } from './account-provider-terms'

const { openUrlWithSystemBrowser, logFrontendError } = vi.hoisted(() => ({
  openUrlWithSystemBrowser: vi.fn(),
  logFrontendError: vi.fn()
}))
vi.mock('@/lib/api', () => ({ openerApi: { openUrlWithSystemBrowser } }))
vi.mock('@/lib/log-api', () => ({ logFrontendError }))

beforeEach(() => {
  vi.clearAllMocks()
  openUrlWithSystemBrowser.mockResolvedValue({ success: true, data: undefined })
})

describe('provider terms links', () => {
  it('Read more opens the risks section in the system browser', () => {
    const onCancel = vi.fn()
    render(<AutoSwitchRiskDialog provider="codex" open onConfirm={vi.fn()} onCancel={onCancel} />)
    const link = within(screen.getByRole('alertdialog')).getByRole('link', { name: 'Read more' })
    const click = new MouseEvent('click', { bubbles: true, cancelable: true })
    fireEvent(link, click)
    expect(click.defaultPrevented).toBe(true)
    expect(openUrlWithSystemBrowser).toHaveBeenCalledTimes(1)
    expect(openUrlWithSystemBrowser).toHaveBeenCalledWith(
      'https://usetroupe.dev/docs/agent-setup/codex-accounts#risks-of-switching-accounts'
    )
    expect(onCancel).not.toHaveBeenCalled()
  })

  it('More info opens the risks section in the system browser', () => {
    render(<SecondAccountNotice provider="antigravity" />)
    const link = within(screen.getByTestId('second-account-notice')).getByRole('link', {
      name: 'More info'
    })
    expect(link).toHaveAttribute(
      'href',
      'https://usetroupe.dev/docs/agent-setup/antigravity-accounts#risks-of-switching-accounts'
    )
    const click = new MouseEvent('click', { bubbles: true, cancelable: true })
    fireEvent(link, click)
    expect(click.defaultPrevented).toBe(true)
    expect(openUrlWithSystemBrowser).toHaveBeenCalledWith(
      'https://usetroupe.dev/docs/agent-setup/antigravity-accounts#risks-of-switching-accounts'
    )
  })

  it('logs a link that fails to open', async () => {
    openUrlWithSystemBrowser.mockResolvedValue({
      success: false,
      error: 'The browser blocked the new tab (popup blocked)',
      code: 'OPEN_URL_ERROR'
    })
    render(<SecondAccountNotice provider="claude" />)
    fireEvent.click(screen.getByRole('link', { name: 'More info' }))
    await waitFor(() =>
      expect(logFrontendError).toHaveBeenCalledWith({
        level: 'warn',
        source: 'account-provider-terms',
        message:
          '[accounts] could not open https://usetroupe.dev/docs/agent-setup/claude-accounts#risks-of-switching-accounts: The browser blocked the new tab (popup blocked)'
      })
    )
  })
})
