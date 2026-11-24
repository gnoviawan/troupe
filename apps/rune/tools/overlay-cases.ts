// The overlays the pixel check opens on both sides: on the design-system gallery by clicking
// its triggers (dump-overlays.ts), and on the Rune gallery by tapping the trigger keyed
// `trigger` in the scene `entry` (parity-overlays.ts). `surface` is the open surface's selector
// on the web; `key` is the same surface's element key in the Rune gallery.

export const VIEWPORT: [number, number] = [1280, 800]

export interface OpenStep {
  action: 'click' | 'context' | 'hover'
  /** `text` finds the trigger by its text (a context-menu area has no role). */
  role: 'button' | 'combobox' | 'text'
  name: string
}

export interface OverlayCase {
  id: string
  entry: string
  open: OpenStep[]
  surface: string
  /** Rune: the trigger's key and how the gallery opens it, then the surface's key. */
  trigger: string
  gesture: 'tap' | 'context' | 'hover'
  key: string
  settle?: number
}

const POPPER = '[data-radix-popper-content-wrapper] > *'

export const OVERLAY_CASES: OverlayCase[] = [
  {
    id: 'dialog',
    entry: 'dialog',
    open: [{ action: 'click', role: 'button', name: 'Rename session' }],
    surface: '[role=dialog]',
    trigger: 'rename-trigger',
    gesture: 'tap',
    key: 'rename-dialog'
  },
  {
    id: 'alert-dialog',
    entry: 'dialog',
    open: [{ action: 'click', role: 'button', name: 'Delete worktree' }],
    surface: '[role=alertdialog]',
    trigger: 'delete-trigger',
    gesture: 'tap',
    key: 'delete-dialog'
  },
  {
    id: 'confirm-danger',
    entry: 'confirm-dialog',
    open: [{ action: 'click', role: 'button', name: 'Danger' }],
    surface: '[role=alertdialog]',
    trigger: 'confirm-danger-trigger',
    gesture: 'tap',
    key: 'confirm-danger-dialog'
  },
  {
    id: 'sheet-right',
    entry: 'sheet',
    open: [{ action: 'click', role: 'button', name: 'right' }],
    surface: '[role=dialog]',
    trigger: 'sheet-right-trigger',
    gesture: 'tap',
    key: 'sheet-right-dialog'
  },
  {
    id: 'menu-dropdown',
    entry: 'menus',
    open: [{ action: 'click', role: 'button', name: 'View options' }],
    surface: '[role=menu]',
    trigger: 'view-menu-trigger',
    gesture: 'tap',
    key: 'view-menu-menu'
  },
  {
    id: 'menu-context',
    entry: 'menus',
    open: [{ action: 'context', role: 'text', name: 'Right-click here' }],
    surface: '[role=menu]',
    trigger: 'file-menu-trigger',
    gesture: 'context',
    key: 'file-menu-menu'
  },
  {
    id: 'select',
    entry: 'select',
    open: [{ action: 'click', role: 'combobox', name: 'Branch' }],
    surface: '[role=listbox]',
    trigger: 'branch-trigger',
    gesture: 'tap',
    key: 'branch-listbox'
  },
  {
    id: 'popover',
    entry: 'floating',
    open: [{ action: 'click', role: 'button', name: 'Find branch' }],
    surface: POPPER,
    trigger: 'find-branch-trigger',
    gesture: 'tap',
    key: 'find-branch-content'
  },
  {
    id: 'tooltip',
    entry: 'floating',
    open: [{ action: 'hover', role: 'button', name: 'Split terminal' }],
    surface: POPPER,
    trigger: 'split-tip-trigger',
    gesture: 'hover',
    key: 'split-tip-tooltip',
    settle: 1200
  },
  {
    id: 'command-dialog',
    entry: 'command',
    open: [{ action: 'click', role: 'button', name: 'Open CommandDialog' }],
    surface: '[role=dialog]',
    trigger: 'command-trigger',
    gesture: 'tap',
    key: 'command-dialog'
  }
]
