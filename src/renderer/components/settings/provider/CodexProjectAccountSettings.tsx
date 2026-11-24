import type { CodexAccountView } from '@shared/types/codex-accounts.types'
import { useState } from 'react'
import { ExternalLink } from '@/components/icons'
import { Button } from '@/components/ui/button'
import type { ProjectCodexAccountDraft } from '@/hooks/use-project-codex-account-draft'
import { useServerAdmitsRemoteWrites } from '@/hooks/use-server-admits-remote-writes'
import { AccountOptionRow } from './AccountOptionRow'
import { CodexSignInDialog } from './CodexSignInDialog'
import { CODEX_NOT_READY_REASON, codexAccountReadiness } from './codex-account-status'
import { ProviderGroupHeader } from './ProviderGroupHeader'

interface CodexProjectAccountSettingsProps {
  codex: ProjectCodexAccountDraft
  onChange: () => void
  onManageAccounts: () => void
}

/**
 * Project settings → Accounts: pick the Codex Account for this Project, or
 * follow the default. A signed-out Account cannot be chosen.
 */
export function CodexProjectAccountSettings({
  codex,
  onChange,
  onManageAccounts
}: CodexProjectAccountSettingsProps): React.JSX.Element {
  const { snapshot, loading, error, reload, refreshUsage } = codex.accounts
  const canManage = useServerAdmitsRemoteWrites()
  const [signingIn, setSigningIn] = useState<CodexAccountView | null>(null)
  const accounts = snapshot?.accounts ?? []
  const failed = !loading && (error !== null || !snapshot)

  const readinessOf = (accountId: string | null) => {
    const account = accounts.find((a) => a.id === (accountId ?? snapshot?.defaultAccountId))
    return account ? codexAccountReadiness(account) : 'unconfirmed'
  }

  const choose = (accountId: string | null): void => {
    if (!canManage || accountId === codex.value || readinessOf(accountId) !== 'ready') return
    codex.setValue(accountId)
    onChange()
  }

  const selected = (accountId: string | null): boolean => codex.value === accountId

  return (
    <div className="mt-6 flex flex-col gap-2" data-testid="codex-project-accounts">
      <ProviderGroupHeader
        iconKey="acp:codex-acp"
        name="Codex"
        lastChecked={null}
        refreshing={false}
        action={
          <Button type="button" size="xs" variant="outline" onClick={onManageAccounts}>
            Manage accounts
            <ExternalLink size={12} />
          </Button>
        }
      />
      {loading && <p className="py-3 text-xs text-muted-foreground">Loading Codex accounts…</p>}
      {failed && (
        <p className="py-3 text-xs text-muted-foreground">
          {error ?? 'Codex accounts are not available right now.'}
        </p>
      )}
      {!loading && snapshot && (
        <div
          className="flex flex-col gap-1"
          role="listbox"
          aria-label="Codex account for this project"
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
            const readiness = codexAccountReadiness(account)
            return (
              <AccountOptionRow
                key={account.id}
                tabStop={selected(account.id)}
                label={account.name}
                sub={
                  readiness === 'ready'
                    ? account.builtin
                      ? 'Host Codex home'
                      : 'Signed in'
                    : CODEX_NOT_READY_REASON[readiness]
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
      <CodexSignInDialog
        account={signingIn}
        onClose={() => setSigningIn(null)}
        onSignedIn={async () => {
          setSigningIn(null)
          await reload()
          await refreshUsage(true)
        }}
      />
    </div>
  )
}
