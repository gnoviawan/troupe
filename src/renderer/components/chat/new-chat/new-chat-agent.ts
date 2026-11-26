import { rememberLastSelectedAgent } from '@/lib/agents/last-selected-agent'
import { useAcpStore } from '@/stores/acp-store'

/** Remember the agent a new chat started with, like a launcher pick. */
export function rememberNewChatAgent(configId: string): void {
  rememberLastSelectedAgent(configId)
  useAcpStore.getState().setSelectedAgentConfigId(configId)
}
