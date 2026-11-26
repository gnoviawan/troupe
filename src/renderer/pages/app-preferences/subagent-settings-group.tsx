import { useCallback, useMemo, useState } from 'react'
import { Plus } from '@/components/icons'
import { rowId } from '@/components/settings/row-id'
import { SettingsCard, SettingsGroup } from '@/components/settings/SettingsGroup'
import { SettingsNotice } from '@/components/settings/SettingsNotice'
import { SettingsRow } from '@/components/settings/SettingsRow'
import { SettingsStepper } from '@/components/settings/SettingsStepper'
import { SettingsSwitchRow } from '@/components/settings/SettingsSwitchRow'
import { Button } from '@/components/ui/button'
import type { SubagentProfile } from '@/lib/acp-api'
import { ProfileRow, type SaveProfile } from './subagent-profiles/ProfileRow'
import {
  agentShortName,
  profileUpserts,
  storedProfile,
  uniqueProfileId
} from './subagent-profiles/profile-model'
import { useSubagentSettings } from './subagent-profiles/use-subagent-settings'

const MIN_RUNNING = 1
const MAX_RUNNING = 8
const MIN_DEPTH = 1
const MAX_DEPTH = 4

const noop = (): void => {}

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
 * thinking, mode, approvals and note. Edits show at once and reach the host
 * in order; its answers stay the source of truth.
 */
export function SubagentSettingsGroup(): React.JSX.Element {
  const { view, failed, change } = useSubagentSettings()
  const [expandedId, setExpandedId] = useState<string | null>(null)

  const save = useCallback<SaveProfile>(
    (id, edit) =>
      change((latest) => {
        const current = latest.profiles.find((profile) => profile.id === id)
        if (!current) return null
        const next = edit(storedProfile(current))
        const previousId = next.id === id ? undefined : id
        return { upsertProfiles: profileUpserts(latest.profiles, next, previousId) }
      }),
    [change]
  )
  const rename = useCallback(
    (from: string, to: string) => {
      setExpandedId((open) => (open === from ? to : open))
      void save(from, (profile) => ({ ...profile, id: to })).catch(() =>
        setExpandedId((open) => (open === to ? from : open))
      )
    },
    [save]
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
    setExpandedId(id)
    void change((latest) => ({ upsertProfiles: profileUpserts(latest.profiles, profile) })).catch(
      () => setExpandedId((open) => (open === id ? null : open))
    )
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
            onToggle={(enabled) => void change(() => ({ enabled })).catch(noop)}
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
              onValueChange={(maxRunning) => void change(() => ({ maxRunning })).catch(noop)}
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
              onValueChange={(maxDepth) => void change(() => ({ maxDepth })).catch(noop)}
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
                rename={rename}
                onDuplicate={() =>
                  addProfile(
                    profile.agentId,
                    profile.implicit ? agentShortName(profile.agentId) : profile.id,
                    storedProfile(profile)
                  )
                }
                onDelete={() => {
                  void change(() => ({ deleteProfiles: [profile.id] })).catch(noop)
                }}
              />
            ))
          )}
        </SettingsCard>
      </SettingsGroup>
    </>
  )
}
