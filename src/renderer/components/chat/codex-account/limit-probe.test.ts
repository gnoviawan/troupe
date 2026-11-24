import type { CodexAccountUsage } from '@shared/types/codex-accounts.types'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { useAcpStore } from '@/stores/acp-store'
import { FRESH } from '@/stores/acp-store/testkit'
import { startCodexLimitProbe } from './limit-probe'
import { codexAccountFor } from './use-project-codex-account'

const { api } = vi.hoisted(() => ({
  api: { list: vi.fn(), usage: vi.fn(), setResume: vi.fn() }
}))

vi.mock('@/lib/codex-accounts-api', () => ({ codexAccountsApi: api }))
vi.mock('@/lib/log-api', () => ({ logFrontendError: vi.fn() }))

const account = (id: string, name: string) => ({
  id,
  name,
  builtin: id === 'default',
  signedIn: true
})

const read = (accountId: string, percent: number): CodexAccountUsage => ({
  accountId,
  status: 'ok',
  planType: 'plus',
  windows: [{ kind: 'session', percent, resetsAt: null, windowDurationMins: 300 }],
  checkedAt: 1,
  message: null
})

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
      configToLiveAgent: { 'acp-registry:codex-acp\0/work\0project\0office': 'a1' },
      sessionUsage: {}
    } as never)
    const accounts = {
      accounts: [account('default', 'Personal'), account('ab12', 'Work'), account('cd34', 'Side')],
      defaultAccountId: 'default',
      projectAccounts: { office: 'default' } as Record<string, string>,
      chatAccounts: { s1: 'ab12' } as Record<string, string>,
      autoSwitch: true
    }
    expect(codexAccountFor(accounts, 'office', 's1')).toBe('ab12')
    api.list.mockResolvedValue(accounts)
    api.usage.mockResolvedValue([read('default', 100), read('ab12', 4), read('cd34', 40)])
    const stop = startCodexLimitProbe()
    await vi.waitFor(() =>
      expect(useAcpStore.getState().sessionUsage.s2?.rateLimit?.status).toBe('rejected')
    )
    expect(useAcpStore.getState().sessionUsage.s1?.rateLimit).toBeUndefined()
    stop()
  })
})
