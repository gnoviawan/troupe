/**
 * Terminal session vectors: what `lib/command-lifecycle/` does for seeded walks of events and
 * calls (`terminal-io.ts` has the rest). `apps/rune/crates/model/tests/terminal/mod.rs`
 * replays them against `model::terminal`.
 *
 * A walk is generated from a fixed seed, so the file is the same on every run. The cases the
 * unit suites name (`transition.test.ts`, `session-store*.test.ts`, `completion*.test.ts`,
 * `input-guard.test.ts`, `command-editor-transport.test.ts`) seed the pools the walks draw
 * from.
 */

import {
  type CompletionContext,
  CompletionScheduler
} from '../../../src/renderer/lib/command-lifecycle/completion'
import { commandDraftStore } from '../../../src/renderer/lib/command-lifecycle/draft-store'
import {
  bumpEditorRevision,
  readEditorRevision
} from '../../../src/renderer/lib/command-lifecycle/editor-revision'
import { applyScreenMode } from '../../../src/renderer/lib/command-lifecycle/screen-mode'
import {
  type SessionLifecycleInput,
  type SubmitToken,
  sessionStore
} from '../../../src/renderer/lib/command-lifecycle/session-store'
import {
  type InputMode,
  reduceSession,
  type SessionEvent,
  type SessionPhase,
  type SessionState
} from '../../../src/renderer/lib/command-lifecycle/transition'
import type { CompletionRpcReply } from '../../../src/shared/types/ipc.types'

export const terminalSources = [
  'src/renderer/components/terminal/command-editor/editor-keys.ts',
  'src/renderer/hooks/use-command-history.ts',
  'src/renderer/lib/command-editor-transport.ts',
  'src/renderer/lib/command-lifecycle/completion-output.ts',
  'src/renderer/lib/command-lifecycle/completion.ts',
  'src/renderer/lib/command-lifecycle/draft-store.ts',
  'src/renderer/lib/command-lifecycle/editor-revision.ts',
  'src/renderer/lib/command-lifecycle/input-guard.ts',
  'src/renderer/lib/command-lifecycle/screen-mode.ts',
  'src/renderer/lib/command-lifecycle/session-store.ts',
  'src/renderer/lib/command-lifecycle/transition.ts',
  'src/renderer/lib/persistence-trim.ts',
  'src/renderer/stores/command-history-store.ts'
]

export interface Logged {
  level: string
  source: string
  message: string
}

/** The log lines `logFrontendError` received since the last drain; the test mocks it. */
export const logged: Logged[] = []

export function drainLogs(source?: string): Logged[] {
  const out = logged.splice(0).filter((l) => !source || l.source === source)
  return out
}

/** mulberry32: a seeded generator, so a walk is the same on every run. */
function rng(seed: number) {
  let a = seed >>> 0
  const next = () => {
    a = (a + 0x6d2b79f5) >>> 0
    let t = a
    t = Math.imul(t ^ (t >>> 15), t | 1)
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61)
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296
  }
  return {
    chance: (p: number) => next() < p,
    pick: <T>(items: readonly T[]): T => items[Math.floor(next() * items.length)] as T,
    int: (lo: number, hi: number) => lo + Math.floor(next() * (hi - lo + 1))
  }
}
type Rng = ReturnType<typeof rng>

export const MODES: InputMode[] = ['Editor', 'PTY', 'raw', 'exited']
export const PHASES: SessionPhase[] = [
  'Unknown',
  'AtPrompt',
  'Submitting',
  'Executing',
  'Terminated'
]

// ---- reducer ------------------------------------------------------------------------------

function strip(state: SessionState): Omit<SessionState, 'buffer'> {
  const { buffer: _buffer, ...rest } = state
  return rest
}

/** The fields of `next` that differ from `prev`; a field `next` lacks reads as null. */
function changed(prev: object, next: object): Record<string, unknown> {
  const a = prev as Record<string, unknown>
  const b = next as Record<string, unknown>
  const out: Record<string, unknown> = {}
  for (const key of new Set([...Object.keys(a), ...Object.keys(b)])) {
    if (JSON.stringify(a[key]) !== JSON.stringify(b[key])) out[key] = b[key] ?? null
  }
  return out
}

function startState(r: Rng): SessionState {
  const generation = r.pick([null, 1, 2, 3])
  return {
    sessionId: r.chance(0.9) ? 's1' : null,
    nonce: r.chance(0.9) ? 'n1' : null,
    registered: r.chance(0.85),
    alive: r.chance(0.95),
    trusted: r.chance(0.6),
    mode: r.pick(MODES.slice(0, r.chance(0.9) ? 3 : 4)),
    phase: r.pick(PHASES.slice(0, 4)),
    buffer: '',
    promptGeneration: generation,
    lastExitCode: r.pick([null, 0, 2]),
    committedSubmit: r.chance(0.3),
    completionPending: r.chance(0.2),
    alternateScreen: r.chance(0.2),
    heldPromptGeneration: r.pick([null, null, 2, 4])
  }
}

function sessionIdOf(r: Rng): string | undefined {
  return r.chance(0.85) ? 's1' : r.pick(['s2', undefined])
}

function nonceOf(r: Rng): string | undefined {
  return r.chance(0.85) ? 'n1' : r.pick(['n2', undefined])
}

function reducerEvent(r: Rng, gen: { last: number }): SessionEvent {
  const kind = r.int(0, 15)
  if (kind <= 4) {
    return {
      type: 'PromptReady',
      sessionId: sessionIdOf(r),
      frame: {
        type: 'PromptReady',
        nonce: nonceOf(r),
        prompt_generation: r.chance(0.9) ? generation(r, gen) : undefined,
        line_empty: r.chance(0.8) ? true : r.pick([false, undefined]),
        prompt_suppressed: r.chance(0.8) ? true : r.pick([false, undefined]),
        cwd: '/repo'
      }
    }
  }
  if (kind <= 6) return { type: 'CommandStarted', sessionId: sessionIdOf(r), nonce: nonceOf(r) }
  if (kind <= 8) {
    return {
      type: 'CommandFinished',
      sessionId: sessionIdOf(r),
      nonce: nonceOf(r),
      exit_code: r.pick([0, 1, 127, undefined])
    }
  }
  return r.pick<SessionEvent>([
    { type: 'Osc133', text: 'A' },
    { type: 'IntegrationLost' },
    r.chance(0.3) ? { type: 'ProcessExit' } : { type: 'Elapsed', ms: 60_000 },
    { type: 'CompletionPending' },
    { type: 'Desync' },
    { type: 'AlternateScreenEnter' },
    { type: 'AlternateScreenLeave' }
  ])
}

export function recordReducer(seed: number, walks: number, steps: number) {
  const r = rng(seed)
  return Array.from({ length: walks }, () => {
    let state = startState(r)
    const start = strip(state)
    const gen = { last: state.promptGeneration ?? 0 }
    const trail = Array.from({ length: steps }, () => {
      const screen = r.chance(0.1)
      if (screen) {
        const alternateScreen = r.chance(0.5)
        const reduced = applyScreenMode(state, {
          alternateScreen,
          bracketedPaste: false,
          applicationCursor: false
        })
        const diff = changed(strip(state), strip(reduced.state))
        state = reduced.state
        return { screen: alternateScreen, changed: diff, log: reduced.log?.detail ?? null }
      }
      const event = reducerEvent(r, gen)
      const reduced = reduceSession(state, event)
      const diff = changed(strip(state), strip(reduced.state))
      state = reduced.state
      return { event, changed: diff, log: reduced.log?.detail ?? null }
    })
    return { start, steps: trail }
  })
}

// ---- session store ------------------------------------------------------------------------

type StoreOp =
  | { op: 'apply'; event: SessionLifecycleInput }
  | { op: 'screen'; alternateScreen: boolean }
  | { op: 'beginSubmit' }
  | {
      op: 'recordSubmit'
      result: 'Accepted' | 'PreWrite' | 'Partial' | 'NotStarted'
      token: number
    }
  | { op: 'leaveEditor' }
  | { op: 'invalidate'; level: 'info' | 'warn' }
  | { op: 'processExited' }
  | { op: 'forget' }
  | { op: 'offer'; mode: InputMode; text: string }
  | { op: 'bumpRevision' }

/** Prompt generations rise as a shell's do; now and then one repeats or goes back. */
function generation(r: Rng, gen: { last: number }): number {
  if (r.chance(0.75)) gen.last += 1
  return r.chance(0.85) ? gen.last : r.int(0, gen.last)
}

function lifecycle(r: Rng, kind: string, gen: { last: number }): SessionLifecycleInput {
  const event: SessionLifecycleInput = { type: 'session_lifecycle', kind }
  // A key is left out rather than set to undefined, so the JSON says what the store saw.
  if (kind === 'InitShell') {
    if (r.chance(0.9)) event.session_id = r.chance(0.9) ? 's1' : 's2'
    if (r.chance(0.9)) event.nonce = r.chance(0.8) ? 'n1' : 'n2'
    return event
  }
  if (r.chance(0.97)) event.session_id = r.chance(0.95) ? 's1' : 's2'
  if (r.chance(0.95)) event.nonce = r.chance(0.04) ? null : r.chance(0.95) ? 'n1' : 'n2'
  if (kind === 'PromptReady') {
    if (r.chance(0.97)) event.prompt_generation = generation(r, gen)
    if (r.chance(0.97)) event.cwd = r.pick(['/repo', '/repo', '/repo/src', ''])
    if (r.chance(0.95)) event.line_empty = r.chance(0.92)
    if (r.chance(0.95)) event.prompt_suppressed = r.chance(0.92)
  }
  if (kind === 'CommandFinished' && r.chance(0.8)) event.exit_code = r.pick([0, 1, null])
  return event
}

function storeOp(r: Rng, tokens: number, gen: { last: number }, registered: boolean): StoreOp {
  const kind = r.int(0, 23)
  if (kind <= 0 || (!registered && r.chance(0.6))) {
    return { op: 'apply', event: lifecycle(r, 'InitShell', gen) }
  }
  if (kind <= 5) return { op: 'apply', event: lifecycle(r, 'PromptReady', gen) }
  if (kind <= 7) return { op: 'apply', event: lifecycle(r, 'CommandStarted', gen) }
  if (kind <= 8) return { op: 'apply', event: lifecycle(r, 'CommandFinished', gen) }
  if (kind <= 9) return { op: 'apply', event: lifecycle(r, 'Desync', gen) }
  if (kind <= 10) return { op: 'screen', alternateScreen: r.chance(0.5) }
  if (kind <= 13) return { op: 'beginSubmit' }
  if (kind <= 16 && tokens > 0) {
    return {
      op: 'recordSubmit',
      result: r.pick(['Accepted', 'Accepted', 'PreWrite', 'Partial', 'NotStarted'] as const),
      token: r.chance(0.8) ? tokens - 1 : r.int(0, tokens - 1)
    }
  }
  if (kind <= 17)
    return {
      op: 'offer',
      mode: r.pick(['Editor', 'Editor', 'PTY'] as const),
      text: r.pick(['ls', 'git st', 'echo "a\nb"', ''])
    }
  if (kind <= 18) return { op: 'bumpRevision' }
  if (kind <= 19) return { op: 'leaveEditor' }
  if (kind <= 21) return { op: 'beginSubmit' }
  if (kind <= 22)
    return r.chance(0.5)
      ? { op: 'invalidate', level: r.pick(['info', 'warn'] as const) }
      : { op: 'processExited' }
  return r.chance(0.4) ? { op: 'forget' } : { op: 'screen', alternateScreen: r.chance(0.5) }
}

export function recordStore(seed: number, walks: number, steps: number) {
  const r = rng(seed)
  return Array.from({ length: walks }, (_, walk) => {
    const id = `w${seed}-${walk}`
    sessionStore.reset()
    drainLogs()
    const tokens: SubmitToken[] = []
    let previous: object = sessionStore.read(id)
    const gen = { last: 0 }
    const trail = Array.from({ length: steps }, (_, step) => {
      const op: StoreOp =
        step === 0
          ? {
              op: 'apply',
              event: { type: 'session_lifecycle', kind: 'InitShell', session_id: 's1', nonce: 'n1' }
            }
          : storeOp(r, tokens.length, gen, sessionStore.read(id).registered)
      let ret: unknown = null
      switch (op.op) {
        case 'apply':
          sessionStore.apply(id, op.event)
          break
        case 'screen':
          sessionStore.applyScreen(id, {
            alternateScreen: op.alternateScreen,
            bracketedPaste: false,
            applicationCursor: false
          })
          break
        case 'beginSubmit': {
          const token = sessionStore.beginSubmit(id)
          if (token) tokens.push(token)
          ret = token ? { epochIndex: tokens.length - 1 } : null
          break
        }
        case 'recordSubmit':
          ret = sessionStore.recordSubmit(id, op.result, tokens[op.token] as SubmitToken)
          break
        case 'leaveEditor':
          sessionStore.leaveEditor(id)
          break
        case 'invalidate':
          sessionStore.invalidate(id, op.level)
          break
        case 'processExited':
          sessionStore.processExited(id)
          break
        case 'forget':
          sessionStore.forget(id)
          break
        case 'offer':
          commandDraftStore.offer(id, op.mode, op.text)
          break
        case 'bumpRevision':
          ret = bumpEditorRevision(id)
          break
      }
      const last = tokens.at(-1)
      const session = sessionStore.read(id)
      const diff = changed(previous, session)
      previous = session
      return {
        ...op,
        ret,
        changed: diff,
        draft: commandDraftStore.read(id),
        draftVersion: commandDraftStore.invalidationVersion(id),
        revision: readEditorRevision(id),
        current: last ? sessionStore.isSubmitCurrent(id, last) : null,
        logs: drainLogs('sessionStore').map(({ level, message }) => ({ level, message }))
      }
    })
    return { terminalId: id, steps: trail }
  })
}

// ---- completion scheduler -----------------------------------------------------------------

type SchedulerOp =
  | { op: 'request'; context: CompletionContext; showing: boolean }
  | { op: 'update'; context: CompletionContext }
  | { op: 'prepareSubmit' }
  | { op: 'invalidate' }
  | { op: 'advance'; ms: number }
  | { op: 'reply'; requestId: string; reply: CompletionRpcReply }

const DRAFTS = ['', '  ', 'git ch', 'ls "my f', 'echo hi there', 'cd sr']

function context(r: Rng): CompletionContext {
  const draft = r.pick(DRAFTS)
  const cursor = r.chance(0.8) ? draft.length : r.int(0, draft.length)
  const out: CompletionContext = {
    terminalId: 't1',
    sessionId: 's1',
    mode: 'Editor',
    phase: 'AtPrompt',
    draft,
    bufferRevision: r.int(1, 3),
    cursor,
    promptGeneration: r.chance(0.9) ? 4 : 5
  }
  if (r.chance(0.9)) out.cwd = r.chance(0.9) ? '/repo' : ''
  return out
}

function completionReply(
  r: Rng,
  requestId: string,
  asked: CompletionContext | undefined
): CompletionRpcReply {
  const kind = r.int(0, 9)
  if (kind <= 5 && asked) {
    const off = r.chance(0.15)
    return {
      result: 'Accepted',
      outcome: 'Completed',
      completion: {
        request_id: off && r.chance(0.5) ? 'completion-other' : requestId,
        buffer_revision: asked.bufferRevision + (off ? 1 : 0),
        cursor: asked.cursor,
        prompt_generation: asked.promptGeneration,
        cwd: asked.cwd ?? '',
        output: '737461747573\n7374617368'
      }
    }
  }
  if (kind <= 6)
    return { result: 'Accepted', outcome: r.pick(['TimedOut', 'Cancelled', 'Dropped'] as const) }
  return { result: r.pick(['NotStarted', 'PreWrite', 'Partial'] as const) }
}

const flush = async () => {
  for (let i = 0; i < 5; i += 1) await Promise.resolve()
}

export async function recordScheduler(seed: number, walks: number, steps: number) {
  const r = rng(seed)
  const out = []
  for (let walk = 0; walk < walks; walk += 1) {
    let now = 0
    let timerIds = 0
    const timers = new Map<number, { at: number; handler: () => void }>()
    const effects: unknown[] = []
    const pending = new Map<string, (reply: CompletionRpcReply) => void>()
    const asked = new Map<string, CompletionContext>()
    let requests = 0
    const autoRefresh = r.chance(0.8)
    let lastContext: CompletionContext | undefined
    const scheduler = new CompletionScheduler({
      host: {
        requestCompletion: (terminalId: string, request: { requestId: string }) => {
          effects.push({ kind: 'request', terminalId, request })
          return new Promise<CompletionRpcReply>((resolve) =>
            pending.set(request.requestId, resolve)
          )
        },
        cancelCompletion: async (terminalId: string, requestId: string) => {
          effects.push({ kind: 'cancel', terminalId, requestId })
          return 'Accepted' as const
        }
      },
      createRequestId: () => {
        requests += 1
        const id = `completion-${requests}`
        if (lastContext) asked.set(id, lastContext)
        return id
      },
      settings: { automaticCompletionRefresh: autoRefresh },
      clock: {
        setTimeout: (handler, ms) => {
          timerIds += 1
          timers.set(timerIds, { at: now + ms, handler })
          return timerIds
        },
        clearTimeout: (id) => {
          timers.delete(id as number)
        }
      },
      log: (event) => effects.push({ kind: 'log', ...event }),
      onResult: (completion) => effects.push({ kind: 'show', completion })
    })
    const trail = []
    for (let step = 0; step < steps; step += 1) {
      const kind = r.int(0, 11)
      let op: SchedulerOp
      if (kind <= 2) op = { op: 'request', context: context(r), showing: r.chance(0.15) }
      else if (kind <= 5) op = { op: 'update', context: context(r) }
      else if (kind <= 6) op = { op: r.chance(0.5) ? 'prepareSubmit' : 'invalidate' }
      else if (kind <= 8 || pending.size === 0)
        op = { op: 'advance', ms: r.pick([50, 100, 150, 4000, 4100]) }
      else {
        const ids = [...pending.keys()]
        const requestId = r.chance(0.8) ? (ids.at(-1) as string) : r.pick(ids)
        op = { op: 'reply', requestId, reply: completionReply(r, requestId, asked.get(requestId)) }
      }
      effects.length = 0
      switch (op.op) {
        case 'request':
          lastContext = op.context
          scheduler.requestCompletion(op.context, op.showing)
          break
        case 'update':
          lastContext = op.context
          scheduler.updateContext(op.context)
          break
        case 'prepareSubmit':
          await scheduler.prepareSubmit()
          break
        case 'invalidate':
          scheduler.invalidate()
          break
        case 'advance': {
          const until = now + op.ms
          for (;;) {
            const due = [...timers.entries()]
              .filter(([, t]) => t.at <= until)
              .sort(([a, ta], [b, tb]) => ta.at - tb.at || a - b)[0]
            if (!due) break
            timers.delete(due[0])
            now = due[1].at
            due[1].handler()
            await flush()
          }
          now = until
          break
        }
        case 'reply': {
          const resolve = pending.get(op.requestId)
          pending.delete(op.requestId)
          resolve?.(op.reply)
          break
        }
      }
      await flush()
      trail.push({ ...op, now, effects: [...effects], inFlight: scheduler.inFlightRequestId })
    }
    out.push({ autoRefresh, steps: trail })
  }
  return out
}
