import { useState } from 'react'
import { Plus, RefreshCw, ShieldCheck, X } from '@/components/icons'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Spinner } from '@/components/ui/spinner'
import { Switch } from '@/components/ui/switch'
import { useAppComuPolicy, useComuPermissions, useProjectComuPolicy } from '@/hooks/use-comu-policy'
import type { ComuPolicyFailure } from '@/lib/comu-policy-api'
import { SettingsCard, SettingsGroup } from './SettingsGroup'
import { SettingsNotice } from './SettingsNotice'
import { SettingsRow, settingsDescriptionId } from './SettingsRow'

export const COMU_ANCHORS = {
  appEnabled: 'settings-comu-enabled',
  appCursor: 'settings-comu-cursor',
  appPermissions: 'settings-comu-permissions',
  appAskApps: 'settings-comu-ask-apps',
  appAskForeground: 'settings-comu-ask-foreground',
  appAskClipboard: 'settings-comu-ask-clipboard',
  appAllowedApps: 'settings-comu-allowed-apps',
  projectAllowedApps: 'settings-comu-project-allowed-apps'
} as const

export function describeComuFailure(failure: ComuPolicyFailure): string {
  switch (failure.reason) {
    case 'policy_store_unavailable':
      return "Computer Use rules aren't available on this host."
    case 'invalid_app_id':
      return failure.message
    case 'limit_exceeded':
      return 'Too many apps are already in this allow-list.'
    case 'policy_read_only':
      return 'These rules were saved by a newer version of Troupe and cannot be edited here.'
    default:
      return failure.message || 'Could not update Computer Use rules.'
  }
}

function Loading(): React.JSX.Element {
  return (
    <div className="flex items-center gap-2 text-xs text-muted-foreground" role="status">
      <Spinner size={14} decorative />
      Loading Computer Use rules…
    </div>
  )
}

function ReadOnlyNotice(): React.JSX.Element {
  return (
    <SettingsNotice>
      These rules were saved by a newer version of Troupe, or their file is damaged. You can read
      them but not change them.
    </SettingsNotice>
  )
}

interface SwitchRowProps {
  id: string
  controlId: string
  label: string
  description: string
  checked: boolean
  disabled: boolean
  onToggle: (enabled: boolean) => void
}

function SwitchRow({
  id,
  controlId,
  label,
  description,
  checked,
  disabled,
  onToggle
}: SwitchRowProps): React.JSX.Element {
  return (
    <SettingsRow
      id={id}
      htmlFor={controlId}
      label={label}
      description={description}
      layout="inline"
      disabled={disabled}
    >
      <Switch
        id={controlId}
        aria-describedby={settingsDescriptionId(controlId)}
        checked={checked}
        disabled={disabled}
        onCheckedChange={onToggle}
      />
    </SettingsRow>
  )
}

interface AllowedAppsEditorProps {
  id: string
  label: string
  description: string
  entries: string[]
  disabled: boolean
  onAdd: (app: string) => Promise<ComuPolicyFailure | null>
  onRemove: (app: string) => Promise<ComuPolicyFailure | null>
}

function AllowedAppsEditor({
  id,
  label,
  description,
  entries,
  disabled,
  onAdd,
  onRemove
}: AllowedAppsEditorProps): React.JSX.Element {
  const [draft, setDraft] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const runEdit = async (fn: () => Promise<ComuPolicyFailure | null>): Promise<boolean> => {
    setBusy(true)
    try {
      const failure = await fn()
      setError(failure ? describeComuFailure(failure) : null)
      return failure === null
    } finally {
      setBusy(false)
    }
  }

  return (
    <SettingsGroup id={id} title={label} description={description} className="scroll-mt-4">
      <SettingsCard>
        {entries.length === 0 ? (
          <p className="px-4 py-3 text-xs text-muted-foreground">None yet.</p>
        ) : (
          <ul aria-label={`${label} list`} className="divide-y divide-border">
            {entries.map((entry) => (
              <li key={entry} className="flex min-w-0 items-center gap-3 px-4 py-2.5">
                <span className="min-w-0 flex-1 truncate font-mono text-xs text-foreground">
                  {entry}
                </span>
                <Button
                  type="button"
                  size="icon-sm"
                  variant="ghost"
                  className="size-7 text-muted-foreground"
                  disabled={disabled || busy}
                  onClick={() => void runEdit(() => onRemove(entry))}
                  aria-label={`Remove ${entry}`}
                >
                  <X size={14} />
                </Button>
              </li>
            ))}
          </ul>
        )}
        <form
          className="flex flex-col gap-2 border-t border-border px-4 py-3 sm:flex-row"
          onSubmit={(event) => {
            event.preventDefault()
            const trimmed = draft.trim()
            if (!trimmed || disabled || busy) return
            void runEdit(() => onAdd(trimmed)).then((ok) => {
              if (ok) setDraft('')
            })
          }}
        >
          <Input
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            placeholder="com.apple.Simulator or Calculator"
            aria-label={`${label} entry`}
            disabled={disabled || busy}
            className="h-8 flex-1 text-xs"
          />
          <Button
            type="submit"
            size="sm"
            variant="secondary"
            disabled={disabled || busy || draft.trim().length === 0}
          >
            <Plus size={14} />
            Add
          </Button>
        </form>
      </SettingsCard>
      {error && (
        <SettingsNotice tone="danger" role="alert">
          {error}
        </SettingsNotice>
      )}
    </SettingsGroup>
  )
}

export function ComuProjectPolicy({ projectId }: { projectId: string }): React.JSX.Element {
  const { state, edit } = useProjectComuPolicy(projectId)

  if (state.status === 'loading') return <Loading />
  if (state.status === 'error') {
    return (
      <SettingsNotice tone="danger" role="alert">
        {describeComuFailure(state.failure)}
      </SettingsNotice>
    )
  }

  const { policy } = state
  return (
    <>
      {policy.readOnly && <ReadOnlyNotice />}
      <AllowedAppsEditor
        id={COMU_ANCHORS.projectAllowedApps}
        label="Allowed apps in this project"
        description="Agents operate these apps in this project without asking each chat. Enter a bundle identifier (macOS), AUMID (Windows), or app name."
        entries={policy.allowedApps}
        disabled={policy.readOnly}
        onAdd={(app) => edit({ op: 'add_app', app })}
        onRemove={(app) => edit({ op: 'remove_app', app })}
      />
    </>
  )
}

export function ComuAppPolicy(): React.JSX.Element {
  const { state, edit } = useAppComuPolicy()
  const {
    status: osPerms,
    loading: permsLoading,
    recheck,
    request
  } = useComuPermissions(state.status === 'ready')
  const [switchError, setSwitchError] = useState<string | null>(null)

  if (state.status === 'loading') return <Loading />
  if (state.status === 'error') {
    return (
      <SettingsNotice tone="danger" role="alert">
        {describeComuFailure(state.failure)}
      </SettingsNotice>
    )
  }

  const { policy } = state
  const locked = policy.readOnly
  const settle = (failure: ComuPolicyFailure | null): void => {
    setSwitchError(failure ? describeComuFailure(failure) : null)
  }

  return (
    <>
      {locked && <ReadOnlyNotice />}
      {switchError && (
        <SettingsNotice tone="danger" role="alert">
          {switchError}
        </SettingsNotice>
      )}

      <SettingsGroup title="General" className="scroll-mt-4">
        <SettingsCard>
          <SwitchRow
            id={COMU_ANCHORS.appEnabled}
            controlId="comu-enabled"
            label="Enable Computer Use (Troupe Comu)"
            description="Let agents inspect and operate native desktop apps on this machine using the comu_* tools. Troupe itself is always off-limits."
            checked={policy.enabled}
            disabled={locked}
            onToggle={(enabled) => {
              void edit({ op: 'set_enabled', enabled }).then(settle)
            }}
          />
          <SwitchRow
            id={COMU_ANCHORS.appCursor}
            controlId="comu-cursor-enabled"
            label="Show virtual agent cursor"
            description="Draw a click-through agent cursor badge over target windows while an agent clicks, drags, or types."
            checked={policy.cursorEnabled}
            disabled={locked || !policy.enabled}
            onToggle={(enabled) => {
              void edit({ op: 'set_cursor_enabled', enabled }).then(settle)
            }}
          />
        </SettingsCard>
      </SettingsGroup>

      {osPerms && (
        <SettingsGroup
          id={COMU_ANCHORS.appPermissions}
          title="System permissions"
          description="Operating native apps requires OS-level accessibility and screen capture access on this host."
          className="scroll-mt-4"
        >
          <SettingsCard>
            <SettingsRow
              label={osPerms.ready ? 'System access ready' : 'System access needed'}
              description={osPerms.summary}
              layout="inline"
            >
              <div className="flex flex-wrap items-center gap-2">
                {osPerms.platform === 'macos' && !osPerms.accessibilityGranted && (
                  <Button
                    type="button"
                    size="xs"
                    variant="secondary"
                    disabled={permsLoading}
                    onClick={() => void request('accessibility')}
                  >
                    <ShieldCheck size={13} />
                    Grant Accessibility
                  </Button>
                )}
                {osPerms.platform === 'macos' && !osPerms.screenRecordingGranted && (
                  <Button
                    type="button"
                    size="xs"
                    variant="secondary"
                    disabled={permsLoading}
                    onClick={() => void request('screen_recording')}
                  >
                    <ShieldCheck size={13} />
                    Grant Screen Recording
                  </Button>
                )}
                <Button
                  type="button"
                  size="xs"
                  variant="outline"
                  disabled={permsLoading}
                  onClick={() => void recheck()}
                >
                  <RefreshCw size={13} />
                  Recheck
                </Button>
              </div>
            </SettingsRow>
          </SettingsCard>
        </SettingsGroup>
      )}

      <SettingsGroup title="Ask first" className="scroll-mt-4">
        <SettingsCard>
          <SwitchRow
            id={COMU_ANCHORS.appAskApps}
            controlId="comu-ask-apps"
            label="Ask before operating an app"
            description="On: an agent asks in chat before driving an app that isn't on an allow-list."
            checked={policy.askBeforeDrivingApps}
            disabled={locked}
            onToggle={(enabled) => {
              void edit({ op: 'set_ask_before_driving_apps', enabled }).then(settle)
            }}
          />
          <SwitchRow
            id={COMU_ANCHORS.appAskForeground}
            controlId="comu-ask-foreground"
            label="Ask before foreground or full-desktop actions"
            description="On: an agent asks before raising windows to the front, capturing the whole desktop, or moving the real OS cursor."
            checked={policy.askBeforeForegroundDesktop}
            disabled={locked}
            onToggle={(enabled) => {
              void edit({ op: 'set_ask_before_foreground_desktop', enabled }).then(settle)
            }}
          />
          <SwitchRow
            id={COMU_ANCHORS.appAskClipboard}
            controlId="comu-ask-clipboard"
            label="Ask before reading clipboard text"
            description="On: an agent asks each time it requests plain text from your system clipboard."
            checked={policy.askBeforeClipboardRead}
            disabled={locked}
            onToggle={(enabled) => {
              void edit({ op: 'set_ask_before_clipboard_read', enabled }).then(settle)
            }}
          />
        </SettingsCard>
      </SettingsGroup>

      <AllowedAppsEditor
        id={COMU_ANCHORS.appAllowedApps}
        label="Allowed apps in every project"
        description="Agents operate these apps without asking across all projects. Troupe itself can never be added."
        entries={policy.allowedApps}
        disabled={locked}
        onAdd={(app) => edit({ op: 'add_app', app })}
        onRemove={(app) => edit({ op: 'remove_app', app })}
      />
    </>
  )
}
