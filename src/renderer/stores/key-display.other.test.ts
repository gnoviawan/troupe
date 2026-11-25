import { describe, expect, it, vi } from 'vitest'

vi.mock('@/lib/platform', () => ({ isMac: false, isWindows: true, isLinux: false }))

import { formatKeyForDisplay, formatKeyParts } from './keyboard-shortcuts-store'

describe('key display on Windows and Linux', () => {
  it('names the modifiers and joins with +', () => {
    expect(formatKeyParts('ctrl+shift+p')).toEqual(['Ctrl', 'Shift', 'P'])
    expect(formatKeyForDisplay('ctrl+shift+p')).toBe('Ctrl+Shift+P')
  })

  it('keeps Ctrl as Ctrl', () => {
    expect(formatKeyForDisplay('ctrl+k')).toBe('Ctrl+K')
  })

  it('names arrows', () => {
    expect(formatKeyForDisplay('ctrl+shift+arrowup')).toBe('Ctrl+Shift+↑')
  })
})
