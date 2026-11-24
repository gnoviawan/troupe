import type {
  AntigravityAccountUsage,
  AntigravityAccountView
} from '@shared/types/antigravity-accounts.types'
import type { AccountTone } from './account-status'

/** Whether Antigravity chats can start on an Account: only a confirmed login passes. */
export type AntigravityAccountReadiness = 'ready' | 'signed-out' | 'unconfirmed'

export function antigravityAccountReadiness(
  account: AntigravityAccountView,
  usage?: Pick<AntigravityAccountUsage, 'status'>
): AntigravityAccountReadiness {
  if (account.signedIn === false || usage?.status === 'signed_out') return 'signed-out'
  return account.signedIn === true ? 'ready' : 'unconfirmed'
}

export const ANTIGRAVITY_NOT_READY_REASON: Record<
  Exclude<AntigravityAccountReadiness, 'ready'>,
  string
> = {
  'signed-out': 'Signed out',
  unconfirmed: 'Could not check sign-in'
}

/** The status dot of an Antigravity account row, the same tones as a Claude row. */
export const ANTIGRAVITY_STATUS_TONE: Record<AntigravityAccountReadiness, AccountTone> = {
  ready: 'ok',
  'signed-out': 'signed-out',
  unconfirmed: 'unknown'
}
