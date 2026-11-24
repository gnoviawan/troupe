/**
 * Antigravity Accounts facade.
 *
 * Every surface lists the Accounts, chooses the Default or a Project Account,
 * stores the automatic-switching flag, and adds, renames and removes an
 * Account through the ACP transport (`/ws`, over the Tauri Channel on desktop).
 *
 * Sign-in spawns the Antigravity agent with that Account's `GEMINI_HOME`, then
 * the caller runs ACP `authenticate` with `oauth-personal` (Google) or
 * `oauth-business` (Gemini Enterprise). The agent opens the Google page through
 * the agent browser-auth flow, so the same path works on the desktop app and on
 * a remote browser.
 */

import type {
  AntigravityAccount,
  AntigravityAccountsSnapshot,
  AntigravityAccountUsage,
  RemovedAntigravityAccount
} from '@shared/types/antigravity-accounts.types'
import type { AgentConfig, SpawnAgentResult } from '@/lib/acp-api'
import { getAcpTransport } from './acp-transport'
import { notifyAntigravityAccountListChanged } from './antigravity-account-list'
import { runCommand } from './command-engine'
import { logFrontendError } from './log-api'

async function logged<T>(action: string, run: () => Promise<T>): Promise<T> {
  try {
    return await run()
  } catch (error) {
    void logFrontendError({
      level: 'warn',
      source: `antigravityAccounts.${action}`,
      message: `Antigravity account ${action} failed: ${error instanceof Error ? error.message : String(error)}`
    })
    throw error
  }
}

export const antigravityAccountsApi = {
  list: (): Promise<AntigravityAccountsSnapshot> =>
    logged('list', () => getAcpTransport().listAntigravityAccounts()),

  /**
   * Per-model quotas of every Account. The host caches them for 5 minutes.
   * `force` still reuses a read younger than 60 seconds. A failed read keeps
   * the last good windows.
   */
  usage: (force = false): Promise<AntigravityAccountUsage[]> =>
    logged('usage', () => getAcpTransport().listAntigravityUsage(force)),

  setDefault: (accountId: string): Promise<void> =>
    logged('setDefault', async () => {
      await getAcpTransport().setDefaultAntigravityAccount(accountId)
      notifyAntigravityAccountListChanged()
    }),

  /**
   * `null` returns the Project to the Default Account. `expectedAccountId`
   * applies the choice only while the Project still uses that Account.
   */
  setProjectAccount: (
    projectId: string,
    accountId: string | null,
    options: { expectedAccountId?: string } = {}
  ): Promise<boolean> =>
    logged('setProjectAccount', async () => {
      const applied = await getAcpTransport().setProjectAntigravityAccount(
        projectId,
        accountId,
        options.expectedAccountId
      )
      if (applied) notifyAntigravityAccountListChanged()
      return applied
    }),

  setChatAccount: (
    projectId: string,
    sessionId: string,
    accountId: string | null,
    options: { expectedAccountId?: string } = {}
  ): Promise<boolean> =>
    logged('setChatAccount', async () => {
      const applied = await getAcpTransport().setChatAntigravityAccount(
        projectId,
        sessionId,
        accountId,
        options.expectedAccountId
      )
      if (applied) notifyAntigravityAccountListChanged()
      return applied
    }),

  setResume: (sessionId: string, at: number | null): Promise<void> =>
    logged('setResume', () => getAcpTransport().setAntigravityResume(sessionId, at)),

  /** A chat at its limit continues on the signed-in Account with the most room. */
  setAutoSwitch: (enabled: boolean): Promise<void> =>
    logged('setAutoSwitch', async () => {
      await getAcpTransport().setAntigravityAutoSwitch(enabled)
      notifyAntigravityAccountListChanged()
    }),

  /** A lost reply is resent with the same command id, so it never adds twice. */
  add: (name: string): Promise<AntigravityAccount> =>
    logged('add', async () => {
      const run = await runCommand({
        kind: 'add_antigravity_account',
        entityKey: name,
        send: (call) => getAcpTransport().addAntigravityAccount(name, call)
      })
      notifyAntigravityAccountListChanged()
      return run.result
    }),

  rename: (accountId: string, name: string): Promise<void> =>
    logged('rename', async () => {
      await getAcpTransport().renameAntigravityAccount(accountId, name)
      notifyAntigravityAccountListChanged()
    }),

  /** Projects that moved to the Default Account, and chats that now follow their Project. */
  remove: (accountId: string): Promise<RemovedAntigravityAccount> =>
    logged('remove', async () => {
      const moved = await getAcpTransport().removeAntigravityAccount(accountId)
      notifyAntigravityAccountListChanged()
      return moved
    }),

  /**
   * Spawn the Antigravity agent with this Account's home. The caller authenticates
   * that agent, then stops it. The login stays in the home.
   */
  startLogin: (accountId: string, config: AgentConfig): Promise<SpawnAgentResult> =>
    logged('startLogin', async () => {
      const run = await runCommand({
        kind: 'start_antigravity_account_login',
        entityKey: accountId,
        send: (call) => getAcpTransport().startAntigravityAccountLogin(accountId, config, call)
      })
      return run.result
    })
}
