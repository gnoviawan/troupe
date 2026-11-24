/**
 * What a client must provide to take part in the matrix, and the helpers
 * every driver shares.
 *
 * A client session starts on the dashboard as a fresh device and hands the
 * runner a `Driver`. The runner replays each step's actions, asks for each
 * check, then grabs one frame per step. A driver whose client has a DOM also
 * measures the surface's region and where text and icons were painted, so
 * the diff can score them apart.
 */

import type { PartRects, Rect } from '../diff'
import type { Action, ActionLog, Check, CheckLog, Client, Surface, Target } from '../types'
import type { World } from '../world'

export interface Point {
  x: number
  y: number
}

/**
 * Where each action's target was found, keyed `<step>/<action index>`. The
 * client that resolves targets first records them; a client that cannot name
 * a target (a DOM-only label, or no automation tree at all) acts there.
 */
export type Points = Map<string, Point>

export interface Driver {
  act(step: string, index: number, action: Action): Promise<ActionLog>
  /** Absent for a client whose text cannot be read: its frames are the record. */
  check?(step: string, check: Check): Promise<CheckLog>
  grab(file: string): Promise<void>
  /** The visible box of the first element matching `selector`, for a client with a DOM. */
  measure?(selector: string): Promise<Rect | null>
  /** Where this frame painted text and icons, for a client with a DOM. */
  parts?(): Promise<PartRects>
}

export interface SessionContext {
  world: World
  surface: Surface
  deviceId: string
  points: Points
  /** Per-surface scratch space the client may use (a throwaway HOME). */
  stateDir: string
}

export interface ClientLauncher {
  client: Client
  /**
   * True when the driver finds targets itself (test id, text, DOM). The
   * runner drives such a client first, so a pixel-only client can act at the
   * points it recorded.
   */
  resolvesTargets: boolean
  /** Start a session on the surface's start route, run `body`, tear the session down. */
  session<T>(ctx: SessionContext, body: (driver: Driver) => Promise<T>): Promise<T>
}

/** One `design-system` registry entry, and the Rune gallery scene of the same id. */
export interface GalleryEntry {
  id: string
  name: string
  width: number
  height: number
}

/** A client of the gallery profile, open on its gallery for the whole run. */
export interface GallerySession {
  /** What drew the frames, when the client can tell. */
  renderer?: string
  /**
   * Shows `entry` and writes its frame, `entry.width` x `entry.height`, to
   * `file`. Returns how the entry was picked and, for a client with a DOM,
   * where text and icons were painted in the frame's coordinates.
   */
  shoot(
    entry: GalleryEntry,
    file: string
  ): Promise<{ via: ActionLog['via']; parts: PartRects | null }>
  close(): Promise<void>
}

/**
 * Where a driver moves the pointer after an action that leaves it over a
 * control (`PARKS_POINTER`): the bottom edge of the window, over no control.
 * A pointer left on a button changes its hover look a few seconds later
 * (measured on the tool strip: about 2.8 s after a click), so a frame would
 * depend on when it was grabbed. A `hover` action keeps the pointer where it is.
 */
export const PARK_POINT: Point = { x: 640, y: 799 }
export const PARKS_POINTER: ReadonlySet<Action['do']> = new Set([
  'click',
  'dblclick',
  'drag',
  'choose'
])

export const PRESENT_TIMEOUT_MS = 15_000
export const GONE_TIMEOUT_MS = 6_000
export const SETTLE_AFTER_ACTION_MS = 250

export const normalize = (text: string) => text.replace(/\s+/g, ' ').trim()

export function describeTarget(target: Target): string {
  if ('testId' in target) return `#${target.testId}`
  if ('text' in target) return `"${target.text}"`
  if ('label' in target) return `[${target.label}]`
  if ('point' in target) return `@${target.point.x},${target.point.y}`
  return target.css
}

export function describe(action: Action): string {
  switch (action.do) {
    case 'click': {
      const verb =
        action.button === 'right'
          ? 'right-click'
          : action.modifiers
            ? `${action.modifiers}-click`
            : 'click'
      return `${verb} ${describeTarget(action.target)}`
    }
    case 'dblclick':
    case 'hover':
      return `${action.do} ${describeTarget(action.target)}`
    case 'press':
      return `press ${action.keys}`
    case 'type':
      return `type "${action.text}"`
    case 'wheel':
      return `wheel ${action.deltaY} over ${describeTarget(action.target)}`
    case 'drag':
      return `drag ${describeTarget(action.from)} -> ${describeTarget(action.to)}`
    case 'wait':
      return `wait ${action.ms} ms`
    case 'choose':
      return `choose ${action.file} from ${describeTarget(action.target)}`
  }
}

/** Every text in `texts` occurs in `haystack`, each after the previous one. */
export function ordered(haystack: string, texts: string[]): boolean {
  const flat = normalize(haystack)
  let from = 0
  for (const text of texts) {
    const needle = normalize(text)
    const at = flat.indexOf(needle, from)
    if (at < 0) return false
    from = at + needle.length
  }
  return true
}

export async function poll(fn: () => Promise<boolean>, timeoutMs: number): Promise<boolean> {
  const deadline = Date.now() + timeoutMs
  for (;;) {
    if (await fn()) return true
    if (Date.now() > deadline) return false
    await Bun.sleep(150)
  }
}

/** The first line of an error, for a log entry. */
export const errorLine = (error: unknown) =>
  error instanceof Error ? (error.message.split('\n')[0] ?? '') : String(error)
