import type { AntigravityAccountView } from '@shared/types/antigravity-accounts.types'
import type { ClaudeAccountView } from '@shared/types/claude-accounts.types'
import type { CodexAccountView } from '@shared/types/codex-accounts.types'
import { useCallback } from 'react'
import { accountReadiness } from '@/components/settings/provider/account-status'
import { antigravityAccountReadiness } from '@/components/settings/provider/antigravity-account-status'
import { codexAccountReadiness } from '@/components/settings/provider/codex-account-status'
import { useAntigravityAccounts } from '@/hooks/use-antigravity-accounts'
import { useClaudeAccounts } from '@/hooks/use-claude-accounts'
import { useCodexAccounts } from '@/hooks/use-codex-accounts'
import type { AgentSignIn } from '@/lib/agents/agent-setup'

/** A provider whose default-account login is readable without spawning its agent. */
export type SignInProvider =
  | { kind: 'claude'; account: ClaudeAccountView }
  | { kind: 'codex'; account: CodexAccountView }
  | { kind: 'antigravity'; account: AntigravityAccountView }

export interface ProviderSignIn {
  /** The sign-in state for an agent row; `null` for agents not checked here. */
  of: (agentId: string) => AgentSignIn | null
  /** The default account a row's Sign in acts on. */
  providerOf: (agentId: string) => SignInProvider | null
  /** Reads every account list again. */
  reload: () => Promise<void>
}

function defaultAccount<A extends { id: string }>(
  snapshot: { accounts: A[]; defaultAccountId: string } | null
): A | null {
  return snapshot?.accounts.find((a) => a.id === snapshot.defaultAccountId) ?? null
}

/**
 * The Claude (`claude-acp`), Codex (`codex-acp`) and Antigravity
 * (`antigravity-acp`) default accounts as sign-in states. A list that failed or has no default account reads as
 * `null`: unknown is not signed out, so the row stays Ready and the launcher
 * decides at first send.
 */
export function useProviderSignIn(): ProviderSignIn {
  const claude = useClaudeAccounts({ usage: false })
  const codex = useCodexAccounts()
  const antigravity = useAntigravityAccounts()
  const claudeDefault = defaultAccount(claude.snapshot)
  const codexDefault = defaultAccount(codex.snapshot)
  const antigravityDefault = defaultAccount(antigravity.snapshot)

  const providerOf = (agentId: string): SignInProvider | null => {
    if (agentId === 'claude-acp' && claudeDefault) return { kind: 'claude', account: claudeDefault }
    if (agentId === 'codex-acp' && codexDefault) return { kind: 'codex', account: codexDefault }
    if (agentId === 'antigravity-acp' && antigravityDefault) {
      return { kind: 'antigravity', account: antigravityDefault }
    }
    return null
  }

  const of = (agentId: string): AgentSignIn | null => {
    if (agentId === 'claude-acp') {
      if (claude.loading && !claude.snapshot) return 'checking'
      return claudeDefault && !claude.error ? accountReadiness(claudeDefault, undefined) : null
    }
    if (agentId === 'codex-acp') {
      if (codex.loading && !codex.snapshot) return 'checking'
      return codexDefault && !codex.error ? codexAccountReadiness(codexDefault) : null
    }
    if (agentId === 'antigravity-acp') {
      if (antigravity.loading && !antigravity.snapshot) return 'checking'
      return antigravityDefault && !antigravity.error
        ? antigravityAccountReadiness(antigravityDefault)
        : null
    }
    return null
  }

  const reloadClaude = claude.reload
  const reloadCodex = codex.reload
  const reloadAntigravity = antigravity.reload
  const reload = useCallback(async () => {
    await Promise.all([reloadClaude(), reloadCodex(), reloadAntigravity()])
  }, [reloadClaude, reloadCodex, reloadAntigravity])

  return { of, providerOf, reload }
}
