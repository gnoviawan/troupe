import {
  findConflictingShortcut,
  formatKeyForDisplay,
  matchesShortcut,
  normalizeKeyEvent,
  useKeyboardShortcutsStore
} from '../../../../src/renderer/stores/keyboard-shortcuts-store'
import type { KeyboardShortcutsConfig } from '../../../../src/renderer/types/settings'
import type { Scenario, StoreRecorder, VectorEvent } from '../record'

const ev = (action: string, ...args: unknown[]): VectorEvent => ({ action, args })
const q = (action: string, ...args: unknown[]): VectorEvent & { query: true } => ({
  action,
  args,
  query: true
})

interface KeyInit {
  key: string
  code?: string
  ctrlKey?: boolean
  metaKey?: boolean
  shiftKey?: boolean
  altKey?: boolean
}

/** The module's pure helpers, called by name like store actions. */
const HELPERS: Record<string, (...args: never[]) => unknown> = {
  findConflictingShortcut: (shortcuts: KeyboardShortcutsConfig, key: string, exclude: string) =>
    findConflictingShortcut(shortcuts, key, exclude),
  normalizeKeyEvent: (init: KeyInit) => normalizeKeyEvent(new KeyboardEvent('keydown', init)),
  formatKeyForDisplay: (key: string) => formatKeyForDisplay(key),
  matchesShortcut: (init: KeyInit, key: string) =>
    matchesShortcut(new KeyboardEvent('keydown', init), key)
}

const mini: KeyboardShortcutsConfig = {
  a: { id: 'a', label: 'A', description: 'first', defaultKey: 'ctrl+k' },
  b: { id: 'b', label: 'B', description: 'second', defaultKey: 'ctrl+j', customKey: 'ctrl+k' },
  c: { id: 'c', label: 'C', description: 'third', defaultKey: 'shift+ctrl+arrowdown' },
  d: { id: 'd', label: 'D', description: 'empty', defaultKey: '' }
}

const keyEvents: KeyInit[] = [
  { key: 'k', code: 'KeyK', ctrlKey: true },
  { key: 'K', code: 'KeyK', metaKey: true, shiftKey: true },
  { key: 'P', code: 'KeyP', ctrlKey: true, metaKey: true, shiftKey: true },
  { key: ' ', code: 'Space', altKey: true },
  { key: 'Escape', code: 'Escape' },
  { key: '_', code: 'Minus', ctrlKey: true, shiftKey: true },
  { key: '–', code: 'Minus', altKey: true },
  { key: '+', code: 'Equal', ctrlKey: true },
  { key: 'Control', code: 'ControlLeft', ctrlKey: true },
  { key: 'Shift', code: 'ShiftLeft', shiftKey: true, altKey: true },
  { key: 'ArrowDown', code: 'ArrowDown', ctrlKey: true, shiftKey: true },
  { key: 'PageDown', code: 'PageDown', altKey: true },
  { key: '∫', code: 'KeyB', altKey: true, metaKey: true },
  { key: 'ñ', code: 'Semicolon' }
]

const matchCases: Array<[KeyInit, string]> = [
  [{ key: 'k', code: 'KeyK', ctrlKey: true }, 'ctrl+k'],
  [{ key: 'k', code: 'KeyK', metaKey: true }, 'ctrl+k'],
  [{ key: 'p', code: 'KeyP', ctrlKey: true, shiftKey: true }, 'shift+ctrl+p'],
  [{ key: 'ArrowDown', code: 'ArrowDown', ctrlKey: true }, 'ctrl+downarrow'],
  [{ key: '∫', code: 'KeyB', altKey: true, ctrlKey: true }, 'ctrl+alt+b'],
  [{ key: '∫', code: 'KeyB', altKey: true }, 'alt+c'],
  [{ key: 'r', code: 'KeyR', ctrlKey: true }, ''],
  [{ key: '=', code: 'Equal', altKey: true }, 'alt+='],
  [{ key: 'n', code: 'KeyN', altKey: true, shiftKey: true }, 'alt+shift+n']
]

const scenarios = (platform: 'web' | 'desktop'): Scenario[] => [
  {
    name: `${platform}_update_and_reset`,
    context: { platform: { desktop: platform === 'desktop', mac: false } },
    steps: [
      ev('updateShortcut', 'commandPalette', 'ctrl+shift+k'),
      ev('updateShortcut', 'commandPalette', 'k+ctrl'),
      ev('updateShortcut', 'newTerminal', 'alt+shift+t'),
      ev('updateShortcut', 'zoomIn', 'ctrl+='),
      ev('updateShortcut', 'missing', 'ctrl+x'),
      ev('updateShortcut', 'saveFile', 'ctrl+uparrow'),
      ev('resetShortcut', 'newTerminal'),
      ev('resetShortcut', 'missing'),
      ev('resetAllShortcuts')
    ]
  }
]

export const shortcutScenarios: Scenario[] = [
  ...scenarios('web'),
  ...scenarios('desktop'),
  {
    name: 'set_shortcuts_and_conflicts',
    steps: [
      ev('setShortcuts', mini),
      ev('updateShortcut', 'c', 'ctrl+shift+downarrow'),
      ev('updateShortcut', 'a', 'ctrl+j'),
      q('findConflictingShortcut', mini, 'ctrl+k', 'x'),
      q('findConflictingShortcut', mini, 'ctrl+k', 'a'),
      q('findConflictingShortcut', mini, 'k+ctrl', 'b'),
      q('findConflictingShortcut', mini, 'ctrl+shift+downarrow', 'x'),
      q('findConflictingShortcut', mini, '', 'x'),
      q('findConflictingShortcut', mini, 'ctrl+q', 'x')
    ]
  },
  {
    name: 'normalize_key_events',
    steps: keyEvents.map((init) => q('normalizeKeyEvent', init))
  },
  {
    name: 'format_keys_for_display',
    steps: [
      'ctrl+shift+p',
      'cmd+alt+tab',
      'esc',
      'alt+space',
      'alt+pageup',
      'ctrl+pagedown',
      ''
    ].map((key) => q('formatKeyForDisplay', key))
  },
  {
    name: 'match_key_events',
    steps: matchCases.map(([init, key]) => q('matchesShortcut', init, key))
  }
]

type Globals = { __TAURI_INTERNALS__?: unknown }

export const shortcutsRecorder: StoreRecorder = {
  store: 'shortcuts',
  sources: [
    'src/renderer/stores/keyboard-shortcuts-store.ts',
    'src/renderer/types/settings.ts',
    'src/renderer/lib/platform.ts'
  ],
  reset(context) {
    // `getDefaultKeyboardShortcuts` reads this global to pick the desktop set.
    const platform = context.platform as { desktop?: boolean } | undefined
    if (platform?.desktop) (window as Globals).__TAURI_INTERNALS__ = {}
    else delete (window as Globals).__TAURI_INTERNALS__
    useKeyboardShortcutsStore.getState().resetAllShortcuts()
    useKeyboardShortcutsStore.setState({ isLoaded: false })
  },
  seed(initial) {
    useKeyboardShortcutsStore.setState(structuredClone(initial))
  },
  snapshot() {
    const { shortcuts, isLoaded } = useKeyboardShortcutsStore.getState()
    return { shortcuts, isLoaded }
  },
  dispatch({ action, args }) {
    const helper = HELPERS[action]
    if (helper) return helper(...(args as never[]))
    const fn = (useKeyboardShortcutsStore.getState() as unknown as Record<string, unknown>)[action]
    if (typeof fn !== 'function') throw new Error(`shortcuts store has no action ${action}`)
    return fn(...args)
  }
}
