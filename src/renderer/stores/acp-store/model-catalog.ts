import { type PersistedModelCatalog, PersistenceKeys } from '@shared/types/persistence.types'
import {
  catalogModels,
  optionsFromPersisted
} from '@/components/chat/agent-model-selector/selector-model'
import type { StoredAgentConfig } from '@/lib/acp-agents-persistence'
import { agentConfigIdentityKey } from '@/lib/agents/acp-config-identity'
import { persistenceApi } from '@/lib/api'
import { logFrontendError } from '@/lib/log-api'
import type { AcpSet, AgentOptionsCacheEntry } from './types'

const MODEL_CATEGORY = 'model'

/**
 * The model and mode lists of an options-cache entry, or null when the entry
 * has neither (nothing worth saving).
 */
export function modelCatalogFromOptions(
  entry: Pick<AgentOptionsCacheEntry, 'models' | 'configOptions' | 'modes'>
): PersistedModelCatalog | null {
  const option = entry.configOptions.find(
    (o) =>
      o.category === MODEL_CATEGORY &&
      typeof o.currentValue === 'string' &&
      (o.options?.length ?? 0) > 0
  )
  const models =
    entry.models && entry.models.availableModels.length > 0
      ? {
          currentModelId: entry.models.currentModelId,
          availableModels: entry.models.availableModels.map((m) => ({
            modelId: m.modelId,
            name: m.name,
            description: m.description ?? null
          }))
        }
      : null
  // Re-checking `typeof` narrows `currentValue` for the persisted shape,
  // which only allows a string.
  const modelOption =
    option && typeof option.currentValue === 'string'
      ? {
          id: option.id,
          name: option.name,
          currentValue: option.currentValue,
          options: catalogModels(option)
        }
      : null
  const modes =
    entry.modes && entry.modes.availableModes.length > 0
      ? {
          currentModeId: entry.modes.currentModeId,
          availableModes: entry.modes.availableModes.map((m) => ({
            id: m.id,
            name: m.name,
            description: m.description ?? null
          }))
        }
      : null
  if (!modelOption && !models && !modes) return null
  return {
    models,
    modelOption,
    modes,
    updatedAt: Date.now()
  }
}

/** Save the model list for a config. Fire-and-forget; failures are logged. */
export function persistModelCatalog(configId: string, catalog: PersistedModelCatalog): void {
  const fail = (reason: string): void => {
    void logFrontendError({
      level: 'warn',
      source: 'acp-store.persistModelCatalog',
      message: `persist failed for ${configId}: ${reason}`
    })
  }
  // The options cache is written from store setters; a storage failure here
  // must never break those setters, so this never throws.
  void (async () => {
    try {
      const result = await persistenceApi.writeDebounced(
        PersistenceKeys.agentModelCatalog(configId),
        catalog
      )
      if (!result.success) fail(result.error)
    } catch (err) {
      fail(err instanceof Error ? err.message : String(err))
    }
  })()
}

/** Read the saved model list for a config. Null when none is saved or the read fails. */
export async function readModelCatalog(configId: string): Promise<PersistedModelCatalog | null> {
  try {
    const result = await persistenceApi.read<PersistedModelCatalog>(
      PersistenceKeys.agentModelCatalog(configId)
    )
    if (result.success) return result.data ?? null
    if (result.code !== 'KEY_NOT_FOUND') {
      void logFrontendError({
        level: 'warn',
        source: 'acp-store.readModelCatalog',
        message: `read failed for ${configId}: ${result.error}`
      })
    }
    return null
  } catch (err) {
    void logFrontendError({
      level: 'warn',
      source: 'acp-store.readModelCatalog',
      message: `read failed for ${configId}: ${err instanceof Error ? err.message : String(err)}`
    })
    return null
  }
}

const hydrated = new Set<string>()

/**
 * Fill an agent's options cache from its saved model list, once per app run,
 * so the launcher and the pickers list its models before the agent runs. An
 * entry already in memory (a live session wrote it) is newer and stays. A
 * config whose command, args or env changed during the read runs another
 * agent, so the list is dropped.
 */
export function hydrateOptionsCacheFromCatalog(set: AcpSet, config: StoredAgentConfig): void {
  const configId = config.id
  if (hydrated.has(configId)) return
  hydrated.add(configId)
  const identity = agentConfigIdentityKey(config)
  void readModelCatalog(configId).then((saved) => {
    if (!saved) return
    set((s) => {
      if (s.agentOptionsCache[configId]) return s
      const current = s.agentConfigs.find((c) => c.id === configId)
      if (!current || agentConfigIdentityKey(current) !== identity) return s
      const entry: AgentOptionsCacheEntry = {
        ...optionsFromPersisted(saved),
        modes: saved.modes ?? null,
        updatedAt: saved.updatedAt
      }
      return { agentOptionsCache: { ...s.agentOptionsCache, [configId]: entry } }
    })
  })
}
