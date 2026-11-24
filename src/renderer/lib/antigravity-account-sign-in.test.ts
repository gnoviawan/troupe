import { beforeEach, describe, expect, it, vi } from 'vitest'
import { useAcpStore } from '@/stores/acp-store'
import { FRESH } from '@/stores/acp-store/testkit'
import { signInAntigravityAccount } from './antigravity-account-sign-in'

const { api, authenticateAgent, killAgent } = vi.hoisted(() => ({
  api: { startLogin: vi.fn() },
  authenticateAgent: vi.fn(),
  killAgent: vi.fn()
}))

vi.mock('@/lib/antigravity-accounts-api', () => ({ antigravityAccountsApi: api }))
vi.mock('@/lib/log-api', () => ({ logFrontendError: vi.fn() }))

beforeEach(() => {
  vi.clearAllMocks()
  authenticateAgent.mockResolvedValue(true)
  killAgent.mockResolvedValue(undefined)
  api.startLogin.mockResolvedValue({
    agentId: 'agent-sign-in',
    capabilities: {},
    authMethods: [
      { id: 'oauth-personal', name: 'Log in with Google' },
      { id: 'oauth-business', name: 'Log in with Gemini Enterprise' }
    ]
  })
  useAcpStore.setState({
    ...FRESH,
    agentConfigs: [
      {
        id: 'acp-registry:antigravity-acp',
        name: 'Antigravity',
        command: 'agy_acp_server',
        args: [],
        env: { GEMINI_API_KEY: 'test-key' }
      }
    ],
    authenticateAgent,
    killAgent
  } as never)
})

describe('signInAntigravityAccount', () => {
  it('spawns with the Antigravity config, authenticates with Google, then stops the agent', async () => {
    await signInAntigravityAccount('ab12', 'oauth-personal')
    expect(api.startLogin).toHaveBeenCalledWith(
      'ab12',
      expect.objectContaining({
        configId: 'acp-registry:antigravity-acp',
        command: 'agy_acp_server'
      })
    )
    expect(authenticateAgent).toHaveBeenCalledWith('agent-sign-in', 'oauth-personal')
    expect(killAgent).toHaveBeenCalledWith('agent-sign-in')
  })

  it('refuses the API-key and Agent Platform methods and does not spawn', async () => {
    await expect(signInAntigravityAccount('ab12', 'gemini-api-key')).rejects.toThrow(/Google only/)
    await expect(signInAntigravityAccount('ab12', 'agent-platform')).rejects.toThrow(/Google only/)
    expect(api.startLogin).not.toHaveBeenCalled()
  })

  it('refuses when the Antigravity agent is not set up', async () => {
    useAcpStore.setState({ agentConfigs: [] } as never)
    await expect(signInAntigravityAccount('ab12', 'oauth-personal')).rejects.toThrow(/not set up/)
    expect(api.startLogin).not.toHaveBeenCalled()
  })

  it('stops the agent when authenticate fails', async () => {
    authenticateAgent.mockRejectedValue(new Error('cancelled'))
    await expect(signInAntigravityAccount('ab12', 'oauth-business')).rejects.toThrow('cancelled')
    expect(killAgent).toHaveBeenCalledWith('agent-sign-in')
  })
})
