import type { CodexAccountView } from '@shared/types/codex-accounts.types'
import { useState } from 'react'
import { Dialog, DialogContent } from '@/components/ui/dialog'
import { signInCodexAccount } from '@/lib/codex-account-sign-in'
import { accountErrorText } from './account-dialog-parts'
import { CodexSignInStep } from './CodexSignInStep'

/**
 * Sign in an existing Codex account: the "Sign in" step of the Add account
 * dialog on its own. A failed sign-in shows its reason and lets the user pick
 * a method again.
 */
export function CodexSignInDialog({
  account,
  onClose,
  onSignedIn
}: {
  account: CodexAccountView | null
  onClose: () => void
  onSignedIn: (account: CodexAccountView) => Promise<void>
}): React.JSX.Element {
  return (
    <Dialog open={account !== null} onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="gap-5 sm:max-w-[400px]">
        {/* The content unmounts on close, so each open starts at the method pick. */}
        {account && <CodexSignInFlow account={account} onClose={onClose} onSignedIn={onSignedIn} />}
      </DialogContent>
    </Dialog>
  )
}

function CodexSignInFlow({
  account,
  onClose,
  onSignedIn
}: {
  account: CodexAccountView
  onClose: () => void
  onSignedIn: (account: CodexAccountView) => Promise<void>
}): React.JSX.Element {
  const [method, setMethod] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)

  const start = async (methodId: string): Promise<void> => {
    setMethod(methodId)
    setError(null)
    try {
      await signInCodexAccount(account.id, methodId)
      await onSignedIn(account)
    } catch (err) {
      setError(accountErrorText(err))
      setMethod(null)
    }
  }

  return (
    <CodexSignInStep
      accountName={account.name}
      method={method}
      error={error}
      onPick={(methodId) => void start(methodId)}
      onCancel={onClose}
    />
  )
}
