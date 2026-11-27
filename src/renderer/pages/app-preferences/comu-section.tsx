import { ComuAppPolicy } from '@/components/settings/ComuPolicySettings'
import { SettingsSection } from '@/components/settings/SettingsLayout'

export function ComuSection(): React.JSX.Element {
  return (
    <SettingsSection id="comu">
      <ComuAppPolicy />
    </SettingsSection>
  )
}
