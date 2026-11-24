import type { AcpState } from './types'

/** The agent config of a chat: the live reuse-key map first, then the session index. */
export function sessionAgentConfigId(
  state: Pick<AcpState, 'sessions' | 'configToLiveAgent' | 'sessionIndex'>,
  sessionId: string
): string | null {
  const session = state.sessions?.[sessionId]
  if (session?.agentId) {
    for (const [key, id] of Object.entries(state.configToLiveAgent ?? {})) {
      if (id === session.agentId) return key.split('\0')[0] ?? null
    }
  }
  return state.sessionIndex?.find((entry) => entry.id === sessionId)?.agentConfigId ?? null
}
