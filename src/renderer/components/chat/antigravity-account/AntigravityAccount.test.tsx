import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { useAcpStore } from '@/stores/acp-store'
import { FRESH } from '@/stores/acp-store/testkit'
import { AntigravityAccountChip } from './AntigravityAccountChip'

const { api, switcher, openApp } = vi.hoisted(() => ({
  openApp: vi.fn(),
  api: { list: vi.fn(), usage: vi.fn() },
  switcher: { switchProjectAntigravityAccount: vi.fn(), switchChatAntigravityAccount: vi.fn() }
}))

vi.mock('@/lib/antigravity-accounts-api', () => ({ antigravityAccountsApi: api }))
vi.mock('@/lib/log-api', () => ({ logFrontendError: vi.fn() }))
vi.mock('sonner', () => ({ toast: { success: vi.fn(), error: vi.fn() } }))
vi.mock('@/stores/settings-modal-store', () => ({
  useSettingsModalStore: (select: (s: { openApp: typeof openApp }) => unknown) =>
    select({ openApp })
}))
vi.mock('@/stores/antigravity-account-switch', async () => {
  const actual = await vi.importActual<typeof import('@/stores/antigravity-account-switch')>(
    '@/stores/antigravity-account-switch'
  )
  return {
    ...actual,
    switchProjectAntigravityAccount: switcher.switchProjectAntigravityAccount,
    switchChatAntigravityAccount: switcher.switchChatAntigravityAccount
  }
})

beforeEach(() => {
  vi.clearAllMocks()
  api.usage.mockResolvedValue([])
  api.list.mockResolvedValue({
    accounts: [
      { id: 'default', name: 'Personal', builtin: true, signedIn: true },
      { id: 'ab12', name: 'Work', builtin: false, signedIn: true },
      { id: 'cd34', name: 'Side', builtin: false, signedIn: false }
    ],
    defaultAccountId: 'default',
    projectAccounts: { office: 'ab12' },
    chatAccounts: {},
    autoSwitch: false
  })
  switcher.switchProjectAntigravityAccount.mockResolvedValue(true)
  switcher.switchChatAntigravityAccount.mockResolvedValue('restarted')
  useAcpStore.setState({
    ...FRESH,
    sessions: {
      s1: {
        id: 's1',
        agentId: 'a1',
        cwd: '/work',
        projectId: 'office',
        status: 'active',
        title: null,
        activeTurn: false,
        openTurnId: null,
        modes: null,
        models: null,
        configOptions: [],
        lastError: null,
        createdAt: 1
      }
    },
    configToLiveAgent: { 'acp-registry:antigravity-acp\0/work\0project\0office': 'a1' }
  } as never)
})

describe('AntigravityAccountChip', () => {
  it('shows the account for this chat and does not switch the project', async () => {
    render(<AntigravityAccountChip sessionId="s1" />)
    const chip = await screen.findByTestId('antigravity-account-chip')
    expect(chip).toHaveTextContent('Work')
    fireEvent.pointerDown(chip, { button: 0, ctrlKey: false })
    expect(await screen.findByRole('menuitem', { name: /Follow project/ })).toBeInTheDocument()
    fireEvent.click(await screen.findByRole('menuitem', { name: /^Personal/ }))
    await waitFor(() =>
      expect(switcher.switchChatAntigravityAccount).toHaveBeenCalledWith(
        expect.objectContaining({ sessionId: 's1', projectId: 'office', runsOn: 'ab12' }),
        'default'
      )
    )
    expect(switcher.switchProjectAntigravityAccount).not.toHaveBeenCalled()
  })

  it('does nothing when the chat already follows the project', async () => {
    render(<AntigravityAccountChip sessionId="s1" />)
    fireEvent.pointerDown(await screen.findByTestId('antigravity-account-chip'), {
      button: 0,
      ctrlKey: false
    })
    fireEvent.click(await screen.findByRole('menuitem', { name: /Follow project/ }))
    expect(switcher.switchChatAntigravityAccount).not.toHaveBeenCalled()
  })

  it('disables Follow project from the project account, not the default', async () => {
    api.list.mockResolvedValue({
      accounts: [
        { id: 'default', name: 'Personal', builtin: true, signedIn: true },
        { id: 'ab12', name: 'Work', builtin: false, signedIn: false }
      ],
      defaultAccountId: 'default',
      projectAccounts: { office: 'ab12' },
      chatAccounts: { s1: 'default' },
      autoSwitch: false
    })
    render(<AntigravityAccountChip sessionId="s1" />)
    fireEvent.pointerDown(await screen.findByTestId('antigravity-account-chip'), {
      button: 0,
      ctrlKey: false
    })
    const follow = await screen.findByRole('menuitem', { name: /Follow project/ })
    expect(follow).toHaveTextContent('Uses Work')
    expect(follow).toHaveTextContent(/signed out/i)
    expect(follow).toHaveAttribute('data-disabled')
  })

  it('does not offer a signed-out account', async () => {
    render(<AntigravityAccountChip sessionId="s1" />)
    fireEvent.pointerDown(await screen.findByTestId('antigravity-account-chip'), {
      button: 0,
      ctrlKey: false
    })
    const side = await screen.findByRole('menuitem', { name: /Side/ })
    expect(side).toHaveAttribute('data-disabled')
  })

  it('renders nothing for a chat that is not Antigravity', () => {
    useAcpStore.setState({
      configToLiveAgent: { 'acp-registry:claude-acp\0/work\0project\0office': 'a1' }
    } as never)
    const { container } = render(<AntigravityAccountChip sessionId="s1" />)
    expect(container).toBeEmptyDOMElement()
  })
})
