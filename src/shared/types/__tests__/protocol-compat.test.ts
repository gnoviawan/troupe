import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'
import type * as G from '../generated/protocol'
import { WsErrorCodeValues, WsEventTypeValues } from '../generated/protocol'
import type * as Ipc from '../ipc.types'
import type * as H from '../web-protocol.types'
import { WS_ERROR_CODES, WS_EVENT_TYPES } from '../web-protocol.types'
import type * as T from '../web-terminal-protocol.types'

/** `true` when each type is assignable to the other. */
type Same<A, B> = [A] extends [B] ? ([B] extends [A] ? true : false) : false
/** `true` when `A` is assignable to `B`. */
type Fits<A, B> = [A] extends [B] ? true : false
const check = <_ extends true>(): void => {}

// The hand-written types the generated ones do not replace describe the same
// wire.
check<Same<G.WebTerminalReply, T.WebTerminalReply>>()
check<Same<G.WebTerminalFrame, T.WebTerminalFrame>>()
check<Same<G.TerminalStateSnapshot, Ipc.TerminalStateSnapshot>>()
check<Same<G.GitStatus, Ipc.GitStatus>>()
check<Same<G.SpawnedTerminal, Ipc.SpawnedTerminal>>()
check<Same<G.TerminalAttachResult, Ipc.TerminalAttachResult>>()
check<Same<G.RotatedClaim, Ipc.RotatedClaim>>()
check<Same<G.TerminalSpawnOptions, Ipc.TerminalSpawnOptions>>()

// Where the host sends more than the hand-written contract says, the generated
// type is wider. Each gap is pinned here so it cannot grow unnoticed.
type ExtraEventTypes =
  | 'question_request'
  | 'question_cancelled'
  | 'elicitation_request'
  | 'mcp_install_request'
  | 'mcp_install_resolved'
  | 'mcp_registry_changed'
  | 'browser_ticket_closed'
  | 'claude_limit_action'
  | 'codex_limit_action'
check<Same<Exclude<G.WsEventType, H.WsEventType>, ExtraEventTypes>>()
check<Fits<H.WsEventType, G.WsEventType>>()
check<Same<Omit<G.WsEvent, 'type'>, Omit<H.WsEvent, 'type'>>>()
check<Fits<H.WsEvent, G.WsEvent>>()
check<Same<Omit<G.SessionSnapshotEvent, 'events'>, Omit<H.SessionSnapshotEvent, 'events'>>>()
// `err.code` also carries SCREAMING_SNAKE_CASE domain codes (`STORE_UNAVAILABLE`).
check<Same<G.WsError['code'], string>>()
check<Same<Omit<G.WsError, 'code'>, Omit<H.WsError, 'code'>>>()
check<Fits<H.WsError, G.WsError>>()
check<Fits<H.WsReply, G.WsReply>>()
// `/terminal/ws` request payloads stay open until each request is typed.
check<Same<Omit<G.WebTerminalRequest, 'payload'>, Omit<T.WebTerminalRequest, 'payload'>>>()
check<Fits<T.WebTerminalRequest, G.WebTerminalRequest>>()

type Schema = Record<string, unknown> | boolean
interface Document {
  $defs: Record<string, Schema>
}

const FIXTURES = join(
  dirname(fileURLToPath(import.meta.url)),
  '../../../../crates/troupe-protocol/fixtures'
)
const readJson = (name: string): unknown => JSON.parse(readFileSync(join(FIXTURES, name), 'utf8'))
const schema = JSON.parse(
  readFileSync(join(FIXTURES, '../schema/protocol.schema.json'), 'utf8')
) as Document

/** The JSON Schema subset the crate emits; returns the first mismatch. */
function mismatch(value: unknown, node: Schema, path = '$'): string | null {
  if (node === true) return null
  if (node === false) return `${path}: no value allowed`
  const ref = node.$ref as string | undefined
  if (ref) return mismatch(value, schema.$defs[ref.replace('#/$defs/', '')], path)
  if ('const' in node && value !== node.const) return `${path}: want ${String(node.const)}`
  const choices = (node.anyOf ?? node.oneOf) as Schema[] | undefined
  if (choices && !choices.some((choice) => mismatch(value, choice, path) === null)) {
    return `${path}: matches no variant`
  }
  for (const part of (node.allOf as Schema[] | undefined) ?? []) {
    const error = mismatch(value, part, path)
    if (error) return error
  }
  if (Array.isArray(node.enum) && !node.enum.includes(value)) return `${path}: not in enum`
  const types = ([] as string[]).concat((node.type as string | string[] | undefined) ?? [])
  if (types.length > 0 && !types.some((type) => isType(value, type))) {
    return `${path}: want ${types.join('|')}`
  }
  if (Array.isArray(value) && node.items) {
    for (const [index, item] of value.entries()) {
      const error = mismatch(item, node.items as Schema, `${path}[${index}]`)
      if (error) return error
    }
  }
  if (isType(value, 'object')) {
    const record = value as Record<string, unknown>
    for (const key of (node.required as string[] | undefined) ?? []) {
      if (!(key in record)) return `${path}.${key}: missing`
    }
    const properties = (node.properties ?? {}) as Record<string, Schema>
    for (const [key, item] of Object.entries(record)) {
      const rule = properties[key] ?? (node.additionalProperties as Schema | undefined)
      const error = rule === undefined ? null : mismatch(item, rule, `${path}.${key}`)
      if (error) return error
    }
  }
  return null
}

function isType(value: unknown, type: string): boolean {
  switch (type) {
    case 'null':
      return value === null
    case 'integer':
      return Number.isInteger(value)
    case 'array':
      return Array.isArray(value)
    case 'object':
      return typeof value === 'object' && value !== null && !Array.isArray(value)
    default:
      return typeof value === type
  }
}

interface Handwritten {
  file: string
  name: string
  _handwritten: boolean
  frame: unknown
}

function fixtures(file: string): Array<[string, unknown]> {
  const live = Object.entries(readJson(file) as Record<string, unknown>)
  const handwritten = (readJson('handwritten.json') as Handwritten[])
    .filter((entry) => entry.file === file)
    .map((entry): [string, unknown] => [entry.name, entry.frame])
  return [...live, ...handwritten]
}

const FILE_TYPES: Array<[string, string]> = [
  ['ws-request.json', 'WsRequest'],
  ['ws-reply.json', 'WsServerFrame'],
  ['ws-event.json', 'WsServerFrame'],
  ['ws-frame.json', 'WsServerFrame'],
  ['ws-watch.json', 'WsServerFrame'],
  ['terminal-request.json', 'WebTerminalRequest'],
  ['terminal-reply.json', 'WebTerminalFrame'],
  ['terminal-frame.json', 'WebTerminalFrame']
]

describe('troupe-protocol fixtures', () => {
  it.each(FILE_TYPES)('every frame in %s matches %s', (file, type) => {
    const frames = fixtures(file)
    expect(frames.length).toBeGreaterThan(0)
    const failures = frames
      .map(([name, frame]) => [name, mismatch(frame, schema.$defs[type])] as const)
      .filter(([, error]) => error !== null)
    expect(failures).toEqual([])
  })

  it('typed reply payloads match their generated types', () => {
    const replies = Object.fromEntries(fixtures('ws-reply.json')) as Record<string, G.WsOkReply>
    const typed: Array<[string, string]> = [
      ['authenticate', 'AcpAuthenticateReply'],
      ['subscribe', 'SessionSubscribeReply'],
      ['recover_session_snapshot', 'SessionSnapshotEvent'],
      ['get_session_cursor', 'SessionCursorReply'],
      ['store_read', 'StoreReadReply'],
      ['watch', 'WsWatchReply'],
      ['mark_chat_read', 'ChatReadMarkReply'],
      ['canvas_mount', 'WsCanvasMountReply']
    ]
    for (const [request, type] of typed) {
      expect([request, mismatch(replies[request].payload, schema.$defs[type])]).toEqual([
        request,
        null
      ])
    }
    const sessions = [
      replies.list_persisted_sessions,
      replies['list_persisted_sessions.quick_chat']
    ].flatMap((reply) => reply.payload as G.PersistedSessionSummary[])
    expect(sessions.length).toBeGreaterThan(0)
    for (const session of sessions) {
      expect(mismatch(session, schema.$defs.PersistedSessionSummary)).toBeNull()
    }
    expect(sessions.map((session) => session.quickChatId).filter(Boolean)).toEqual(['qc-fixture'])
  })

  it('a watch frame carries exactly one of state and delta', () => {
    const base = {
      type: 'watch',
      watch: 'projects',
      scope: '',
      revision: { hostEpoch: 'epoch', n: 1 }
    }
    const frame = schema.$defs.WsWatchFrame
    expect(mismatch({ ...base, reset: true, state: {} }, frame)).toBeNull()
    expect(mismatch({ ...base, delta: {} }, frame)).toBeNull()
    expect(mismatch({ ...base, reset: true, state: {}, delta: {} }, frame)).toBe(
      '$: matches no variant'
    )
    expect(mismatch(base, frame)).toBe('$: matches no variant')
  })

  it('the checker rejects a frame of the wrong shape', () => {
    const gap = { type: 'gap', sid: 7 }
    expect(mismatch(gap, schema.$defs.WsGapFrame)).toBe('$.sid: want string')
    expect(mismatch({ id: 'r1', ok: 'yes' }, schema.$defs.WsReply)).toBe('$: matches no variant')
  })

  it('every hand-written fixture is marked', () => {
    for (const entry of readJson('handwritten.json') as Handwritten[]) {
      expect([entry.file, entry.name, entry._handwritten]).toEqual([entry.file, entry.name, true])
    }
  })
})

describe('generated constants', () => {
  it('match the hand-written contract', () => {
    expect([...WsErrorCodeValues]).toEqual(Object.values(WS_ERROR_CODES))
    expect(WsEventTypeValues.slice(0, WS_EVENT_TYPES.length)).toEqual([...WS_EVENT_TYPES])
    expect(WsEventTypeValues.slice(WS_EVENT_TYPES.length)).toEqual([
      'question_request',
      'question_cancelled',
      'elicitation_request',
      'mcp_install_request',
      'mcp_install_resolved',
      'mcp_registry_changed',
      'browser_ticket_closed',
      'claude_limit_action',
      'codex_limit_action'
    ])
  })
})
