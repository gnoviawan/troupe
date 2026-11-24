import { useState } from 'react'
import { KeyRound } from '@/components/icons'
import { Switch } from '@/components/ui/switch'
import { logFrontendError } from '@/lib/log-api'
import { SettingsCard } from '../SettingsGroup'
import { SettingsRow, settingsDescriptionId } from '../SettingsRow'
import {
  type AccountProviderId,
  AutoSwitchRiskDialog,
  PROVIDER_TERMS
} from './account-provider-terms'

export function AccountGroupFooter({
  provider,
  accountCount,
  canManage,
  autoSwitch,
  saving,
  onAutoSwitchChange
}: {
  provider: AccountProviderId
  accountCount: number
  canManage: boolean
  autoSwitch: boolean
  saving: boolean
  onAutoSwitchChange: (enabled: boolean) => void
}): React.JSX.Element | null {
  const [confirming, setConfirming] = useState(false)
  const switchId = `${provider}-auto-switch`

  const closeWarning = (accepted: boolean): void => {
    setConfirming(false)
    void logFrontendError({
      level: 'info',
      source: 'AccountGroupFooter',
      message: `[accounts] ${provider} automatic switching risk warning ${accepted ? 'accepted' : 'cancelled'}`
    })
    if (accepted) onAutoSwitchChange(true)
  }

  if (accountCount >= 2) {
    return (
      <>
        <SettingsCard className="mt-3">
          <SettingsRow
            htmlFor={switchId}
            label="Switch accounts automatically"
            description={`When a chat reaches its account's limit, move it to another signed-in account. Off by default, because ${PROVIDER_TERMS[provider].company}'s terms may not allow it.`}
            layout="inline"
          >
            <Switch
              id={switchId}
              aria-describedby={settingsDescriptionId(switchId)}
              checked={autoSwitch}
              disabled={saving || !canManage}
              onCheckedChange={(enabled) =>
                enabled ? setConfirming(true) : onAutoSwitchChange(false)
              }
            />
          </SettingsRow>
        </SettingsCard>
        <AutoSwitchRiskDialog
          provider={provider}
          open={confirming}
          onConfirm={() => closeWarning(true)}
          onCancel={() => closeWarning(false)}
        />
      </>
    )
  }
  if (!canManage) return null
  return (
    <p
      data-testid={`${provider}-second-account-hint`}
      className="flex items-start gap-2 pt-3 text-2xs leading-relaxed text-muted-foreground"
    >
      <KeyRound size={12} aria-hidden="true" className="mt-0.5 shrink-0" />
      Add a second account to keep work and personal apart.
    </p>
  )
}
