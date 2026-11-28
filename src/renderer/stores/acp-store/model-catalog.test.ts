import { beforeEach, describe, expect, it, vi } from 'vitest'

const { store, api, mockLog } = vi.hoisted(() => {
  const store = new Map<string, unknown>()
  return {
    store,
    mockLog: vi.fn(async () => {}),
    api: {
      writeFails: false,
      read: vi.fn(async (key: string) =>
        store.has(key)
          ? { success: true, data: store.get(key) }
          : { success: false, code: 'KEY_NOT_FOUND', error: `Key not found: ${key}` }
      ),
      writeDebounced: vi.fn(async (key: string, data: unknown) => {
        if (api.writeFails) return { success: false, code: 'WRITE_ERROR', error: 'disk full' }
        store.set(key, data)
        return { success: true, data: undefined }
      })
    }
  }
})

vi.mock('@/lib/api', () => ({ persistenceApi: api }))
vi.mock('@/lib/log-api', () => ({ logFrontendError: mockLog }))

import {
  hydrateOptionsCacheFromCatalog,
  modelCatalogFromOptions,
  persistModelCatalog,
  readModelCatalog
} from './model-catalog'
import type { AcpStoreState, AgentOptionsCacheEntry } from './types'

const MODEL_OPTION = {
  id: 'model',
  name: 'Model',
  category: 'model',
  type: 'select',
  currentValue: 'opus',
  options: [{ value: 'opus', name: 'Opus' }]
}

const MODES = {
  currentModeId: 'default',
  availableModes: [
    { id: 'default', name: 'Default' },
    { id: 'plan', name: 'Plan', description: 'Read-only planning' }
  ]
}

describe('model catalog persistence', () => {
  beforeEach(() => {
    store.clear()
    api.writeFails = false
    vi.clearAllMocks()
  })

  it('keeps only the model list from an options-cache entry', () => {
    const catalog = modelCatalogFromOptions({
      models: null,
      modes: null,
      configOptions: [MODEL_OPTION, { ...MODEL_OPTION, id: 'effort', category: 'thought_level' }]
    })
    expect(catalog?.modelOption?.id).toBe('model')
    expect(catalog?.models).toBeNull()
    expect(catalog?.modes).toBeNull()
  })

  it('saves the models inside grouped select options', () => {
    const catalog = modelCatalogFromOptions({
      models: null,
      modes: null,
      configOptions: [
        {
          ...MODEL_OPTION,
          currentValue: 'gemini-flash',
          options: [
            { value: 'gemini-flash', name: 'Gemini Flash' },
            {
              group: 'anthropic',
              name: 'Anthropic',
              options: [{ value: 'claude-sonnet', name: 'Claude Sonnet' }]
            }
          ]
        }
      ]
    })
    expect(catalog?.modelOption?.options).toEqual([
      { value: 'gemini-flash', name: 'Gemini Flash', description: null },
      { value: 'claude-sonnet', name: 'Claude Sonnet', description: null }
    ])
  })

  it('returns null when models, model option, and modes are all absent', () => {
    expect(modelCatalogFromOptions({ models: null, modes: null, configOptions: [] })).toBeNull()
    expect(
      modelCatalogFromOptions({
        models: null,
        modes: { currentModeId: 'default', availableModes: [] },
        configOptions: []
      })
    ).toBeNull()
  })

  it('round-trips a saved list per config', async () => {
    const catalog = modelCatalogFromOptions({
      models: null,
      modes: null,
      configOptions: [MODEL_OPTION]
    })
    if (!catalog) throw new Error('expected a catalog')
    persistModelCatalog('acp-registry:cursor', catalog)
    await vi.waitFor(() => expect(store.size).toBe(1))
    expect(api.writeDebounced).toHaveBeenCalledWith(
      'agents/model-catalog/acp-registry:cursor',
      catalog
    )
    expect(await readModelCatalog('acp-registry:cursor')).toEqual(catalog)
  })

  it('round-trips a catalog with only modes from save through hydrate', async () => {
    const config = {
      id: 'modes-only-agent',
      name: 'Modes Only',
      command: 'agent',
      args: [],
      env: {}
    }
    const catalog = modelCatalogFromOptions({ models: null, modes: MODES, configOptions: [] })
    expect(catalog).toEqual({
      models: null,
      modelOption: null,
      modes: {
        currentModeId: 'default',
        availableModes: [
          { id: 'default', name: 'Default', description: null },
          { id: 'plan', name: 'Plan', description: 'Read-only planning' }
        ]
      },
      updatedAt: expect.any(Number)
    })
    if (!catalog) throw new Error('expected a catalog')

    persistModelCatalog(config.id, catalog)
    await vi.waitFor(() => expect(store.size).toBe(1))

    let state = {
      agentConfigs: [config],
      agentOptionsCache: {} as Record<string, AgentOptionsCacheEntry>
    }
    hydrateOptionsCacheFromCatalog((updater) => {
      const patch =
        typeof updater === 'function' ? updater(state as unknown as AcpStoreState) : updater
      state = { ...state, ...(patch as Partial<typeof state>) }
    }, config)

    await vi.waitFor(() =>
      expect(state.agentOptionsCache[config.id]).toEqual({
        models: null,
        modes: {
          currentModeId: 'default',
          availableModes: [
            { id: 'default', name: 'Default', description: null },
            { id: 'plan', name: 'Plan', description: 'Read-only planning' }
          ]
        },
        configOptions: [],
        updatedAt: catalog.updatedAt
      })
    )
  })

  it('hydrates an old saved catalog without modes as modes: null', async () => {
    const config = {
      id: 'legacy-catalog-agent',
      name: 'Legacy',
      command: 'agent',
      args: [],
      env: {}
    }
    store.set(`agents/model-catalog/${config.id}`, {
      models: null,
      modelOption: {
        id: 'model',
        name: 'Model',
        currentValue: 'opus',
        options: [{ value: 'opus', name: 'Opus', description: null }]
      },
      updatedAt: 42
    })

    let state = {
      agentConfigs: [config],
      agentOptionsCache: {} as Record<string, AgentOptionsCacheEntry>
    }
    hydrateOptionsCacheFromCatalog((updater) => {
      const patch =
        typeof updater === 'function' ? updater(state as unknown as AcpStoreState) : updater
      state = { ...state, ...(patch as Partial<typeof state>) }
    }, config)

    await vi.waitFor(() =>
      expect(state.agentOptionsCache[config.id]).toEqual({
        models: null,
        modes: null,
        configOptions: [
          {
            id: 'model',
            name: 'Model',
            category: 'model',
            type: 'select',
            currentValue: 'opus',
            options: [{ value: 'opus', name: 'Opus', description: null }]
          }
        ],
        updatedAt: 42
      })
    )
  })

  it('reads null without a log when nothing is saved', async () => {
    expect(await readModelCatalog('unknown')).toBeNull()
    expect(mockLog).not.toHaveBeenCalled()
  })

  it('logs a failed write and never throws', async () => {
    api.writeFails = true
    const catalog = modelCatalogFromOptions({
      models: null,
      modes: null,
      configOptions: [MODEL_OPTION]
    })
    if (!catalog) throw new Error('expected a catalog')
    expect(() => persistModelCatalog('c', catalog)).not.toThrow()
    await vi.waitFor(() => expect(mockLog).toHaveBeenCalled())
    expect(mockLog).toHaveBeenCalledWith(
      expect.objectContaining({ level: 'warn', source: 'acp-store.persistModelCatalog' })
    )
  })
})
