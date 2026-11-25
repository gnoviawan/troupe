import { beforeEach, describe, expect, it, vi } from 'vitest'
import { antigravityAccountsApi } from '@/lib/antigravity-accounts-api'

const transport = vi.hoisted(() => ({
  listAntigravityAccounts: vi.fn(),
  listAntigravityUsage: vi.fn(),
  setDefaultAntigravityAccount: vi.fn(),
  setProjectAntigravityAccount: vi.fn(),
  setAntigravityAutoSwitch: vi.fn(),
  addAntigravityAccount: vi.fn(),
  renameAntigravityAccount: vi.fn(),
  removeAntigravityAccount: vi.fn(),
  startAntigravityAccountLogin: vi.fn(),
  checkAntigravityUsage: vi.fn()
}))

vi.mock('@/lib/acp-transport', () => ({ getAcpTransport: () => transport }))
vi.mock('@/lib/log-api', () => ({ logFrontendError: vi.fn() }))
const runCommand = vi.hoisted(() =>
  vi.fn(async ({ send }: { send: (call: { commandId: string }) => Promise<unknown> }) => ({
    result: await send({ commandId: 'cmd-1' })
  }))
)
vi.mock('@/lib/command-engine', () => ({ runCommand }))

const SNAPSHOT = {
  accounts: [{ id: 'default', name: 'Default', builtin: true, signedIn: true }],
  defaultAccountId: 'default',
  projectAccounts: {},
  chatAccounts: {},
  autoSwitch: true
}

beforeEach(() => {
  vi.clearAllMocks()
  transport.listAntigravityAccounts.mockResolvedValue(SNAPSHOT)
  transport.setProjectAntigravityAccount.mockResolvedValue(true)
  transport.addAntigravityAccount.mockResolvedValue({ id: 'ab12', name: 'Work', createdAt: 1 })
  transport.removeAntigravityAccount.mockResolvedValue({
    movedProjectIds: ['office'],
    movedChatIds: ['s1']
  })
  transport.startAntigravityAccountLogin.mockResolvedValue({
    agentId: 'a1',
    capabilities: {},
    authMethods: []
  })
})

describe('antigravityAccountsApi', () => {
  it('lists, chooses, and stores the auto-switch flag over the transport', async () => {
    await expect(antigravityAccountsApi.list()).resolves.toEqual(SNAPSHOT)
    await antigravityAccountsApi.setDefault('ab12')
    await expect(
      antigravityAccountsApi.setProjectAccount('office', null, { expectedAccountId: 'ab12' })
    ).resolves.toBe(true)
    await antigravityAccountsApi.setAutoSwitch(false)
    expect(transport.setDefaultAntigravityAccount).toHaveBeenCalledWith('ab12')
    expect(transport.setProjectAntigravityAccount).toHaveBeenCalledWith('office', null, 'ab12')
    expect(transport.setAntigravityAutoSwitch).toHaveBeenCalledWith(false)
    transport.listAntigravityUsage.mockResolvedValue([])
    await antigravityAccountsApi.usage(true)
    expect(transport.listAntigravityUsage).toHaveBeenCalledWith(true)
  })

  it('adds, renames, removes, and starts a sign-in spawn', async () => {
    await expect(antigravityAccountsApi.add('Work')).resolves.toMatchObject({ id: 'ab12' })
    await antigravityAccountsApi.rename('ab12', 'Office')
    await expect(antigravityAccountsApi.remove('ab12')).resolves.toEqual({
      movedProjectIds: ['office'],
      movedChatIds: ['s1']
    })
    const config = { name: 'Antigravity', command: 'npx', args: [], env: {} }
    await expect(antigravityAccountsApi.startLogin('ab12', config)).resolves.toMatchObject({
      agentId: 'a1'
    })
    expect(transport.startAntigravityAccountLogin).toHaveBeenCalledWith('ab12', config, {
      commandId: 'cmd-1'
    })
  })

  it('runs each usage check as a fresh command and returns the agent result', async () => {
    const read = {
      accountId: 'ab12',
      status: 'unavailable',
      planType: 'Google AI Pro',
      windows: [],
      checkedAt: 1_700_000_000_000,
      message: 'data not available yet'
    }
    transport.checkAntigravityUsage.mockResolvedValue(read)
    const config = { name: 'Antigravity', command: 'agy', args: [], env: {} }

    await expect(antigravityAccountsApi.checkUsage('ab12', config)).resolves.toEqual(read)

    expect(transport.checkAntigravityUsage).toHaveBeenCalledWith('ab12', config, {
      commandId: 'cmd-1'
    })
    expect(runCommand).toHaveBeenCalledWith(
      expect.objectContaining({ kind: 'check_antigravity_usage', entityKey: 'ab12', fresh: true })
    )
    expect(transport.listAntigravityUsage).not.toHaveBeenCalled()
  })
})
