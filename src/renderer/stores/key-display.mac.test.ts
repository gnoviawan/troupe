import { describe, expect, it, vi } from 'vitest'

vi.mock('@/lib/platform', () => ({ isMac: true, isWindows: false, isLinux: false }))

import { formatKeyForDisplay, formatKeyParts } from './keyboard-shortcuts-store'

describe('key display on macOS', () => {
  it('shows a lone ctrl as ⌘, because ⌘ triggers it', () => {
    expect(formatKeyParts('ctrl+k')).toEqual(['⌘', 'K'])
    expect(formatKeyForDisplay('ctrl+k')).toBe('⌘K')
  })

  it('keeps ⌃ when ⌘ is held too', () => {
    expect(formatKeyParts('ctrl+cmd+t')).toEqual(['⌃', '⌘', 'T'])
  })

  it("puts the modifiers in Apple's order", () => {
    expect(formatKeyParts('ctrl+shift+alt+n')).toEqual(['⌥', '⇧', '⌘', 'N'])
    expect(formatKeyParts('ctrl+shift+p')).toEqual(['⇧', '⌘', 'P'])
  })

  it('names arrows and other keys instead of upper-casing them', () => {
    expect(formatKeyParts('ctrl+shift+arrowdown')).toEqual(['⇧', '⌘', '↓'])
    expect(formatKeyParts('delete')).toEqual(['Delete'])
    expect(formatKeyParts('ctrl+shift+home')).toEqual(['⇧', '⌘', 'Home'])
    expect(formatKeyParts('ctrl+pagedown')).toEqual(['⌘', 'PageDown'])
    expect(formatKeyParts('f2')).toEqual(['F2'])
  })

  it('shows the modifiers of a combination still being pressed', () => {
    expect(formatKeyParts('cmd+shift')).toEqual(['⇧', '⌘'])
  })
})
