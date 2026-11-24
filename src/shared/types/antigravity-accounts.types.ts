/**
 * Antigravity Accounts contract. Mirrors the Rust `acp::antigravity_accounts`
 * serde shapes. No field holds a secret: Google logins stay in each Account's
 * own `GEMINI_HOME`.
 */

/** Id of the built-in Account that uses the host's shared `GEMINI_HOME`. */
export const BUILTIN_ANTIGRAVITY_ACCOUNT_ID = 'default'

/** Catalog id of the managed Antigravity ACP agent. */
export const ANTIGRAVITY_AGENT_CONFIG_ID = 'acp-registry:antigravity-acp'

/**
 * ACP authenticate method ids. Google logins only; never `gemini-api-key` or
 * `agent-platform`.
 */
export const ANTIGRAVITY_GOOGLE_METHOD = 'oauth-personal'
export const ANTIGRAVITY_BUSINESS_METHOD = 'oauth-business'

/** One Google login for Antigravity. */
export interface AntigravityAccount {
  id: string
  name: string
  /** Unix epoch milliseconds; `0` for the built-in Account. */
  createdAt: number
}

export interface AntigravityAccountView {
  id: string
  name: string
  /** `true` for the Account that uses the host's shared `GEMINI_HOME`. */
  builtin: boolean
  /** Google login state of the home; `null` when the host could not read it. */
  signedIn: boolean | null
}

export interface AntigravityAccountsSnapshot {
  accounts: AntigravityAccountView[]
  defaultAccountId: string
  /** Project id → Account id, only for Projects with an explicit choice. */
  projectAccounts: Record<string, string>
  /** Session id → Account id, only for chats with their own choice. */
  chatAccounts: Record<string, string>
  /** A chat at its Account's limit continues on the signed-in Account with the most room. */
  autoSwitch: boolean
}

export interface AntigravityAccountRef {
  id: string
  name: string
}

/**
 * What the host did for one Antigravity chat at its Account's limit. Mirrors
 * `acp:antigravity_limit_action`.
 */
export type AntigravityLimitActionEvent = { sessionId: string; projectId: string } & (
  | {
      action: 'switched'
      from: AntigravityAccountRef
      to: AntigravityAccountRef
      fromAgentId: string
      newAgentId: string
    }
  /** `at`: epoch milliseconds. */
  | { action: 'resume_scheduled'; at: number }
  | { action: 'resume_cancelled' }
  | { action: 'resumed'; fromAgentId: string | null; newAgentId: string }
  | { action: 'failed'; reason: string }
)

/** What removing an Antigravity Account moved. */
export interface RemovedAntigravityAccount {
  /** Projects that used the Account and now follow the Default Account. */
  movedProjectIds: string[]
  /** Chats that had the Account as their own and now follow their Project. */
  movedChatIds: string[]
}

/** The quota of one model on an Account. */
export interface AntigravityUsageWindow {
  kind: 'model'
  /** Model display name, e.g. "Gemini 3 Pro". */
  label: string
  modelId: string
  /** Percent used, 0–100. */
  percent: number
  /** ISO-8601 reset time; `null` when Antigravity did not send one. */
  resetsAt: string | null
}

export type AntigravityUsageStatus = 'ok' | 'unavailable' | 'signed_out' | 'error'

/** The usage of one Account at `checkedAt`. */
export interface AntigravityAccountUsage {
  accountId: string
  status: AntigravityUsageStatus
  /** Plan / tier name when Antigravity sent one. Never an email. */
  planType: string | null
  /** One window per model with a quota, most used first. */
  windows: AntigravityUsageWindow[]
  /** Unix epoch milliseconds of the read. */
  checkedAt: number
  /** Reason for `error` or `unavailable`. Never holds login details. */
  message: string | null
}

/**
 * `data` of the `agent_error` that ends an Antigravity turn at an exhausted
 * quota. `resetsAt` is epoch milliseconds when the agent named a reset time.
 */
export interface AntigravityPlanLimitData {
  errorKind: 'quota_exhausted'
  agent: typeof ANTIGRAVITY_ERROR_AGENT
  resetsAt: number | null
}

/** The `agent` tag of an Antigravity plan-limit error. */
export const ANTIGRAVITY_ERROR_AGENT = 'antigravity'
