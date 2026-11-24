import {
  ANTIGRAVITY_GOOGLE_METHOD,
  type AntigravityAccountView
} from '@shared/types/antigravity-accounts.types'
import { useState } from 'react'
import { Globe } from '@/components/icons'
import { Button } from '@/components/ui/button'
import { DialogFooter } from '@/components/ui/dialog'
import {
  ANTIGRAVITY_SIGN_IN_METHODS,
  signInAntigravityAccount
} from '@/lib/antigravity-account-sign-in'
import { AccountSignInWaiting, AccountStepHeader } from './account-dialog-parts'
import { accountErrorText } from './account-error'

/**
 * The Antigravity "Sign in" step: pick a Google account or Gemini Enterprise,
 * then wait while the browser sign-in window shows the Google page. Same
 * layout as the Claude and Codex sign-in steps.
 */
export function AntigravitySignInStep({
  accountName,
  method,
  error,
  onPick,
  onCancel
}: {
  accountName: string
  /** The method that is running, or null while the user picks one. */
  method: string | null
  error: string | null
  onPick: (methodId: string) => void
  onCancel: () => void
}): React.JSX.Element {
  const description =
    method === null
      ? 'Antigravity accounts use a Google login. Sign in with a Google account, or with Gemini Enterprise for a work account.'
      : method === ANTIGRAVITY_GOOGLE_METHOD
        ? 'The sign-in window opened Google. Sign in with the account you want to add, then come back here.'
        : 'The sign-in window opened Google. Sign in with your Gemini Enterprise account, then come back here.'
  return (
    <>
      <AccountStepHeader
        tone="muted"
        icon={<Globe size={15} />}
        title={`Sign in to ${accountName}`}
      >
        {description}
      </AccountStepHeader>
      {method === null ? (
        <div className="grid gap-2">
          {ANTIGRAVITY_SIGN_IN_METHODS.map((option) => (
            <Button
              key={option.id}
              type="button"
              size="sm"
              variant={option.id === ANTIGRAVITY_GOOGLE_METHOD ? 'default' : 'secondary'}
              onClick={() => onPick(option.id)}
            >
              {option.label}
            </Button>
          ))}
        </div>
      ) : (
        <AccountSignInWaiting />
      )}
      {error && <p className="text-xs text-destructive">{error}</p>}
      <DialogFooter>
        {/* A running sign-in cannot be stopped here: the host window has its own Cancel. */}
        <Button variant="ghost" size="sm" onClick={onCancel} disabled={method !== null}>
          Cancel
        </Button>
      </DialogFooter>
    </>
  )
}

/**
 * Runs an Antigravity sign-in: the method pick, then the wait. `onRunningChange`
 * reports a running sign-in, so the dialog can stay open until it ends.
 * Without `onFailed` a failure shows its reason here and the pick comes back.
 */
export function AntigravitySignInRun({
  account,
  onRunningChange,
  onSignedIn,
  onFailed,
  onCancel
}: {
  account: AntigravityAccountView
  onRunningChange: (running: boolean) => void
  onSignedIn: () => void
  onFailed?: (reason: string) => void
  onCancel: () => void
}): React.JSX.Element {
  const [method, setMethod] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)

  const start = async (methodId: string): Promise<void> => {
    setMethod(methodId)
    setError(null)
    onRunningChange(true)
    try {
      await signInAntigravityAccount(account.id, methodId)
      onRunningChange(false)
      onSignedIn()
    } catch (err) {
      onRunningChange(false)
      if (onFailed) {
        onFailed(accountErrorText(err))
        return
      }
      setError(accountErrorText(err))
      setMethod(null)
    }
  }

  return (
    <AntigravitySignInStep
      accountName={account.name}
      method={method}
      error={error}
      onPick={(methodId) => void start(methodId)}
      onCancel={onCancel}
    />
  )
}
