import type {
  AntigravityAccountsSnapshot,
  AntigravityAccountUsage,
  AntigravityAccountView,
  AntigravityUsageWindow
} from '@shared/types/antigravity-accounts.types'
import { antigravityAccountReadiness } from '@/components/settings/provider/antigravity-account-status'
import {
  type AntigravityAccountsState,
  useAntigravityAccounts
} from '@/hooks/use-antigravity-accounts'
import { primaryAntigravityWindow } from '@/lib/antigravity-usage-format'

export interface AntigravityAccountChoice {
  account: AntigravityAccountView
  usage: AntigravityAccountUsage | undefined
  window: AntigravityUsageWindow | null
}

export interface ProjectAntigravityAccount {
  accounts: AntigravityAccountsState
  projectId: string
  current: AntigravityAccountChoice
  defaultAccount: AntigravityAccountView | null
  /** `true` when the Project chose an Account (not "Follow default"). */
  explicit: boolean
  /** Signed-in Accounts with room left, least used first. */
  alternatives: AntigravityAccountChoice[]
}

const choice = (
  account: AntigravityAccountView,
  usage: Record<string, AntigravityAccountUsage>
): AntigravityAccountChoice => ({
  account,
  usage: usage[account.id],
  window: primaryAntigravityWindow(usage[account.id])
})

/** The Account a Project uses: its own choice, else the Default Account. */
export const projectAntigravityAccountId = (
  snapshot: AntigravityAccountsSnapshot,
  projectId: string
): string => snapshot.projectAccounts[projectId] ?? snapshot.defaultAccountId

/** The chat's own Account, when its entry names an Account the host still has. */
export function ownChatAccountId(
  snapshot: AntigravityAccountsSnapshot,
  sessionId: string
): string | null {
  const id = snapshot.chatAccounts?.[sessionId]
  return id !== undefined && snapshot.accounts.some((account) => account.id === id) ? id : null
}

/**
 * The Account a chat or Project uses, resolved like the host: the chat's own
 * choice, else its Project's, else the Default Account.
 */
export const antigravityAccountFor = (
  snapshot: AntigravityAccountsSnapshot,
  projectId: string,
  sessionId?: string
): string =>
  (sessionId ? ownChatAccountId(snapshot, sessionId) : null) ??
  projectAntigravityAccountId(snapshot, projectId)

/** Signed-in Accounts other than `currentId` with room left, least used first. */
export function rankAntigravityAlternatives(
  snapshot: AntigravityAccountsSnapshot,
  usage: Record<string, AntigravityAccountUsage>,
  currentId: string
): AntigravityAccountChoice[] {
  return snapshot.accounts
    .filter(
      (account) =>
        account.id !== currentId &&
        antigravityAccountReadiness(account, usage[account.id]) === 'ready'
    )
    .map((account) => choice(account, usage))
    .filter((option) => (option.window?.percent ?? 0) < 100)
    .sort((left, right) => (left.window?.percent ?? 0) - (right.window?.percent ?? 0))
}

/**
 * The Antigravity Account context of one Project. `null` when there is no Project
 * or while the list loads.
 */
export function useProjectAntigravityAccount(
  projectId: string | null
): ProjectAntigravityAccount | null {
  const accounts = useAntigravityAccounts({ enabled: Boolean(projectId), usage: true })
  const { snapshot, usage } = accounts
  if (!projectId || !snapshot) return null
  const defaultAccount =
    snapshot.accounts.find((account) => account.id === snapshot.defaultAccountId) ?? null
  const currentId = projectAntigravityAccountId(snapshot, projectId)
  const current = snapshot.accounts.find((account) => account.id === currentId)
  if (!current) return null
  return {
    accounts,
    projectId,
    current: choice(current, usage),
    defaultAccount,
    explicit: Boolean(snapshot.projectAccounts[projectId]),
    alternatives: rankAntigravityAlternatives(snapshot, usage, current.id)
  }
}

export function antigravityChoiceReady(account: AntigravityAccountView): boolean {
  return antigravityAccountReadiness(account) === 'ready'
}
