import type { CodexAccountUsage, CodexAccountView } from '@shared/types/codex-accounts.types'
import { useState } from 'react'
import { AlertCircle, Check, RotateCcw } from '@/components/icons'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogFooter } from '@/components/ui/dialog'
import { signInCodexAccount } from '@/lib/codex-account-sign-in'
import { codexAccountsApi } from '@/lib/codex-accounts-api'
import { codexWindowLabel, planLabel, primaryCodexWindow } from '@/lib/codex-usage-format'
import {
  AccountNameStep,
  AccountStepHeader,
  AccountSteps,
  AccountSummary,
  type AddAccountStep,
  accountErrorText as errorText
} from './account-dialog-parts'
import { CodexSignInStep } from './CodexSignInStep'

interface CodexAddAccountDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  /** Names already in use, so suggestions skip them. */
  existingNames: string[]
  defaultAccountName: string | null
  /** Latest usage reads, to summarise the new Account when it is ready. */
  usage: Record<string, CodexAccountUsage>
  /** Called after the Account list changed (created, signed in, default). */
  onChanged: () => Promise<void>
  /** Read the new Account's usage after it signed in. */
  onSignedIn: (account: CodexAccountView) => Promise<void>
}

/**
 * "Add account" for Codex, the same dialog as the Claude one: Name → Sign in
 * (ChatGPT in the browser, or a device code) → Done, or a failed step with
 * Try again. The account stays in the list, signed out, until the sign-in
 * finishes.
 */
export function CodexAddAccountDialog({
  open,
  onOpenChange,
  ...flow
}: CodexAddAccountDialogProps): React.JSX.Element {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="gap-5 sm:max-w-[400px]">
        {/* The content unmounts on close, so each open starts at Name. */}
        <AddCodexAccountFlow {...flow} close={() => onOpenChange(false)} />
      </DialogContent>
    </Dialog>
  )
}

function AddCodexAccountFlow({
  existingNames,
  defaultAccountName,
  usage,
  onChanged,
  onSignedIn,
  close
}: Omit<CodexAddAccountDialogProps, 'open' | 'onOpenChange'> & {
  close: () => void
}): React.JSX.Element {
  const [step, setStep] = useState<AddAccountStep>('name')
  const [name, setName] = useState('')
  const [account, setAccount] = useState<CodexAccountView | null>(null)
  const [method, setMethod] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const submitName = async (): Promise<void> => {
    const trimmed = name.trim()
    if (!trimmed) {
      setError('Enter a name.')
      return
    }
    setBusy(true)
    setError(null)
    try {
      const created = await codexAccountsApi.add(trimmed)
      setAccount({ id: created.id, name: created.name, builtin: false, signedIn: false })
      await onChanged()
      setStep('signing')
    } catch (err) {
      setError(errorText(err))
    } finally {
      setBusy(false)
    }
  }

  const signIn = async (target: CodexAccountView, methodId: string): Promise<void> => {
    setStep('signing')
    setMethod(methodId)
    setError(null)
    try {
      await signInCodexAccount(target.id, methodId)
      await onChanged()
      await onSignedIn(target)
      setStep('done')
    } catch (err) {
      setError(errorText(err))
      setStep('failed')
    }
  }

  const makeDefault = async (): Promise<void> => {
    if (!account) return
    setBusy(true)
    try {
      await codexAccountsApi.setDefault(account.id)
      await onChanged()
      close()
    } catch (err) {
      setError(errorText(err))
    } finally {
      setBusy(false)
    }
  }

  const read = account ? usage[account.id] : undefined
  const primary = primaryCodexWindow(read)

  return (
    <>
      {step !== 'failed' && <AccountSteps step={step} />}

      {step === 'name' && (
        <AccountNameStep
          provider="Codex"
          name={name}
          onNameChange={setName}
          existingNames={existingNames}
          error={error}
          busy={busy}
          onSubmit={() => void submitName()}
          onCancel={close}
        />
      )}

      {step === 'signing' && account && (
        <CodexSignInStep
          accountName={account.name}
          method={method}
          error={null}
          onPick={(methodId) => void signIn(account, methodId)}
          onCancel={close}
        />
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
            plan={planLabel(read?.planType ?? null)}
            usageLabel={primary ? codexWindowLabel(primary) : 'Usage'}
            usageValue={primary ? `${Math.round(primary.percent)}% used` : 'not started'}
            defaultAccountName={defaultAccountName}
          />
          {error && <p className="text-xs text-destructive">{error}</p>}
          <DialogFooter>
            <Button variant="ghost" size="sm" onClick={() => void makeDefault()} disabled={busy}>
              Make default
            </Button>
            <Button size="sm" onClick={close} disabled={busy}>
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
            {error ?? 'The sign-in window closed or timed out.'} {account.name} stays in your list,
            signed out.
          </AccountStepHeader>
          <DialogFooter>
            <Button variant="ghost" size="sm" onClick={close}>
              Close
            </Button>
            <Button
              size="sm"
              onClick={() => {
                setMethod(null)
                setError(null)
                setStep('signing')
              }}
            >
              <RotateCcw />
              Try again
            </Button>
          </DialogFooter>
        </>
      )}
    </>
  )
}
