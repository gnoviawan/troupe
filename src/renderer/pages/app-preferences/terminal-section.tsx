import type { DetectedShells } from '@shared/types/ipc.types'
import { useEffect, useState } from 'react'
import { rowId } from '@/components/settings/row-id'
import { SettingsDisclosure } from '@/components/settings/SettingsDisclosure'
import { SettingsCard, SettingsGroup } from '@/components/settings/SettingsGroup'
import { SettingsSection } from '@/components/settings/SettingsLayout'
import { SettingsRow, settingsDescriptionId } from '@/components/settings/SettingsRow'
import { SettingsSelect } from '@/components/settings/SettingsSelect'
import { SettingsStepper } from '@/components/settings/SettingsStepper'
import {
  shellFromSelectValue,
  shellSelectOptions,
  shellSelectValue
} from '@/components/settings/shell-options'
import { TERMINAL_LINE_HEIGHT } from '@/components/terminal/terminal-config'
import { Switch } from '@/components/ui/switch'
import { useUpdateAppSetting } from '@/hooks/use-app-settings'
import { shellApi } from '@/lib/api'
import { MONO_STACK } from '@/lib/font-stacks'
import { logFrontendError } from '@/lib/log-api'
import {
  useAutomaticCompletionRefresh,
  useDefaultShell,
  useTerminalBufferSize,
  useTerminalFontSize,
  useTerminalRenderer,
  useTerminalScreenReaderMode
} from '@/stores/app-settings-store'
import { BUFFER_SIZE_OPTIONS, TERMINAL_RENDERER_OPTIONS } from '@/types/settings'

const FONT_SIZE_MIN = 10
const FONT_SIZE_MAX = 24

const BUFFER_OPTIONS = BUFFER_SIZE_OPTIONS.map((option) => ({
  value: String(option.value),
  label: option.label
}))

/** The Terminal page: text size with a live preview, new terminals, and advanced options. */
export function TerminalSection(): React.JSX.Element {
  return (
    <SettingsSection id="terminal">
      <TextGroup />
      <NewTerminalsGroup />
      <AdvancedGroup />
    </SettingsSection>
  )
}

function TextGroup(): React.JSX.Element {
  const updateSetting = useUpdateAppSetting()
  const fontSize = useTerminalFontSize()
  return (
    <SettingsGroup title="Text">
      <SettingsCard>
        <SettingsRow
          id={rowId('terminal-font-size')}
          label="Font size"
          describes="terminal-font-size"
          description={`Terminal text size, from ${FONT_SIZE_MIN} to ${FONT_SIZE_MAX} px.`}
        >
          <SettingsStepper
            role="group"
            aria-label="Font size"
            aria-describedby={settingsDescriptionId('terminal-font-size')}
            value={fontSize}
            min={FONT_SIZE_MIN}
            max={FONT_SIZE_MAX}
            onValueChange={(value) => updateSetting('terminalFontSize', value)}
            format={(value) => `${value} px`}
            decrementLabel="Decrease font size"
            incrementLabel="Increase font size"
          />
        </SettingsRow>
        <figure className="m-0 bg-terminal-bg px-4 py-3 text-terminal-fg" aria-label="Preview">
          <div
            style={{
              fontFamily: MONO_STACK,
              fontSize: `${fontSize}px`,
              lineHeight: TERMINAL_LINE_HEIGHT
            }}
          >
            <div>$ echo "Hello, World!"</div>
            <div>Hello, World!</div>
            <div>$ ls -la</div>
            <div>drwxr-xr-x 5 user staff 160 Jan 11 10:00 .</div>
          </div>
        </figure>
      </SettingsCard>
    </SettingsGroup>
  )
}

function NewTerminalsGroup(): React.JSX.Element {
  const updateSetting = useUpdateAppSetting()
  const defaultShell = useDefaultShell()
  const screenReaderMode = useTerminalScreenReaderMode()
  const [availableShells, setAvailableShells] = useState<DetectedShells | null>(null)

  useEffect(() => {
    let cancelled = false
    shellApi
      .getAvailableShells()
      .then((result) => {
        if (!cancelled && result.success && result.data) setAvailableShells(result.data)
      })
      // The select still offers System default, so a failed read only shortens the list.
      .catch(() => undefined)
    return () => {
      cancelled = true
    }
  }, [])

  const shellValue = shellSelectValue(defaultShell, availableShells)
  const shellOptions = shellSelectOptions(shellValue, availableShells, 'System default')

  const handleScreenReaderModeChange = async (enabled: boolean): Promise<void> => {
    await updateSetting('terminalScreenReaderMode', enabled)
    // Boundary log (not a failure): the setting only reaches terminals created
    // after the toggle, and xterm.js #3467 / repo #267 can double typed bytes.
    void logFrontendError({
      level: 'info',
      source: 'AppPreferences.terminalScreenReaderMode',
      message: `Terminal screen reader mode ${enabled ? 'enabled' : 'disabled'}; applies to new terminals only (can repeat typed characters, xterm.js #3467 / repo #267)`
    })
  }

  return (
    <SettingsGroup title="New terminals">
      <SettingsCard>
        <SettingsRow
          id={rowId('default-shell')}
          htmlFor="default-shell"
          label="Default shell"
          description="A project can choose its own shell in Project settings."
        >
          <SettingsSelect
            id="default-shell"
            aria-describedby={settingsDescriptionId('default-shell')}
            options={shellOptions}
            value={shellValue}
            onValueChange={(next) => updateSetting('defaultShell', shellFromSelectValue(next))}
          />
        </SettingsRow>
        <SettingsRow
          id={rowId('terminal-screen-reader-mode')}
          htmlFor="terminal-screen-reader-mode"
          label="Screen reader mode"
          description="Makes terminal output readable by screen readers. Can repeat typed characters in some setups. Changes apply to new terminals."
          layout="inline"
        >
          <Switch
            id="terminal-screen-reader-mode"
            aria-describedby={settingsDescriptionId('terminal-screen-reader-mode')}
            checked={screenReaderMode}
            onCheckedChange={handleScreenReaderModeChange}
          />
        </SettingsRow>
      </SettingsCard>
    </SettingsGroup>
  )
}

function AdvancedGroup(): React.JSX.Element {
  const updateSetting = useUpdateAppSetting()
  const terminalRenderer = useTerminalRenderer()
  const bufferSize = useTerminalBufferSize()
  const automaticCompletionRefresh = useAutomaticCompletionRefresh()

  const rendererLabel =
    TERMINAL_RENDERER_OPTIONS.find((option) => option.value === terminalRenderer)?.label ?? 'Auto'
  const bufferLabel =
    BUFFER_SIZE_OPTIONS.find((option) => option.value === bufferSize)?.label ??
    `${bufferSize.toLocaleString()} lines`
  const summary = `${rendererLabel} renderer · ${bufferLabel} · completion refresh ${
    automaticCompletionRefresh ? 'on' : 'off'
  }`

  const handleRendererChange = (value: string): void => {
    if (value === 'auto' || value === 'webgl' || value === 'dom') {
      void updateSetting('terminalRenderer', value)
    }
  }

  const handleAutomaticCompletionRefreshChange = async (enabled: boolean): Promise<void> => {
    await updateSetting('automaticCompletionRefresh', enabled)
    void logFrontendError({
      level: 'info',
      source: 'AppPreferences.automaticCompletionRefresh',
      // The scheduler reads the setting live (see bind-session.ts), so the
      // flip is immediate — unlike xterm options, which need a new terminal.
      message: `Automatic completion refresh ${enabled ? 'enabled' : 'disabled'}; applies immediately`
    })
  }

  return (
    <SettingsDisclosure title="Advanced" summary={summary}>
      <SettingsRow
        id={rowId('terminal-renderer')}
        htmlFor="terminal-renderer"
        label="Renderer"
        description="WebGL is fastest. Auto falls back to DOM when WebGL is not available. Applies to new terminals."
      >
        <SettingsSelect
          id="terminal-renderer"
          aria-describedby={settingsDescriptionId('terminal-renderer')}
          options={TERMINAL_RENDERER_OPTIONS}
          value={terminalRenderer}
          onValueChange={handleRendererChange}
        />
      </SettingsRow>
      <SettingsRow
        id={rowId('terminal-scrollback')}
        htmlFor="terminal-scrollback"
        label="Scrollback"
        description="Lines kept in terminal history. More lines use more memory. Applies to new terminals."
      >
        <SettingsSelect
          id="terminal-scrollback"
          aria-describedby={settingsDescriptionId('terminal-scrollback')}
          options={BUFFER_OPTIONS}
          value={String(bufferSize)}
          onValueChange={(value) =>
            void updateSetting('terminalBufferSize', Number.parseInt(value, 10))
          }
        />
      </SettingsRow>
      <SettingsRow
        id={rowId('terminal-automatic-completion-refresh')}
        htmlFor="terminal-automatic-completion-refresh"
        label="Automatic completion refresh"
        /* The scheduler reads this setting live per request (bind-session
           passes a getter over the store), so flips take effect at once —
           unlike xterm options, which only apply to new terminals. */
        description="Updates command suggestions while you type. When off, press Tab to ask for suggestions. Applies immediately."
        layout="inline"
      >
        <Switch
          id="terminal-automatic-completion-refresh"
          aria-describedby={settingsDescriptionId('terminal-automatic-completion-refresh')}
          checked={automaticCompletionRefresh}
          onCheckedChange={handleAutomaticCompletionRefreshChange}
        />
      </SettingsRow>
    </SettingsDisclosure>
  )
}
