import { describe, expect, it } from 'vitest'
import { mcpBrandLogo } from './mcp-brand-logos'

describe('mcpBrandLogo', () => {
  it.each([
    ['io.github.github/github-mcp-server', 'currentColor'],
    ['com.linear/linear', '#5E6AD2'],
    ['io.github.upstash/context7', 'currentColor'],
    ['com.supabase/mcp', '#3ECF8E'],
    ['com.stripe/mcp', '#635BFF']
  ])('finds a logo for %s', (name, fill) => {
    expect(mcpBrandLogo(name)?.fill).toBe(fill)
  })

  it('does not read the io.github namespace as the GitHub brand', () => {
    expect(mcpBrandLogo('io.github.someone/weather-tools')).toBeNull()
  })

  it('has no logo for an unknown server', () => {
    expect(mcpBrandLogo('io.example/plain')).toBeNull()
  })
})
