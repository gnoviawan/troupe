import { useCallback, useEffect, useMemo, useState } from 'react'
import { toast } from 'sonner'
import { Plus } from '@/components/icons'
import { rowId } from '@/components/settings/row-id'
import { SettingsCard, SettingsGroup } from '@/components/settings/SettingsGroup'
import { SettingsNotice } from '@/components/settings/SettingsNotice'
import { SettingsRow } from '@/components/settings/SettingsRow'
import { SettingsStepper } from '@/components/settings/SettingsStepper'
import { SettingsSwitchRow } from '@/components/settings/SettingsSwitchRow'
import { Button } from '@/components/ui/button'
import type { SubagentProfile, SubagentSettingsInput, SubagentSettingsView } from '@/lib/acp-api'
import { acpApi } from '@/lib/api'
import { logFrontendError } from '@/lib/log-api'
import { ProfileRow, type SaveProfile } from './subagent-profiles/ProfileRow'
import {
  agentShortName,
  storedProfile,
  uniqueProfileId,
  upsertProfileView
} from './subagent-profiles/profile-model'

const MIN_RUNNING = 1
const MAX_RUNNING = 8
const MIN_DEPTH = 1
const MAX_DEPTH = 4

function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error)
}

/** "a, b and c" */
function listIds(ids: readonly string[]): React.ReactNode {
  return ids.map((id, index) => (
    <span key={id}>
      {index === 0 ? '' : index === ids.length - 1 ? ' and ' : ', '}
      <code className="font-mono">{id}</code>
    </span>
  ))
}

/**
 * The Sub-agents preferences: the orchestrator's enable switch and caps, and
 * the profiles the calling agent can start, each with its agent, model,
 * thinking, mode, approvals and note. Every write applies the view the host
 * answers with, so the host stays the single source of truth.
 */
export function SubagentSettingsGroup(): React.JSX.Element {
  const [view, setView] = useState<SubagentSettingsView | null>(null)
  const [failed, setFailed] = useState(false)
  const [expandedId, setExpandedId] = useState<string | null>(null)

  const load = useCallback((isCurrent: () => boolean = () => true) => {
    void acpApi
      .getSubagentSettings()
      .then((next) => {
        if (!isCurrent()) return
        setView(next)
        setFailed(false)
      })
      .catch((error: unknown) => {
        if (!isCurrent()) return
        setView(null)
        setFailed(true)
        void logFrontendError({
          level: 'warn',
          source: 'SubagentSettingsGroup',
          message: `Failed to read the sub-agent settings: ${errorText(error)}`
        })
      })
  }, [])

  useEffect(() => {
    let cancelled = false
    load(() => !cancelled)
    return () => {
      cancelled = true
    }
  }, [load])

  // A refused write reads the host's view back, dropping what was shown early.
  const write = useCallback(
    async (input: SubagentSettingsInput): Promise<void> => {
      try {
        setView(await acpApi.setSubagentSettings(input))
      } catch (error) {
        toast.error('Could not save the sub-agent setting.', { description: errorText(error) })
        void logFrontendError({
          level: 'warn',
          source: 'SubagentSettingsGroup',
          message: `Failed to set subagent settings: ${errorText(error)}`
        })
        load()
        throw error
      }
    },
    [load]
  )

  const save = useCallback<SaveProfile>(
    async (profile, previousId) => {
      setView(
        (prev) =>
          prev && { ...prev, profiles: upsertProfileView(prev.profiles, profile, previousId) }
      )
      if (previousId) setExpandedId((open) => (open === previousId ? profile.id : open))
      await write({ upsertProfiles: [{ previousId, profile }] })
    },
    [write]
  )

  const profiles = view?.profiles
  const launchableAgentIds = useMemo(
    () =>
      new Set(
        (profiles ?? [])
          .filter((profile) => profile.agentName !== null)
          .map((profile) => profile.agentId)
      ),
    [profiles]
  )

  if (failed) {
    return (
      <SettingsGroup title="Sub-agents">
        <SettingsNotice>
          Could not read the sub-agent settings. Agent chats keep working.
        </SettingsNotice>
      </SettingsGroup>
    )
  }
  if (!view || !profiles) {
    return (
      <SettingsGroup title="Sub-agents">
        <SettingsNotice>Loading sub-agent settings…</SettingsNotice>
      </SettingsGroup>
    )
  }

  /** Adds a profile of `agentId` (a copy of `base` when given) named after `stem`, and opens it. */
  const addProfile = (agentId: string, stem: string, base: SubagentProfile | null): void => {
    const id = uniqueProfileId(stem, profiles)
    const profile: SubagentProfile = base
      ? { ...base, id }
      : {
          id,
          agentId,
          enabled: true,
          model: null,
          mode: null,
          configOptions: {},
          approvals: 'ask',
          description: null
        }
    void write({ upsertProfiles: [{ profile }] })
      .then(() => setExpandedId(id))
      .catch(() => {})
  }
  const firstAgent = profiles.find((profile) => profile.agentName !== null)?.agentId ?? null
  const enabledIds = profiles
    .filter((profile) => profile.enabled && profile.agentName !== null)
    .map((profile) => profile.id)

  return (
    <>
      <SettingsGroup title="General">
        <SettingsCard>
          <SettingsSwitchRow
            id="subagent-enabled"
            label="Let agents start other agents"
            description="An agent chat can hand parts of a task to the profiles below. Their approvals come to that chat."
            checked={view.enabled}
            onToggle={(enabled) => void write({ enabled }).catch(() => {})}
          />
          <SettingsRow
            id={rowId('subagent-max-running')}
            describes="subagent-max-running"
            label="Max running sub-agents"
            description="Per chat. Extra starts are refused until one finishes."
          >
            <SettingsStepper
              value={view.maxRunning}
              min={MIN_RUNNING}
              max={MAX_RUNNING}
              decrementLabel="Fewer running sub-agents"
              incrementLabel="More running sub-agents"
              onValueChange={(maxRunning) => void write({ maxRunning }).catch(() => {})}
            />
          </SettingsRow>
          <SettingsRow
            id={rowId('subagent-max-depth')}
            describes="subagent-max-depth"
            label="Max depth"
            description="How many levels of sub-agents can start their own sub-agents. 1 means sub-agents cannot start more."
          >
            <SettingsStepper
              value={view.maxDepth}
              min={MIN_DEPTH}
              max={MAX_DEPTH}
              decrementLabel="Less depth"
              incrementLabel="More depth"
              onValueChange={(maxDepth) => void write({ maxDepth }).catch(() => {})}
            />
          </SettingsRow>
        </SettingsCard>
      </SettingsGroup>
      <SettingsGroup
        id={rowId('subagent-profiles')}
        title={`Profiles · ${profiles.length} · ${enabledIds.length} on`}
        actions={
          <Button
            size="sm"
            disabled={firstAgent === null}
            onClick={() => firstAgent && addProfile(firstAgent, agentShortName(firstAgent), null)}
          >
            <Plus size={14} aria-hidden="true" />
            Add profile
          </Button>
        }
        footnote={
          enabledIds.length > 0 ? (
            <>
              The calling agent sees {listIds(enabledIds)} with their notes. Off profiles are hidden
              from it.
            </>
          ) : (
            'Every profile is off, so the calling agent sees none.'
          )
        }
      >
        <SettingsCard>
          {profiles.length === 0 ? (
            <SettingsRow
              label="No agents yet"
              description="Install an agent and its default profile appears here."
            />
          ) : (
            profiles.map((profile) => (
              <ProfileRow
                key={profile.id}
                profile={profile}
                profiles={profiles}
                launchableAgentIds={launchableAgentIds}
                expanded={expandedId === profile.id}
                onToggle={() => setExpandedId((open) => (open === profile.id ? null : profile.id))}
                save={save}
                onDuplicate={() =>
                  addProfile(
                    profile.agentId,
                    profile.implicit ? agentShortName(profile.agentId) : profile.id,
                    storedProfile(profile)
                  )
                }
                onDelete={() => {
                  void write({ deleteProfiles: [profile.id] }).catch(() => {})
                }}
              />
            ))
          )}
        </SettingsCard>
      </SettingsGroup>
    </>
  )
}
