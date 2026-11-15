import { CODEX_CHATGPT_METHOD } from '@shared/types/codex-accounts.types'
import { Globe } from '@/components/icons'
import { Button } from '@/components/ui/button'
import { DialogFooter } from '@/components/ui/dialog'
import { CODEX_SIGN_IN_METHODS } from '@/lib/codex-account-sign-in'
import { AccountSignInWaiting, AccountStepHeader } from './account-dialog-parts'

/**
 * The Codex "Sign in" step: pick ChatGPT in the browser or a device code,
 * then wait while the host sign-in window shows the page or the code. Same
 * layout as the Claude sign-in step.
 */
export function CodexSignInStep({
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
      ? 'Codex accounts use a ChatGPT login. Sign in with the browser, or use a device code on another device.'
      : method === CODEX_CHATGPT_METHOD
        ? 'Your browser opened ChatGPT. Sign in with the account you want to add, then come back here.'
        : 'Enter the code from the sign-in window on any device, then come back here.'
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
          {CODEX_SIGN_IN_METHODS.map((option) => (
            <Button
              key={option.id}
              type="button"
              size="sm"
              variant={option.id === CODEX_CHATGPT_METHOD ? 'default' : 'secondary'}
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
        <Button variant="ghost" size="sm" onClick={onCancel}>
          Cancel
        </Button>
      </DialogFooter>
    </>
  )
}
