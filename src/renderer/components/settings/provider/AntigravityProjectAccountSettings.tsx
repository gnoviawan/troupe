import type { AntigravityAccountView } from '@shared/types/antigravity-accounts.types'
import { useState } from 'react'
import { ExternalLink } from '@/components/icons'
import { Button } from '@/components/ui/button'
import type { ProjectAntigravityAccountDraft } from '@/hooks/use-project-antigravity-account-draft'
import { useServerAdmitsRemoteWrites } from '@/hooks/use-server-admits-remote-writes'
import { AccountOptionRow } from './AccountOptionRow'
import { AntigravitySignInDialog } from './AntigravitySignInDialog'
import {
  ANTIGRAVITY_NOT_READY_REASON,
  antigravityAccountReadiness
} from './antigravity-account-status'
import { ProviderGroup } from './ProviderGroupHeader'

interface AntigravityProjectAccountSettingsProps {
  antigravity: ProjectAntigravityAccountDraft
  onChange: () => void
  onManageAccounts: () => void
}

/**
 * Project settings → Accounts: pick the Antigravity Account for this Project, or
 * follow the default. A signed-out Account cannot be chosen.
 */
export function AntigravityProjectAccountSettings({
  antigravity,
  onChange,
  onManageAccounts
}: AntigravityProjectAccountSettingsProps): React.JSX.Element {
  const { snapshot, loading, error, reload, refreshUsage } = antigravity.accounts
  const canManage = useServerAdmitsRemoteWrites()
  const [signingIn, setSigningIn] = useState<AntigravityAccountView | null>(null)
  const accounts = snapshot?.accounts ?? []
  const failed = !loading && (error !== null || !snapshot)

  const readinessOf = (accountId: string | null) => {
    const account = accounts.find((a) => a.id === (accountId ?? snapshot?.defaultAccountId))
    return account ? antigravityAccountReadiness(account) : 'unconfirmed'
  }

  const choose = (accountId: string | null): void => {
    if (!canManage || accountId === antigravity.value || readinessOf(accountId) !== 'ready') return
    antigravity.setValue(accountId)
    onChange()
  }

  const selected = (accountId: string | null): boolean => antigravity.value === accountId

  return (
    <ProviderGroup
      data-testid="antigravity-project-accounts"
      iconKey="acp:antigravity-acp"
      name="Antigravity"
      lastChecked={null}
      refreshing={false}
      action={
        <Button type="button" size="xs" variant="outline" onClick={onManageAccounts}>
          Manage accounts
          <ExternalLink size={12} />
        </Button>
      }
    >
      {loading && (
        <p className="py-3 text-xs text-muted-foreground">Loading Antigravity accounts…</p>
      )}
      {failed && (
        <p className="py-3 text-xs text-muted-foreground">
          {error ?? 'Antigravity accounts are not available right now.'}
        </p>
      )}
      {!loading && snapshot && (
        <div
          className="flex flex-col gap-1"
          role="listbox"
          aria-label="Antigravity account for this project"
        >
          <AccountOptionRow
            tabStop={selected(null) || accounts.every((account) => !selected(account.id))}
            label="Follow default"
            sub={
              snapshot.accounts.find((a) => a.id === snapshot.defaultAccountId)?.name
                ? `Uses ${snapshot.accounts.find((a) => a.id === snapshot.defaultAccountId)?.name}`
                : 'Uses the default account'
            }
            selected={selected(null)}
            disabled={!canManage || readinessOf(null) !== 'ready'}
            onSelect={() => choose(null)}
          />
          {accounts.map((account) => {
            const readiness = antigravityAccountReadiness(account)
            return (
              <AccountOptionRow
                key={account.id}
                tabStop={selected(account.id)}
                label={account.name}
                sub={
                  readiness === 'ready'
                    ? account.builtin
                      ? 'Host Antigravity login'
                      : 'Signed in'
                    : ANTIGRAVITY_NOT_READY_REASON[readiness]
                }
                selected={selected(account.id)}
                disabled={!canManage || readiness !== 'ready'}
                badge={account.id === snapshot.defaultAccountId}
                onSelect={() => choose(account.id)}
                onSignIn={
                  canManage && readiness !== 'ready' ? () => setSigningIn(account) : undefined
                }
              />
            )
          })}
        </div>
      )}
      <AntigravitySignInDialog
        account={signingIn}
        onClose={() => setSigningIn(null)}
        onSignedIn={async () => {
          setSigningIn(null)
          await reload()
          await refreshUsage(true)
        }}
      />
    </ProviderGroup>
  )
}
