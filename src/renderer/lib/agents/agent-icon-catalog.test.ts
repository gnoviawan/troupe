import { afterEach, describe, expect, it, vi } from 'vitest'
import claudeSvg from '@/assets/agent-icons/acp/claude-acp.svg?raw'
import bundledOverlay from '@/assets/agent-icons/acp/troupe.json'
import { buildAgentIconCatalog, getAgentIconCatalog } from '@/lib/agents/agent-icon-catalog'
import { indexAgentOverlay, setAgentOverlay } from '@/lib/agents/agent-overlay'

vi.mock('@/lib/log-api', () => ({ logFrontendError: vi.fn(async () => undefined) }))

const COLOUR_SVG =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path fill="#FF7A00" d="M0 0h24v24H0z"></path></svg>'
const MONO_SVG =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path fill="currentColor" d="M0 0h24v24H0z"></path></svg>'
const BRAND = { light: '#112233', dark: '#445566' }

const catalogFor = (overrides: unknown) => buildAgentIconCatalog(indexAgentOverlay(overrides))

afterEach(() => setAgentOverlay(bundledOverlay.overrides))

describe('agent icon catalog', () => {
  it('seeds brand colours from the bundled troupe.json before the host answers', () => {
    const catalog = getAgentIconCatalog()
    expect(catalog.byKey('acp:claude-acp')?.brandColor).toEqual({
      light: '#C15F3C',
      dark: '#D97757'
    })
    expect(catalog.byKey('acp:gemini')?.brandColor?.gradient).toEqual({
      light: ['#1C69FF', '#9168C0'],
      dark: ['#4796E3', '#9177C7']
    })
    expect(catalog.byKey('acp:cursor')?.brandColor).toBeUndefined()
  })

  it('takes the overlay name, logo and colour over the bundled ones', () => {
    const catalog = catalogFor([
      { id: 'claude-acp', name: 'Claude', icon: MONO_SVG, brandColor: BRAND }
    ])
    expect(catalog.byKey('acp:claude-acp')).toEqual({
      key: 'acp:claude-acp',
      label: 'Claude',
      svg: MONO_SVG,
      brandColor: BRAND
    })
    expect(catalog.brandColor(null, 'claude-acp')).toEqual(BRAND)
  })

  it('draws a full-colour overlay logo with its own fills, never a brand colour', () => {
    const catalog = catalogFor([{ id: 'claude-acp', icon: COLOUR_SVG, brandColor: BRAND }])
    const entry = catalog.byKey('acp:claude-acp')
    expect(entry?.svg).toBe(COLOUR_SVG)
    expect(entry?.brandColor).toBeUndefined()
    expect(catalog.brandColor(null, 'claude-acp')).toBeNull()
  })

  it('still finds a persisted bundled logo the overlay replaced', () => {
    const catalog = catalogFor([{ id: 'claude-acp', icon: MONO_SVG, brandColor: BRAND }])
    expect(catalog.bySvg(claudeSvg)?.key).toBe('acp:claude-acp')
    expect(catalog.brandColor(claudeSvg, null)).toEqual(BRAND)
  })

  it('adds a logo for an agent the bundle has none for', () => {
    const catalog = catalogFor([{ id: 'oh-my-pi', name: 'Oh My Pi', icon: COLOUR_SVG }])
    expect(catalog.byKey('acp:oh-my-pi')).toEqual({
      key: 'acp:oh-my-pi',
      label: 'Oh My Pi',
      svg: COLOUR_SVG
    })
    expect(catalog.entries.at(-1)?.key).toBe('generic:zap')
  })

  it('keeps the bundled logo when the sanitizer refuses the overlay one', () => {
    const catalog = catalogFor([
      { id: 'claude-acp', icon: '<svg width="4"><path d="M0 0h4v4H0z"/></svg>' },
      { id: 'cursor', icon: '<img src="x" onerror="alert(1)">' }
    ])
    expect(catalog.byKey('acp:claude-acp')?.svg).toBe(claudeSvg)
    expect(catalog.byKey('acp:cursor')?.svg).not.toContain('onerror')
    expect(catalog.byKey('acp:cursor')?.svg).not.toContain('<img')
  })

  it('strips script from an overlay logo it keeps', () => {
    const catalog = catalogFor([
      {
        id: 'cursor',
        icon: '<svg viewBox="0 0 4 4"><script>alert(1)</script><path d="M0 0h4v4H0z"/></svg>'
      }
    ])
    const svg = catalog.byKey('acp:cursor')?.svg ?? ''
    expect(svg).toContain('viewBox="0 0 4 4"')
    expect(svg).not.toContain('script')
  })

  it('drops a malformed brand colour', () => {
    const catalog = catalogFor([
      { id: 'claude-acp', brandColor: { light: 'red', dark: '#000000' } }
    ])
    expect(catalog.byKey('acp:claude-acp')?.brandColor).toBeUndefined()
  })

  it('rebuilds the catalog when the host overlay replaces the seed', () => {
    const seeded = getAgentIconCatalog()
    expect(getAgentIconCatalog()).toBe(seeded)
    setAgentOverlay([{ id: 'claude-acp', name: 'Claude' }])
    const next = getAgentIconCatalog()
    expect(next).not.toBe(seeded)
    expect(next.byKey('acp:claude-acp')?.label).toBe('Claude')
    expect(next.byKey('acp:claude-acp')?.brandColor).toBeUndefined()
  })
})
