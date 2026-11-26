/**
 * The UI inventory, measured from `src/renderer` at the checked-out head with
 * the rule of the R3 UI map (`troupe-ui-map-v2.md`), so the surface list can
 * be checked against the code it claims to cover.
 *
 * - An overlay is a mount site of a library overlay (the `MOUNT` tags outside
 *   `components/ui`, which defines them), or a file that builds its own modal
 *   (`HAND_BUILT`), minus `NOT_SURFACES`. A phone/desktop twin is one surface
 *   that claims both mount sites. Unlike the map, comment lines are skipped:
 *   three doc comments name `<ContextMenuContent>` and mount nothing.
 * - A screen is a route, a full-screen gate, a pane tab kind, a side-panel
 *   tab, or a settings category.
 */

import { readFileSync } from 'node:fs'
import path from 'node:path'
import type { ScreenKind } from '../types'

export const RENDERER = path.resolve(import.meta.dir, '../../../src/renderer')

const MOUNT =
  /<(DialogContent|AlertDialogContent|SheetContent|MobileSheetContent|PopoverContent|DropdownMenuContent|ContextMenuContent|HoverCardContent|AnimatedMenuContent|ConfirmDialog|ImageLightbox|ImageGalleryLightbox|RailHoverPanelContent)(\s|>|$)/gm
const HAND_BUILT = /aria-modal|role="dialog"|fixed inset-0/

/** Files the patterns match that put no surface of their own on screen, and why. */
export const NOT_SURFACES: Record<string, string> = {
  'components/ConfirmDialog.tsx': 'the ConfirmDialog wrapper: each of its mount sites counts',
  'components/use-modal-focus.ts': 'the focus hook the hand-built modals share',
  'components/ResizeEdges.tsx': 'the window resize edges, not an overlay',
  'lib/smooth-wheel.ts': 'a scroll helper',
  'components/setup/RestartingScreen.tsx': 'a full-screen gate, counted as a screen',
  'components/AiPromptDialog.tsx': 'no production importer'
}

const TEST_FILE = /(__tests__|\.test\.|\.spec\.|test-utils\/|\/vitest\.d\.ts$)/

/** Production source files of the renderer, relative to it. */
export function productionFiles(): string[] {
  return [...new Bun.Glob('**/*.{ts,tsx}').scanSync(RENDERER)]
    .filter((file) => !TEST_FILE.test(file))
    .sort()
}

const read = (file: string) => readFileSync(path.join(RENDERER, file), 'utf8')
const COMMENT_LINE = /^\s*(\*|\/\/|\/\*|\{\/\*)/
const code = (text: string) =>
  text
    .split('\n')
    .filter((line) => !COMMENT_LINE.test(line))
    .join('\n')

/** How many overlay surfaces each file mounts: its library mount sites, plus one if it builds its own modal. */
export function measureOverlays(): Record<string, number> {
  const out: Record<string, number> = {}
  for (const file of productionFiles()) {
    if (file.startsWith('components/ui/') || file in NOT_SURFACES) continue
    const text = code(read(file))
    const n = (text.match(MOUNT)?.length ?? 0) + (HAND_BUILT.test(text) ? 1 : 0)
    if (n > 0) out[file] = n
  }
  return out
}

const names = (file: string, pattern: RegExp) =>
  [...read(file).matchAll(pattern)].map((m) => m[1] as string)

/** The full-screen gates that replace the workspace (read by hand: they share no pattern). */
export const GATES = [
  'WebTokenGateScreen',
  'ServerSetupWizard',
  'RestartingScreen',
  'ErrorFallback',
  'Onboarding'
]

/** The screens of each kind, by the name a `screen` source uses. */
export function measureScreens(): Record<ScreenKind, string[]> {
  const settings = (file: string, prefix: string) =>
    names(file, /^ {4}id: '([a-z-]+)'/gm).map((id) => `${prefix}:${id}`)
  const routes = names('App.tsx', /path: '([^']+)'/g)
  return {
    route: ['/', ...routes.filter((r) => r !== '/' && r !== '*').map((r) => `/${r}`), '*'].sort(),
    gate: [...GATES].sort(),
    pane: [...new Set(names('stores/workspace-store/types.ts', /type: '([a-z-]+)'/g))].sort(),
    'side-panel': names('lib/shell-layout-record.ts', /SHELL_LAST_COLUMN_TABS = \[([^\]]+)\]/g)
      .flatMap((list) => list.split(',').map((tab) => tab.trim().replace(/'/g, '')))
      .sort(),
    settings: [
      ...settings('pages/app-preferences/categories.tsx', 'app'),
      ...settings('pages/project-settings/categories.tsx', 'project')
    ].sort()
  }
}
