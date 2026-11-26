import type { AntigravityAccountUsage } from '@shared/types/antigravity-accounts.types'
import { renderHook } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { useAcpStore } from '@/stores/acp-store'
import { FRESH } from '@/stores/acp-store/testkit'
import { useAntigravityLimitProbe } from './limit-probe'

const { api } = vi.hoisted(() => ({
  api: { list: vi.fn(), usage: vi.fn(), setResume: vi.fn() }
}))

vi.mock('@/lib/antigravity-accounts-api', () => ({ antigravityAccountsApi: api }))
vi.mock('@/lib/log-api', () => ({ logFrontendError: vi.fn() }))

const exhausted: AntigravityAccountUsage = {
  accountId: 'default',
  status: 'ok',
  planType: 'Google AI Pro',
  windows: [
    { kind: 'model', label: 'Gemini 3 Pro', modelId: 'gemini-3-pro', percent: 100, resetsAt: null }
  ],
  checkedAt: 1,
  message: null
}

describe('useAntigravityLimitProbe', () => {
  afterEach(() => {
    useAcpStore.setState(FRESH as never)
  })

  it('never reads a usage check, so a held chat is not marked at its limit', async () => {
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
          activeTurn: true,
          openTurnId: 'turn-1',
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
    api.list.mockResolvedValue({
      accounts: [{ id: 'default', name: 'Personal', builtin: true, signedIn: true }],
      defaultAccountId: 'default',
      projectAccounts: {},
      chatAccounts: {},
      autoSwitch: true
    })
    api.usage.mockResolvedValue([exhausted])

    const { unmount } = renderHook(() => useAntigravityLimitProbe())
    await new Promise((resolve) => setTimeout(resolve, 0))

    expect(api.usage).not.toHaveBeenCalled()
    expect(useAcpStore.getState().sessionUsage.s1).toBeUndefined()
    unmount()
  })
})
