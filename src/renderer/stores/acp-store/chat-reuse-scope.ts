import { ANTIGRAVITY_AGENT_CONFIG_ID } from '@shared/types/antigravity-accounts.types'
import { CODEX_AGENT_CONFIG_ID } from '@shared/types/codex-accounts.types'
import { ownChatAccountId as ownAntigravityChatAccountId } from '@/components/chat/antigravity-account/use-project-antigravity-account'
import { ownChatAccountId } from '@/components/chat/claude-account/use-project-claude-account'
import { ownChatAccountId as ownCodexChatAccountId } from '@/components/chat/codex-account/use-project-codex-account'
import { CLAUDE_AGENT_CONFIG_ID } from '@/lib/agents/registry-config-id'
import { antigravityAccountsApi } from '@/lib/antigravity-accounts-api'
import { claudeAccountsApi } from '@/lib/claude-accounts-api'
import { codexAccountsApi } from '@/lib/codex-accounts-api'
import { logFrontendError } from '@/lib/log-api'
import { reuseScopeFor } from '../acp-reuse-keys'

/**
 * The chat's own Claude Account; `null` when it follows its Project,
 * `undefined` when the host cannot list the Accounts.
 */
export async function readOwnChatAccount(sessionId: string): Promise<string | null | undefined> {
  try {
    return ownChatAccountId(await claudeAccountsApi.list(), sessionId)
  } catch {
    void logFrontendError({
      level: 'warn',
      source: 'acp.reuseScope',
      message: `Claude accounts unreadable for chat ${sessionId}; it gets an agent of its own`
    })
    return undefined
  }
}

/**
 * The chat's own Codex Account; `null` when it follows its Project,
 * `undefined` when the host cannot list the Accounts.
 */
export async function readOwnCodexChatAccount(
  sessionId: string
): Promise<string | null | undefined> {
  try {
    return ownCodexChatAccountId(await codexAccountsApi.list(), sessionId)
  } catch {
    void logFrontendError({
      level: 'warn',
      source: 'acp.reuseScope',
      message: `Codex accounts unreadable for chat ${sessionId}; it gets an agent of its own`
    })
    return undefined
  }
}

/**
 * The chat's own Antigravity Account; `null` when it follows its Project,
 * `undefined` when the host cannot list the Accounts.
 */
export async function readOwnAntigravityChatAccount(
  sessionId: string
): Promise<string | null | undefined> {
  try {
    return ownAntigravityChatAccountId(await antigravityAccountsApi.list(), sessionId)
  } catch {
    void logFrontendError({
      level: 'warn',
      source: 'acp.reuseScope',
      message: `Antigravity accounts unreadable for chat ${sessionId}; it gets an agent of its own`
    })
    return undefined
  }
}

/**
 * The chat id the agent that reopens existing chat `sessionId` is keyed to
 * ({@link reuseScopeFor}). A managed Claude, Codex or Antigravity chat can run its own
 * Account; any other agent reads nothing and shares its Project's agents.
 */
export async function readReuseScope(
  configId: string,
  sessionId: string
): Promise<string | undefined> {
  if (configId === CLAUDE_AGENT_CONFIG_ID) {
    return reuseScopeFor(sessionId, await readOwnChatAccount(sessionId))
  }
  if (configId === CODEX_AGENT_CONFIG_ID) {
    return reuseScopeFor(sessionId, await readOwnCodexChatAccount(sessionId))
  }
  if (configId === ANTIGRAVITY_AGENT_CONFIG_ID) {
    return reuseScopeFor(sessionId, await readOwnAntigravityChatAccount(sessionId))
  }
  return undefined
}
