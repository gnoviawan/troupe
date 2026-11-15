import type { ClaudeAccount, ClaudeAccountUsage } from '@shared/types/claude-accounts.types'
import { useEffect, useState } from 'react'
import { AlertCircle, Check, Globe, RotateCcw } from '@/components/icons'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogFooter } from '@/components/ui/dialog'
import { claudeAccountsApi, isClaudeLoginCancelled } from '@/lib/claude-accounts-api'
import { planLabel, primaryUsageWindow, usageWindowLabel } from '@/lib/claude-usage-format'
import {
  AccountNameStep,
  AccountSignInWaiting,
  AccountStepHeader,
  AccountSteps,
  AccountSummary,
  type AddAccountStep,
  accountErrorText as errorText
} from './account-dialog-parts'
import { LinkSignInStep } from './LinkSignInStep'

interface AddAccountDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  /** Names already in use, so suggestions skip them. */
  existingNames: string[]
  defaultAccountName: string | null
  /** Latest usage reads, to summarise the new Account when it is ready. */
  usage: Record<string, ClaudeAccountUsage>
  /** Called after the Account list changed (created, signed in, default). */
  onChanged: () => Promise<void>
  /** Read the new Account's usage after it signed in. */
  onSignedIn: (account: ClaudeAccount) => Promise<void>
}

/**
 * "Add account" as one dialog that stays open through the sign-in:
 * Name → Sign in → Done (or a failed step with Try again). The desktop app
 * waits for the host's browser; a browser client shows the link sign-in.
 */
export function AddAccountDialog({
  open,
  onOpenChange,
  existingNames,
  defaultAccountName,
  usage,
  onChanged,
  onSignedIn
}: AddAccountDialogProps): React.JSX.Element {
  const flow = claudeAccountsApi.signInFlow()
  const [step, setStep] = useState<AddAccountStep>('name')
  const [name, setName] = useState('')
  const [account, setAccount] = useState<ClaudeAccount | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    if (open) {
      setStep('name')
      setName('')
      setAccount(null)
      setError(null)
      setBusy(false)
    }
  }, [open])

  const signIn = async (target: ClaudeAccount): Promise<void> => {
    setStep('signing')
    setError(null)
    if (flow === 'link') return
    try {
      await claudeAccountsApi.login(target.id)
      await signedIn(target)
    } catch (err) {
      if (isClaudeLoginCancelled(err)) return
      setError(errorText(err))
      setStep('failed')
    }
  }

  const signedIn = async (target: ClaudeAccount): Promise<void> => {
    try {
      await onChanged()
      await onSignedIn(target)
      setStep('done')
    } catch (err) {
      setError(errorText(err))
      setStep('failed')
    }
  }

  const submitName = async (): Promise<void> => {
    const trimmed = name.trim()
    if (!trimmed) {
      setError('Enter a name.')
      return
    }
    setBusy(true)
    setError(null)
    try {
      const created = await claudeAccountsApi.add(trimmed)
      setAccount(created)
      await onChanged()
      void signIn(created)
    } catch (err) {
      setError(errorText(err))
    } finally {
      setBusy(false)
    }
  }

  const close = (): void => {
    // The link step stops its own sign-in when it closes.
    if (step === 'signing' && account && flow === 'browser') {
      void claudeAccountsApi.cancelLogin(account.id)
    }
    onOpenChange(false)
  }

  const makeDefault = async (): Promise<void> => {
    if (!account) return
    setBusy(true)
    try {
      await claudeAccountsApi.setDefault(account.id)
      await onChanged()
      onOpenChange(false)
    } catch (err) {
      setError(errorText(err))
    } finally {
      setBusy(false)
    }
  }

  const read = account ? usage[account.id] : undefined
  const primary = primaryUsageWindow(read)

  return (
    <Dialog open={open} onOpenChange={(next) => !next && close()}>
      <DialogContent className="gap-5 sm:max-w-[400px]">
        {step !== 'failed' && <AccountSteps step={step} />}

        {step === 'name' && (
          <AccountNameStep
            provider="Claude"
            name={name}
            onNameChange={setName}
            existingNames={existingNames}
            error={error}
            busy={busy}
            onSubmit={() => void submitName()}
            onCancel={close}
          />
        )}

        {step === 'signing' && account && flow === 'link' && (
          <LinkSignInStep
            account={account}
            onCancel={close}
            onSignedIn={() => void signedIn(account)}
          />
        )}

        {step === 'signing' && account && flow === 'browser' && (
          <>
            <AccountStepHeader
              tone="muted"
              icon={<Globe size={15} />}
              title={`Sign in to ${account.name}`}
            >
              Your browser opened claude.ai. Sign in with the account you want to add, then come
              back here.
            </AccountStepHeader>
            <AccountSignInWaiting />
            <p className="text-2xs leading-relaxed text-muted-foreground">
              Already signed in to another account on claude.ai? Sign out there first, or open the
              sign-in page in a private window.
            </p>
            <DialogFooter>
              <Button variant="ghost" size="sm" onClick={close}>
                Cancel
              </Button>
            </DialogFooter>
          </>
        )}

        {step === 'done' && account && (
          <>
            <AccountStepHeader
              tone="success"
              icon={<Check size={15} />}
              title={`${account.name} is ready`}
            >
              New chats in projects that use {account.name} sign in with it.
            </AccountStepHeader>
            <AccountSummary
              plan={planLabel(read?.subscriptionType ?? null)}
              usageLabel={primary ? usageWindowLabel(primary) : 'Usage'}
              usageValue={primary ? `${Math.round(primary.percent)}% used` : 'not started'}
              defaultAccountName={defaultAccountName}
            />
            {error && <p className="text-xs text-destructive">{error}</p>}
            <DialogFooter>
              <Button variant="ghost" size="sm" onClick={() => void makeDefault()} disabled={busy}>
                Make default
              </Button>
              <Button size="sm" onClick={() => onOpenChange(false)} disabled={busy}>
                Done
              </Button>
            </DialogFooter>
          </>
        )}

        {step === 'failed' && account && (
          <>
            <AccountStepHeader
              tone="warning"
              icon={<AlertCircle size={15} />}
              title="Sign-in did not finish"
            >
              {error ?? 'The browser window closed or timed out.'} {account.name} stays in your
              list, signed out.
            </AccountStepHeader>
            <DialogFooter>
              <Button variant="ghost" size="sm" onClick={() => onOpenChange(false)}>
                Close
              </Button>
              <Button size="sm" onClick={() => void signIn(account)}>
                <RotateCcw />
                Try again
              </Button>
            </DialogFooter>
          </>
        )}
      </DialogContent>
    </Dialog>
  )
}
