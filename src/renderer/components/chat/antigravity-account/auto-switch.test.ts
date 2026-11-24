import type { AntigravityAccountUsage } from '@shared/types/antigravity-accounts.types'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { logFrontendError } from '@/lib/log-api'
import { useAcpStore } from '@/stores/acp-store'
import { FRESH } from '@/stores/acp-store/testkit'
import { decideAutoSwitch, startAntigravityAutoSwitch } from './auto-switch'
import { useAntigravityResumeStore } from './use-antigravity-limit'
import type { AntigravityAccountChoice } from './use-project-antigravity-account'
import { antigravityAccountFor } from './use-project-antigravity-account'

const { api, switcher } = vi.hoisted(() => ({
  api: { list: vi.fn(), usage: vi.fn(), setResume: vi.fn() },
  switcher: {
    switchProjectAntigravityAccount: vi.fn(),
    applyProjectAntigravityAccount: vi.fn(),
    isChatOpenHere: vi.fn(),
    restartAntigravitySession: vi.fn()
  }
}))

vi.mock('@/lib/antigravity-accounts-api', () => ({ antigravityAccountsApi: api }))
vi.mock('@/lib/log-api', () => ({ logFrontendError: vi.fn() }))
vi.mock('sonner', () => ({
  toast: { success: vi.fn(), error: vi.fn(), info: vi.fn(), warning: vi.fn() }
}))
vi.mock('@/stores/antigravity-account-switch', async () => {
  const actual = await vi.importActual<typeof import('@/stores/antigravity-account-switch')>(
    '@/stores/antigravity-account-switch'
  )
  return {
    ...actual,
    switchProjectAntigravityAccount: switcher.switchProjectAntigravityAccount,
    applyProjectAntigravityAccount: switcher.applyProjectAntigravityAccount,
    isChatOpenHere: switcher.isChatOpenHere,
    restartAntigravitySession: switcher.restartAntigravitySession
  }
})

const account = (id: string, name: string) => ({
  id,
  name,
  builtin: id === 'default',
  signedIn: true
})

const choice = (id: string, name: string, percent: number): AntigravityAccountChoice => ({
  account: account(id, name),
  usage: undefined,
  window: { kind: 'model', label: 'Gemini 3 Pro', modelId: 'gemini-3-pro', percent, resetsAt: null }
})

const limit = (resetsAt: number | null) => ({
  status: 'rejected' as const,
  resetsAt,
  kind: 'five_hour',
  percent: 100,
  overageInUse: false
})

describe('decideAutoSwitch', () => {
  const work = choice('ab12', 'Work', 4)
  const base = { limit: limit(2_000), autoSwitch: true, limited: new Map(), now: 1_000_000 }

  it('does nothing when automatic switching is off', () => {
    expect(decideAutoSwitch({ ...base, autoSwitch: false, candidates: [work] })).toEqual({
      kind: 'none',
      reason: 'off'
    })
  })

  it('switches to the account with the most room', () => {
    expect(decideAutoSwitch({ ...base, candidates: [work] })).toEqual({ kind: 'switch', to: work })
  })

  it('continues at the reset when no account has room', () => {
    expect(decideAutoSwitch({ ...base, candidates: [] })).toEqual({ kind: 'resume', at: 2_000_000 })
  })
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

const snapshot = (autoSwitch = true) => ({
  accounts: [account('default', 'Personal'), account('ab12', 'Work'), account('cd34', 'Side')],
  defaultAccountId: 'default',
  projectAccounts: {} as Record<string, string>,
  chatAccounts: {} as Record<string, string>,
  autoSwitch
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

function setLimit(resetsAt: number | null): void {
  useAcpStore.setState({
    sessionUsage: {
      s1: {
        used: 1,
        size: 10,
        baselineUsed: 1,
        updatedAt: 1,
        source: 'reported',
        rateLimit: limit(resetsAt)
      }
    }
  } as never)
}

describe('probe stamps', () => {
  afterEach(() => {
    useAcpStore.setState(FRESH as never)
  })

  it('records the chat account limit, not the project account limit', async () => {
    const chat = {
      agentId: 'a1',
      cwd: '/work',
      projectId: 'office',
      status: 'active' as const,
      title: null,
      activeTurn: true,
      openTurnId: 'turn-1',
      modes: null,
      models: null,
      configOptions: [],
      lastError: null,
      createdAt: 1
    }
    useAcpStore.setState({
      ...FRESH,
      sessions: {
        s1: { ...chat, id: 's1' },
        s2: { ...chat, id: 's2', openTurnId: 'turn-2' }
      },
      configToLiveAgent: { 'acp-registry:antigravity-acp\0/work\0project\0office': 'a1' },
      sessionUsage: {}
    } as never)
    const accounts = snapshot()
    accounts.projectAccounts = { office: 'default' }
    accounts.chatAccounts = { s1: 'ab12' }
    expect(antigravityAccountFor(accounts, 'office', 's1')).toBe('ab12')
    api.list.mockResolvedValue(accounts)
    api.usage.mockResolvedValue([read('default', 100), read('ab12', 4), read('cd34', 40)])
    const stop = startAntigravityAutoSwitch()
    await vi.waitFor(() =>
      expect(useAcpStore.getState().sessionUsage.s2?.rateLimit?.status).toBe('rejected')
    )
    expect(useAcpStore.getState().sessionUsage.s1?.rateLimit).toBeUndefined()
    stop()
  })
})

describe('startAntigravityAutoSwitch', () => {
  let stop: () => void

  beforeEach(() => {
    vi.clearAllMocks()
    seed()
    api.list.mockResolvedValue(snapshot())
    api.usage.mockResolvedValue([read('default', 40), read('ab12', 4), read('cd34', 40)])
    switcher.switchProjectAntigravityAccount.mockResolvedValue(['s1'])
    switcher.applyProjectAntigravityAccount.mockResolvedValue(['s1'])
    switcher.isChatOpenHere.mockReturnValue(true)
    switcher.restartAntigravitySession.mockResolvedValue(true)
    stop = startAntigravityAutoSwitch()
  })

  afterEach(() => {
    stop()
    useAntigravityResumeStore.getState().forget('s1')
  })

  it('does not move the project or resend the last prompt; the host continues the chat', async () => {
    setLimit(1_900_000_000)
    await vi.waitFor(() =>
      expect(
        vi
          .mocked(logFrontendError)
          .mock.calls.map(([entry]) => (entry as { message: string }).message)
          .some((message) => message.includes('the host continues them'))
      ).toBe(true)
    )
    expect(switcher.switchProjectAntigravityAccount).not.toHaveBeenCalled()
    expect(switcher.restartAntigravitySession).not.toHaveBeenCalled()
    expect(useAntigravityResumeStore.getState().at.s1).toBeUndefined()
  })
})
