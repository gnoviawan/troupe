import { ChevronDown, Folder } from '@/components/icons'
import { APP_VERSION, VersionCard } from '@/components/settings/about/VersionCard'
import { rowId } from '@/components/settings/row-id'
import {
  SETTINGS_CARD_CLASS,
  SettingsCard,
  SettingsGroup
} from '@/components/settings/SettingsGroup'
import { SettingsSection } from '@/components/settings/SettingsLayout'
import { SettingsNotice } from '@/components/settings/SettingsNotice'
import { SettingsRow, settingsDescriptionId } from '@/components/settings/SettingsRow'
import { SettingsSegmented } from '@/components/settings/SettingsSegmented'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger
} from '@/components/ui/dropdown-menu'
import { Switch } from '@/components/ui/switch'
import { logApi } from '@/lib/api'
import { isMac, isWindows } from '@/lib/platform'
import { isTauriContext } from '@/lib/tauri-runtime'
import { isAurUpdateMode, type UpdateChannel } from '@/lib/tauri-updater-api'
import { cn } from '@/lib/utils'
import { useReportBugStore } from '@/stores/report-bug-store'
import { useUpdaterActions, useUpdaterState } from '@/stores/updater-store'
import { ServerSection } from './server-section'

const CHANNELS: Array<{ value: UpdateChannel; label: string; description: string }> = [
  { value: 'stable', label: 'Stable', description: 'Production releases. Most reliable.' },
  { value: 'nightly', label: 'Nightly', description: 'Automated main-branch builds.' }
]

const FOLDER_LABEL = isMac
  ? 'Show in Finder'
  : isWindows
    ? 'Show in File Explorer'
    : 'Show in file manager'

/**
 * Settings → About. Desktop: your version and the update flow, Updates
 * (channel, automatic checks), logs and bug reports, then a full reset. Web:
 * the server's version and setup, logs and bug reports, then the reset.
 */
export function AboutSection({ onResetAll }: { onResetAll: () => void }): React.JSX.Element {
  const isDesktop = isTauriContext()
  const isAurUpdater = isAurUpdateMode()
  return (
    <SettingsSection id="about">
      {isDesktop ? (
        <>
          <SettingsGroup title="Version" description="Current build and update status.">
            <VersionCard isAurUpdater={isAurUpdater} />
          </SettingsGroup>
          <UpdatesGroup isAurUpdater={isAurUpdater} />
        </>
      ) : (
        <>
          <SettingsGroup title="Version" description="Current server build.">
            <SettingsCard>
              <SettingsRow
                id={rowId('server-version')}
                label={`termul-server ${APP_VERSION}`}
                description="The web client is served by the termul-server and updates together with it. Reload the page after the server updates."
              />
            </SettingsCard>
          </SettingsGroup>
          <ServerSection />
        </>
      )}
      <LogsGroup isDesktop={isDesktop} />
      <SettingsGroup title="Reset" description="Restore all preferences and shortcuts to defaults.">
        <div
          id={rowId('reset-settings')}
          data-settings-row=""
          className={cn(
            'flex scroll-mt-4 items-center gap-6 px-4 py-3.5 data-[settings-target]:shadow-[inset_2px_0_0_oklch(var(--foreground))]',
            SETTINGS_CARD_CLASS
          )}
        >
          <div className="min-w-0 flex-1">
            <p className="text-xs font-medium leading-4 text-foreground">Reset all settings</p>
            <p className="mt-0.5 text-xs leading-4 text-muted-foreground">
              Every setting goes back to its default, including keyboard shortcuts. Terminal
              profiles stay.
            </p>
          </div>
          <Button
            type="button"
            size="xs"
            variant="secondary"
            className="text-destructive hover:text-destructive"
            onClick={onResetAll}
          >
            Reset settings
          </Button>
        </div>
      </SettingsGroup>
    </SettingsSection>
  )
}

function UpdatesGroup({ isAurUpdater }: { isAurUpdater: boolean }): React.JSX.Element {
  const { isChecking, autoUpdateEnabled, isManualUpdateMode, updateChannel } = useUpdaterState()
  const { setAutoUpdateEnabled, setUpdateChannel } = useUpdaterActions()
  const channel = CHANNELS.find((option) => option.value === updateChannel) ?? CHANNELS[0]
  return (
    <SettingsGroup title="Updates" description="Release channel and automatic update checks.">
      <SettingsCard>
        {!isAurUpdater && (
          <SettingsRow
            id={rowId('release-channel')}
            describes="release-channel"
            label="Release channel"
            description={channel.description}
          >
            <SettingsSegmented
              id="release-channel"
              label="Release channel"
              describedBy={settingsDescriptionId('release-channel')}
              options={CHANNELS}
              value={updateChannel}
              disabled={isChecking}
              onValueChange={(value) => void setUpdateChannel(value)}
            />
          </SettingsRow>
        )}
        <SettingsRow
          id={rowId('auto-update')}
          htmlFor="auto-update"
          label="Check for updates automatically"
          description="Troupe checks for a new version from time to time."
          layout="inline"
        >
          <Switch
            id="auto-update"
            aria-describedby={settingsDescriptionId('auto-update')}
            checked={autoUpdateEnabled}
            onCheckedChange={(enabled) => void setAutoUpdateEnabled(enabled)}
          />
        </SettingsRow>
      </SettingsCard>
      {!isAurUpdater && updateChannel === 'nightly' && (
        <SettingsNotice tone="warning">
          {`Nightly builds are automated from the latest commit and may be unstable. ${
            isManualUpdateMode
              ? 'Updates are offered as a manual download from the nightly release page.'
              : 'Updates are signed and install in the app.'
          }`}
        </SettingsNotice>
      )}
    </SettingsGroup>
  )
}

function LogsGroup({ isDesktop }: { isDesktop: boolean }): React.JSX.Element {
  return (
    <SettingsGroup
      title="Logs and bug reports"
      description="Diagnostics for troubleshooting issues."
    >
      <SettingsCard>
        <SettingsRow
          id={rowId('report-bug')}
          label="Report a bug"
          description="File an issue with a redacted diagnostics bundle."
          layout="inline"
        >
          <Button
            type="button"
            size="xs"
            variant="secondary"
            aria-label="Report a bug"
            onClick={() => useReportBugStore.getState().openReport()}
          >
            Report
          </Button>
        </SettingsRow>
        <SettingsRow
          id={rowId('application-log')}
          label="Application log"
          description={
            isDesktop
              ? 'Copy the log, save a copy, or open the folder that holds it.'
              : 'Copy the log. It lives on the server host.'
          }
          layout="inline"
        >
          <Button
            type="button"
            size="xs"
            variant="secondary"
            aria-label="Copy log contents"
            onClick={() => void logApi.copyLogContents()}
          >
            Copy
          </Button>
          {isDesktop && (
            <>
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button type="button" size="xs" variant="secondary" aria-label="Export log">
                    Export
                    <ChevronDown size={12} />
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end">
                  <DropdownMenuItem onSelect={() => void logApi.exportLogFile()}>
                    Save as…
                  </DropdownMenuItem>
                  <DropdownMenuItem onSelect={() => void logApi.exportLogToDefault()}>
                    Save to Downloads
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
              <Button
                type="button"
                size="xs"
                variant="secondary"
                aria-label="Show log folder"
                onClick={() => void logApi.revealLogDir()}
              >
                <Folder size={12} />
                {FOLDER_LABEL}
              </Button>
            </>
          )}
        </SettingsRow>
      </SettingsCard>
      {!isDesktop && (
        <SettingsNotice>
          Logs live on the server host. Export and the log folder are available in the desktop app.
        </SettingsNotice>
      )}
    </SettingsGroup>
  )
}
