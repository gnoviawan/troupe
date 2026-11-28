import { describe, expect, it } from 'vitest'
import { searchSettings } from '@/lib/settings-search'
import { appPrefCategories, appPrefSearchIndex } from './categories'

const APP_PREF_SEARCH_INDEX = appPrefSearchIndex()

describe('appPrefCategories', () => {
  it('lists the Settings pages in rail order', () => {
    expect(appPrefCategories().map((category) => category.id)).toEqual([
      'preferences',
      'terminal',
      'terminal-profiles',
      'provider',
      'ai-agents',
      'subagents',
      'skills',
      'mcp-servers',
      'browser',
      'comu',
      'shortcuts',
      'about'
    ])
  })

  it('points every search entry at a page that exists', () => {
    const ids = new Set(appPrefCategories().map((category) => category.id))
    for (const entry of APP_PREF_SEARCH_INDEX) {
      expect(ids.has(entry.categoryId)).toBe(true)
    }
  })
})

// L-28: the screen reader switch is reachable from the settings search box.
describe('APP_PREF_SEARCH_INDEX screen reader mode (L-28)', () => {
  it.each([
    'screen reader',
    'talkback',
    'voiceover',
    'a11y',
    'accessibility'
  ])('ranks "Screen reader mode" first on the Terminal page for "%s"', (query) => {
    const results = searchSettings(query, APP_PREF_SEARCH_INDEX)

    expect(results[0]).toEqual(
      expect.objectContaining({ categoryId: 'terminal', label: 'Screen reader mode' })
    )
  })

  it('has exactly one Screen reader mode entry, on the Terminal page', () => {
    const entries = APP_PREF_SEARCH_INDEX.filter((entry) => entry.label === 'Screen reader mode')

    expect(entries).toHaveLength(1)
    expect(entries[0]?.categoryId).toBe('terminal')
    expect(entries[0]?.description).toBe('Make terminal output readable by screen readers.')
    expect(entries[0]?.keywords).toEqual(
      expect.arrayContaining(['accessibility', 'a11y', 'talkback', 'voiceover', 'nvda'])
    )
  })
})

describe('APP_PREF_SEARCH_INDEX sub-agent profiles', () => {
  it.each([
    ['sub-agent profile', 'Sub-agent profiles'],
    ['thinking', 'Sub-agent profiles'],
    ['effort', 'Sub-agent profiles'],
    ['bypass', 'Sub-agent approvals'],
    ['approvals', 'Sub-agent approvals']
  ])('finds "%s" on the Sub-agents page as %s', (query, label) => {
    const results = searchSettings(query, APP_PREF_SEARCH_INDEX)

    expect(results[0]).toEqual(
      expect.objectContaining({
        categoryId: 'subagents',
        label,
        anchorId: 'setting-subagent-profiles'
      })
    )
  })
})
