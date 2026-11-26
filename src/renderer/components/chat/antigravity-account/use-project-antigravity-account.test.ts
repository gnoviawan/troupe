import type { AntigravityAccountsSnapshot } from '@shared/types/antigravity-accounts.types'
import { describe, expect, it } from 'vitest'
import { antigravityAccountFor, ownChatAccountId } from './use-project-antigravity-account'

const snapshot = {
  accounts: [
    { id: 'default', name: 'Personal', builtin: true, signedIn: true },
    { id: 'work', name: 'Work', builtin: false, signedIn: true },
    { id: 'spare', name: 'Spare', builtin: false, signedIn: true }
  ],
  defaultAccountId: 'default',
  projectAccounts: { office: 'work' },
  chatAccounts: { s1: 'spare', s2: 'gone' },
  autoSwitch: true
} satisfies AntigravityAccountsSnapshot

describe('antigravity account for a chat', () => {
  it('uses the chat choice, else the project, else the default', () => {
    expect(antigravityAccountFor(snapshot, 'office', 's1')).toBe('spare')
    expect(antigravityAccountFor(snapshot, 'office', 's3')).toBe('work')
    expect(antigravityAccountFor(snapshot, 'side')).toBe('default')
    expect(ownChatAccountId(snapshot, 's2')).toBeNull()
    expect(ownChatAccountId(snapshot, 's1')).toBe('spare')
  })
})
