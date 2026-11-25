import { SettingsSection } from '@/components/settings/SettingsLayout'
import { SubagentSettingsGroup } from './subagent-settings-group'

export function SubagentsSection(): React.JSX.Element {
  return (
    <SettingsSection id="subagents">
      <SubagentSettingsGroup />
    </SettingsSection>
  )
}
