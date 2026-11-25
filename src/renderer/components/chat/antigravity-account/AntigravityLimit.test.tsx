import type { AntigravityAccountUsage } from '@shared/types/antigravity-accounts.types'
import { fireEvent, render, screen } from '@testing-library/react'
import { toast } from 'sonner'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { useAcpStore } from '@/stores/acp-store'
import { FRESH } from '@/stores/acp-store/testkit'
import { AntigravityLimitNotice } from './AntigravityLimitNotice'
import { AntigravityLimitWarning } from './AntigravityLimitWarning'
import { useAntigravityResumeStore } from './use-antigravity-limit'

const { api, switcher, openApp, admits } = vi.hoisted(() => ({
  admits: { value: true },
  openApp: vi.fn(),
  api: { list: vi.fn(), usage: vi.fn(), setResume: vi.fn() },
  switcher: { switchProjectAntigravityAccount: vi.fn(), switchChatAntigravityAccount: vi.fn() }
}))

vi.mock('@/lib/antigravity-accounts-api', () => ({ antigravityAccountsApi: api }))
vi.mock('@/lib/log-api', () => ({ logFrontendError: vi.fn() }))
vi.mock('sonner', () => ({ toast: { success: vi.fn(), error: vi.fn(), info: vi.fn() } }))
vi.mock('@/stores/settings-modal-store', () => ({
  useSettingsModalStore: (select: (state: { openApp: typeof openApp }) => unknown) =>
    select({ openApp })
}))
vi.mock('@/hooks/use-server-admits-remote-writes', () => ({
  useServerAdmitsRemoteWrites: () => admits.value
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

const read = (
  accountId: string,
  percent: number,
  resetsAt: string | null = null
): AntigravityAccountUsage => ({
  accountId,
  status: 'ok',
  planType: 'Google AI Pro',
  windows: [{ kind: 'model', label: 'Gemini 3 Pro', modelId: 'gemini-3-pro', percent, resetsAt }],
  checkedAt: 1,
  message: null
})

function seed(): void {
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
    configToLiveAgent: { 'acp-registry:antigravity-acp\0/work\0project\0office': 'a1' },
    sessionUsage: {}
  } as never)
}

beforeEach(() => {
  vi.clearAllMocks()
  admits.value = true
  seed()
  api.list.mockResolvedValue({
    accounts: [
      { id: 'default', name: 'Personal', builtin: true, signedIn: true },
      { id: 'ab12', name: 'Work', builtin: false, signedIn: true },
      { id: 'cd34', name: 'Side', builtin: false, signedIn: true }
    ],
    defaultAccountId: 'default',
    projectAccounts: {},
    chatAccounts: {},
    autoSwitch: true
  })
  switcher.switchProjectAntigravityAccount.mockResolvedValue(true)
  switcher.switchChatAntigravityAccount.mockResolvedValue('restarted')
  api.setResume.mockResolvedValue(undefined)
  useAntigravityResumeStore.setState({ at: {}, turns: {}, timers: {} })
})

function reachLimit(resetsAt: number | null = null): void {
  useAcpStore.getState()._onAgentError({
    agentId: 'a1',
    sessionId: 's1',
    message: 'Antigravity reached its usage limit',
    code: -32603,
    data: { errorKind: 'quota_exhausted', agent: 'antigravity', resetsAt }
  })
}

function onlyPersonalSignedIn(): void {
  api.list.mockResolvedValue({
    accounts: [
      { id: 'default', name: 'Personal', builtin: true, signedIn: true },
      { id: 'ab12', name: 'Work', builtin: false, signedIn: false },
      { id: 'cd34', name: 'Side', builtin: false, signedIn: false }
    ],
    defaultAccountId: 'default',
    projectAccounts: {},
    chatAccounts: {},
    autoSwitch: true
  })
}

describe('AntigravityLimitWarning', () => {
  it('raises no warning from a usage check, even at 82%', async () => {
    api.usage.mockResolvedValue([read('default', 82), read('ab12', 10), read('cd34', 40)])
    const { container } = render(<AntigravityLimitWarning sessionId="s1" />)
    await vi.waitFor(() => expect(api.usage).toHaveBeenCalled())
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(container).toBeEmptyDOMElement()
  })
})

describe('AntigravityLimitNotice', () => {
  it('shows no limit card when only a usage check reads 100%', async () => {
    api.usage.mockResolvedValue([read('default', 100), read('ab12', 4), read('cd34', 40)])
    const { container } = render(<AntigravityLimitNotice sessionId="s1" />)
    await vi.waitFor(() => expect(api.usage).toHaveBeenCalled())
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(container).toBeEmptyDOMElement()
  })

  it('offers the next signed-in account in list order, whatever its last check said', async () => {
    api.usage.mockResolvedValue([read('default', 40), read('ab12', 90), read('cd34', 4)])
    reachLimit()
    render(<AntigravityLimitNotice sessionId="s1" />)
    const alert = await screen.findByRole('alert')
    expect(alert).toHaveTextContent('Personal reached its usage limit')
    const switchButton = await screen.findByRole('button', { name: 'Switch to Work and continue' })
    expect(alert).toHaveTextContent('Switch to Work and continue?')
    expect(alert).not.toHaveTextContent('% used')
    fireEvent.pointerDown(screen.getByRole('button', { name: 'Other account' }), {
      button: 0,
      ctrlKey: false
    })
    expect((await screen.findByRole('menuitem', { name: /Side/ })).textContent).toBe('Side')
    fireEvent.keyDown(screen.getByRole('menu'), { key: 'Escape' })
    fireEvent.click(switchButton)
    await vi.waitFor(() =>
      expect(switcher.switchChatAntigravityAccount).toHaveBeenCalledWith(
        expect.objectContaining({ sessionId: 's1', runsOn: 'default' }),
        'ab12',
        expect.objectContaining({ notice: expect.stringContaining('Antigravity account Personal') })
      )
    )
  })

  it('reports a lost compare-and-set instead of a successful switch', async () => {
    switcher.switchChatAntigravityAccount.mockResolvedValue('lost')
    api.usage.mockResolvedValue([])
    reachLimit()
    render(<AntigravityLimitNotice sessionId="s1" />)
    fireEvent.click(await screen.findByRole('button', { name: 'Switch to Work and continue' }))
    await vi.waitFor(() =>
      expect(toast.info).toHaveBeenCalledWith(
        'The account of this chat already changed',
        expect.objectContaining({ description: expect.any(String) })
      )
    )
    expect(toast.success).not.toHaveBeenCalled()
  })

  it('keeps Cancel visible when a resume is scheduled and another account is signed in', async () => {
    api.usage.mockResolvedValue([])
    reachLimit()
    useAntigravityResumeStore.getState().schedule('s1', Date.now() + 60_000)
    render(<AntigravityLimitNotice sessionId="s1" />)
    expect(await screen.findByRole('button', { name: 'Cancel' })).toBeInTheDocument()
    expect(
      await screen.findByRole('button', { name: 'Switch to Work and continue' })
    ).toBeInTheDocument()
    useAntigravityResumeStore.getState().cancel('s1')
  })

  it('offers the project account when this chat is pinned to another one', async () => {
    api.list.mockResolvedValue({
      accounts: [
        { id: 'default', name: 'Personal', builtin: true, signedIn: true },
        { id: 'ab12', name: 'Work', builtin: false, signedIn: true },
        { id: 'cd34', name: 'Side', builtin: false, signedIn: true }
      ],
      defaultAccountId: 'default',
      projectAccounts: { office: 'ab12' },
      chatAccounts: { s1: 'default' },
      autoSwitch: true
    })
    api.usage.mockResolvedValue([])
    reachLimit()
    render(<AntigravityLimitNotice sessionId="s1" />)
    expect(
      await screen.findByRole('button', { name: 'Switch to Work and continue' })
    ).toBeInTheDocument()
  })

  it('tells the host to drop a scheduled continue before switching', async () => {
    api.usage.mockResolvedValue([])
    reachLimit()
    useAntigravityResumeStore.getState().schedule('s1', Date.now() + 60_000)
    render(<AntigravityLimitNotice sessionId="s1" />)
    fireEvent.click(await screen.findByRole('button', { name: 'Switch to Work and continue' }))
    await vi.waitFor(() => expect(api.setResume).toHaveBeenCalledWith('s1', null))
    expect(switcher.switchChatAntigravityAccount).toHaveBeenCalled()
  })

  it('drops a resume the host refused and keeps the wait button', async () => {
    onlyPersonalSignedIn()
    api.setResume.mockRejectedValue(new Error('offline'))
    api.usage.mockResolvedValue([])
    reachLimit(Date.now() + 3_600_000)
    render(<AntigravityLimitNotice sessionId="s1" />)
    fireEvent.click(await screen.findByRole('button', { name: 'Continue when it resets' }))
    await vi.waitFor(() =>
      expect(toast.error).toHaveBeenCalledWith('Could not schedule the continue: offline')
    )
    expect(screen.getByRole('button', { name: 'Continue when it resets' })).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Cancel' })).not.toBeInTheDocument()
  })

  it('keeps the scheduled continue when cancelling it fails', async () => {
    onlyPersonalSignedIn()
    api.usage.mockResolvedValue([])
    reachLimit(Date.now() + 3_600_000)
    render(<AntigravityLimitNotice sessionId="s1" />)
    fireEvent.click(await screen.findByRole('button', { name: 'Continue when it resets' }))
    expect(await screen.findByRole('button', { name: 'Cancel' })).toBeInTheDocument()
    api.setResume.mockRejectedValue(new Error('offline'))
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }))
    await vi.waitFor(() =>
      expect(toast.error).toHaveBeenCalledWith(
        'Could not cancel the continue',
        expect.objectContaining({
          description: 'offline',
          action: expect.objectContaining({ label: 'Retry' })
        })
      )
    )
    expect(screen.getByRole('button', { name: 'Cancel' })).toBeInTheDocument()
  })

  it('offers wait for reset and Add account when no other account is signed in', async () => {
    onlyPersonalSignedIn()
    api.usage.mockResolvedValue([])
    reachLimit(Date.now() + 3_600_000)
    render(<AntigravityLimitNotice sessionId="s1" />)
    expect(
      await screen.findByRole('button', { name: 'Continue when it resets' })
    ).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Add account' })).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Add account' }))
    expect(openApp).toHaveBeenCalledWith('provider')
  })

  it('never takes a reset time from a usage check', async () => {
    onlyPersonalSignedIn()
    const checkedReset = new Date(Date.now() + 3_600_000).toISOString()
    api.usage.mockResolvedValue([read('default', 100, checkedReset)])
    reachLimit()
    render(<AntigravityLimitNotice sessionId="s1" />)
    expect(await screen.findByRole('alert')).toHaveTextContent('Personal reached its usage limit')
    await vi.waitFor(() => expect(api.usage).toHaveBeenCalled())
    expect(screen.getByRole('alert')).toHaveTextContent('It resets soon.')
    expect(
      screen.queryByRole('button', { name: 'Continue when it resets' })
    ).not.toBeInTheDocument()
  })

  it('continues at the reset time the quota error names', async () => {
    onlyPersonalSignedIn()
    const errorReset = Date.now() + 7_200_000
    api.usage.mockResolvedValue([])
    reachLimit(errorReset)
    expect(useAcpStore.getState().sessionUsage.s1?.rateLimit?.resetsAt).toBe(
      Math.floor(errorReset / 1000)
    )
    render(<AntigravityLimitNotice sessionId="s1" />)
    fireEvent.click(await screen.findByRole('button', { name: 'Continue when it resets' }))
    await vi.waitFor(() =>
      expect(api.setResume).toHaveBeenCalledWith('s1', Math.floor(errorReset / 1000) * 1000)
    )
  })

  it('ignores a Codex-shaped plan-limit error on an Antigravity chat', () => {
    useAcpStore.getState()._onAgentError({
      agentId: 'a1',
      sessionId: 's1',
      message: 'usage limit',
      code: -32603,
      data: { errorKind: 'rate_limited' }
    })
    expect(useAcpStore.getState().sessionUsage.s1?.rateLimit).toBeUndefined()
  })
})
