import { fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { AccountGroupFooter } from './AccountGroupFooter'
import type { AccountProviderId } from './account-provider-terms'

const { logFrontendError } = vi.hoisted(() => ({ logFrontendError: vi.fn() }))
vi.mock('@/lib/log-api', () => ({ logFrontendError }))

const SHARED =
  'A provider can suspend all the accounts of one person together. Safer options are to switch by hand when a chat reaches its limit, buy extra usage or credits, or use an API key. Read more'

function renderFooter(
  props: Partial<{ provider: AccountProviderId; autoSwitch: boolean; canManage: boolean }> = {}
) {
  const onAutoSwitchChange = vi.fn()
  render(
    <AccountGroupFooter
      provider={props.provider ?? 'claude'}
      accountCount={2}
      canManage={props.canManage ?? true}
      autoSwitch={props.autoSwitch ?? false}
      saving={false}
      onAutoSwitchChange={onAutoSwitchChange}
    />
  )
  const toggle = screen.getByRole('switch', { name: 'Switch accounts automatically' })
  return { toggle, onAutoSwitchChange }
}

beforeEach(() => {
  vi.clearAllMocks()
})

describe('AccountGroupFooter', () => {
  it('describes the setting as off by default with a risk', () => {
    const { toggle } = renderFooter()
    expect(toggle).toHaveAccessibleDescription(
      "When a chat reaches its account's limit, move it to another signed-in account. Off by default, because Anthropic's terms may not allow it."
    )
  })

  it('turning it on shows the risk warning and leaves the setting off', () => {
    const { toggle, onAutoSwitchChange } = renderFooter()
    fireEvent.click(toggle)
    const dialog = screen.getByRole('alertdialog')
    expect(within(dialog).getByText('Switch Claude accounts automatically?')).toBeInTheDocument()
    expect(onAutoSwitchChange).not.toHaveBeenCalled()
    expect(toggle).toHaveAttribute('aria-checked', 'false')
  })

  it('Turn on anyway turns it on and logs the choice', async () => {
    const { toggle, onAutoSwitchChange } = renderFooter()
    fireEvent.click(toggle)
    fireEvent.click(screen.getByRole('button', { name: 'Turn on anyway' }))
    expect(onAutoSwitchChange).toHaveBeenCalledTimes(1)
    expect(onAutoSwitchChange).toHaveBeenCalledWith(true)
    expect(logFrontendError).toHaveBeenCalledWith({
      level: 'info',
      source: 'AccountGroupFooter',
      message: '[accounts] claude automatic switching risk warning accepted'
    })
    await waitFor(() => expect(screen.queryByRole('alertdialog')).not.toBeInTheDocument())
  })

  it('Cancel and Escape leave it off', async () => {
    const { toggle, onAutoSwitchChange } = renderFooter()
    fireEvent.click(toggle)
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }))
    await waitFor(() => expect(screen.queryByRole('alertdialog')).not.toBeInTheDocument())
    fireEvent.click(toggle)
    fireEvent.keyDown(window, { key: 'Escape' })
    await waitFor(() => expect(screen.queryByRole('alertdialog')).not.toBeInTheDocument())
    expect(onAutoSwitchChange).not.toHaveBeenCalled()
    expect(toggle).toHaveAttribute('aria-checked', 'false')
    expect(logFrontendError).toHaveBeenLastCalledWith({
      level: 'info',
      source: 'AccountGroupFooter',
      message: '[accounts] claude automatic switching risk warning cancelled'
    })
  })

  it('turning it off needs no confirmation', () => {
    const { toggle, onAutoSwitchChange } = renderFooter({ autoSwitch: true })
    fireEvent.click(toggle)
    expect(onAutoSwitchChange).toHaveBeenCalledWith(false)
    expect(screen.queryByRole('alertdialog')).not.toBeInTheDocument()
  })

  it('on a read-only connection the switch is disabled and opens no warning', () => {
    const { toggle, onAutoSwitchChange } = renderFooter({ canManage: false })
    expect(toggle).toBeDisabled()
    fireEvent.click(toggle)
    expect(screen.queryByRole('alertdialog')).not.toBeInTheDocument()
    expect(onAutoSwitchChange).not.toHaveBeenCalled()
  })

  it.each([
    ['the account count drops below two', { accountCount: 1 }],
    ['the connection turns read-only', { canManage: false }],
    ['another client turns it on', { autoSwitch: true }]
  ] as const)('an open warning closes for good when %s', async (_, change) => {
    const onAutoSwitchChange = vi.fn()
    const props = {
      provider: 'claude' as const,
      accountCount: 2,
      canManage: true,
      autoSwitch: false,
      saving: false,
      onAutoSwitchChange
    }
    const { rerender } = render(<AccountGroupFooter {...props} />)
    fireEvent.click(screen.getByRole('switch', { name: 'Switch accounts automatically' }))
    expect(screen.getByRole('alertdialog')).toBeInTheDocument()
    rerender(<AccountGroupFooter {...props} {...change} />)
    await waitFor(() => expect(screen.queryByRole('alertdialog')).not.toBeInTheDocument())
    rerender(<AccountGroupFooter {...props} />)
    expect(screen.queryByRole('alertdialog')).not.toBeInTheDocument()
    expect(onAutoSwitchChange).not.toHaveBeenCalled()
  })

  it.each([
    [
      'claude',
      'Switch Claude accounts automatically?',
      'Anthropic says Claude plan limits assume ordinary, individual use. Its terms do not allow access by automated means unless Anthropic permits it, and Anthropic can suspend an account without notice.',
      'https://usetroupe.dev/docs/agent-setup/claude-accounts#risks-of-switching-accounts'
    ],
    [
      'codex',
      'Switch Codex accounts automatically?',
      "OpenAI's terms do not allow getting around rate limits or restrictions. Moving a chat to another account at its Codex limit can count as that. OpenAI's own answer to a Codex limit is to buy credits or use an API key.",
      'https://usetroupe.dev/docs/agent-setup/codex-accounts#risks-of-switching-accounts'
    ],
    [
      'antigravity',
      'Switch Antigravity accounts automatically?',
      "Google's Antigravity FAQ says automated account rotation is not supported on consumer plans. Google has suspended Antigravity access for breaking its terms.",
      'https://usetroupe.dev/docs/agent-setup/antigravity-accounts#risks-of-switching-accounts'
    ]
  ] as const)('the %s warning names its own terms', (provider, title, risk, href) => {
    const { toggle } = renderFooter({ provider })
    fireEvent.click(toggle)
    const dialog = screen.getByRole('alertdialog')
    expect(dialog).toHaveAccessibleName(title)
    expect(dialog).toHaveAccessibleDescription(risk)
    expect(within(dialog).getByText(/^A provider can suspend/)).toHaveTextContent(SHARED)
    expect(within(dialog).getByRole('link', { name: 'Read more' })).toHaveAttribute('href', href)
  })
})
