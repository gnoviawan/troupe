import type { AntigravityAccountView } from '@shared/types/antigravity-accounts.types'
import { primaryAntigravityWindow } from '@/lib/antigravity-usage-format'
import { useAcpStore } from '@/stores/acp-store'
import { isAntigravitySession } from '@/stores/antigravity-account-switch'
import {
  antigravityAccountFor,
  ownChatAccountId,
  type ProjectAntigravityAccount,
  rankAntigravityAlternatives,
  useProjectAntigravityAccount
} from './use-project-antigravity-account'

export interface ChatAntigravityAccount extends ProjectAntigravityAccount {
  sessionId: string
  /** The chat runs an Account of its own, not its Project's. */
  ownAccount: boolean
  projectAccount: AntigravityAccountView
}

/** The Antigravity Account of one chat; `null` when the chat is not a managed Antigravity chat. */
export function useChatAntigravityAccount(sessionId: string): ChatAntigravityAccount | null {
  const isAntigravity = useAcpStore((s) => isAntigravitySession(s, sessionId))
  const projectId = useAcpStore((s) => s.sessions?.[sessionId]?.projectId ?? '')
  const project = useProjectAntigravityAccount(isAntigravity && projectId ? projectId : null)
  if (!project?.accounts.snapshot) return null
  const snapshot = project.accounts.snapshot
  const currentId = antigravityAccountFor(snapshot, project.projectId, sessionId)
  const current = snapshot.accounts.find((account) => account.id === currentId)
  const projectAccountId = snapshot.projectAccounts[project.projectId] ?? snapshot.defaultAccountId
  const projectAccount =
    snapshot.accounts.find((account) => account.id === projectAccountId) ?? null
  if (!current || !projectAccount) return null
  const usage = project.accounts.usage[current.id]
  return {
    ...project,
    sessionId,
    current: { account: current, usage, window: primaryAntigravityWindow(usage) },
    ownAccount: ownChatAccountId(snapshot, sessionId) !== null,
    projectAccount,
    // The project list is ranked against the project's account. A chat on its
    // own account must see that project's account when it still has room.
    alternatives: rankAntigravityAlternatives(snapshot, project.accounts.usage, current.id)
  }
}
