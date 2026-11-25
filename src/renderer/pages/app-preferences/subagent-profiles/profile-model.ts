import type { PendingLauncherOptions } from '@/components/agents/pending-launcher-options'
import { MODEL_CATEGORY } from '@/components/chat/chat-input-bar-config'
import { modeKind } from '@/components/chat/mode-menu'
import type {
  SessionConfigOption,
  SessionModelState,
  SessionModeState,
  SubagentProfile,
  SubagentProfileView
} from '@/lib/acp-api'

/** The host's profile id rule: 1 to 64 of `[A-Za-z0-9._:-]`. */
const PROFILE_ID_PATTERN = /^[A-Za-z0-9._:-]{1,64}$/
/** The host's `MAX_DESCRIPTION_CHARS`. */
export const MAX_DESCRIPTION_CHARS = 1000
const REGISTRY_PREFIX = 'acp-registry:'

/** The profile as the host stores it: the view without its derived fields. */
export function storedProfile(view: SubagentProfileView): SubagentProfile {
  const { agentName: _agentName, implicit: _implicit, ...profile } = view
  return profile
}

/** Why `id` cannot name a profile next to `others`, or null when it can. */
export function profileIdError(id: string, others: readonly SubagentProfileView[]): string | null {
  if (id.length === 0) return 'Enter a name.'
  if (id.length > 64) return 'Use 64 characters or fewer.'
  if (!PROFILE_ID_PATTERN.test(id)) return 'Use letters, digits, and . _ : - only.'
  const lower = id.toLowerCase()
  if (others.some((other) => other.id.toLowerCase() === lower)) {
    return 'Another profile has this name.'
  }
  return null
}

/** `base` with the first free `-N` suffix (from 2), unique ignoring case. */
export function uniqueProfileId(base: string, taken: readonly SubagentProfileView[]): string {
  const stem = base.replace(/-\d+$/, '').slice(0, 60) || 'agent'
  const used = new Set(taken.map((profile) => profile.id.toLowerCase()))
  for (let n = 2; ; n++) {
    const candidate = `${stem}-${n}`
    if (!used.has(candidate.toLowerCase())) return candidate
  }
}

/** `acp-registry:claude-acp` reads as `claude`: the slug new profile ids start from. */
export function agentShortName(agentKey: string): string {
  const bare = agentKey.startsWith(REGISTRY_PREFIX)
    ? agentKey.slice(REGISTRY_PREFIX.length)
    : agentKey
  const tail = bare.slice(bare.lastIndexOf(':') + 1).replace(/-acp$/, '')
  return tail.toLowerCase().replace(/[^a-z0-9._-]+/g, '-') || 'agent'
}

/**
 * An agent's only profile cannot be deleted: its implicit default would come
 * straight back. A profile whose agent is gone can always go.
 */
export function canDeleteProfile(
  profile: SubagentProfileView,
  all: readonly SubagentProfileView[]
): boolean {
  if (profile.implicit) return false
  if (profile.agentName === null) return true
  return all.filter((other) => other.agentId === profile.agentId).length > 1
}

/** The profile runs without asking: a bypass-type mode, or Allow everything. */
export function profileSkipsPrompts(profile: SubagentProfile): boolean {
  return profile.approvals === 'allowAll' || modeKind(profile.mode ?? undefined).risk === 'danger'
}

/** The profile's picks in the launcher's pending-options shape. */
export function pendingOf(profile: SubagentProfile): PendingLauncherOptions {
  return {
    modelId: profile.model ?? undefined,
    modeId: profile.mode ?? undefined,
    configValues: profile.configOptions
  }
}

/** `profile` with the picks of `pending`. */
export function withPending(
  profile: SubagentProfile,
  pending: PendingLauncherOptions
): SubagentProfile {
  return {
    ...profile,
    model: pending.modelId ?? null,
    mode: pending.modeId ?? null,
    configOptions: pending.configValues
  }
}

/** `profile` moved to another agent: that agent's own model, mode and options. */
export function withAgent(
  profile: SubagentProfile,
  agentId: string,
  pick?: { modelId: string; configValues: Record<string, string> }
): SubagentProfile {
  return {
    ...profile,
    agentId,
    model: pick?.modelId ?? null,
    mode: null,
    configOptions: pick?.configValues ?? {}
  }
}

/**
 * The agent's options with the profile's saved model and mode added when the
 * agent does not list them (any more), so a saved value stays visible.
 */
export function withSavedValues(
  options: {
    models: SessionModelState | null
    modes: SessionModeState | null
    configOptions: SessionConfigOption[]
  },
  profile: SubagentProfile
): {
  models: SessionModelState | null
  modes: SessionModeState | null
  configOptions: SessionConfigOption[]
} {
  let { models, modes, configOptions } = options
  const { model, mode } = profile
  if (model) {
    const modelOption = configOptions.find((option) => option.category === MODEL_CATEGORY)
    if (modelOption) {
      const listed = (modelOption.options ?? []).some(
        (entry) => entry.value === model || entry.options?.some((value) => value.value === model)
      )
      if (!listed) {
        configOptions = configOptions.map((option) =>
          option === modelOption
            ? { ...option, options: [...(option.options ?? []), { value: model, name: model }] }
            : option
        )
      }
    } else if (!models?.availableModels.some((entry) => entry.modelId === model)) {
      models = {
        currentModelId: model,
        availableModels: [...(models?.availableModels ?? []), { modelId: model, name: model }]
      }
    }
  }
  if (mode && !modes?.availableModes.some((entry) => entry.id === mode)) {
    modes = {
      currentModeId: mode,
      availableModes: [...(modes?.availableModes ?? []), { id: mode, name: mode }]
    }
  }
  return { models, modes, configOptions }
}

/**
 * `profiles` with `profile` saved in place of `previousId` (else its own id),
 * or appended: what the host will answer, shown before it does so the next
 * edit builds on this one.
 */
export function upsertProfileView(
  profiles: readonly SubagentProfileView[],
  profile: SubagentProfile,
  previousId?: string
): SubagentProfileView[] {
  const target = previousId ?? profile.id
  const agentName = profiles.find((other) => other.agentId === profile.agentId)?.agentName ?? null
  const next: SubagentProfileView = { ...profile, agentName, implicit: false }
  const at = profiles.findIndex((other) => other.id === target)
  return at >= 0
    ? profiles.map((other, index) => (index === at ? next : other))
    : [...profiles, next]
}
