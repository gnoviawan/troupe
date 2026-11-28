import { describe, expect, it } from 'vitest'
import {
  type AgentChatNotifySession,
  type AgentChatNotifySnapshot,
  bumpTurnEndNotice,
  decideAgentChatNotifications
} from './agent-chat-notify'

function session(overrides: Partial<AgentChatNotifySession> = {}): AgentChatNotifySession {
  return {
    projectId: 'p1',
    ephemeral: false,
    busy: false,
    queueLength: 0,
    turnEndSeq: 0,
    stopReason: null,
    backgroundRunning: false,
    ...overrides
  }
}

function snapshot(overrides: Partial<AgentChatNotifySnapshot> = {}): AgentChatNotifySnapshot {
  return {
    sessions: { s1: session() },
    permissions: [],
    questions: [],
    viewingSessionId: null,
    notifyTurnFinished: true,
    notifyNeedsYou: true,
    ...overrides
  }
}

describe('bumpTurnEndNotice', () => {
  it('stores the stop reason and increases seq', () => {
    const first = bumpTurnEndNotice({}, 's1', 'end_turn')
    const second = bumpTurnEndNotice(first, 's1', 'cancelled')
    expect(first.s1).toEqual({ seq: 1, stopReason: 'end_turn' })
    expect(second.s1).toEqual({ seq: 2, stopReason: 'cancelled' })
  })

  it('stores null when the stop reason is missing', () => {
    expect(bumpTurnEndNotice({}, 's1', undefined).s1.stopReason).toBeNull()
  })
})

describe('decideAgentChatNotifications', () => {
  it('stays silent for the first snapshot', () => {
    expect(decideAgentChatNotifications(null, snapshot())).toEqual([])
  })

  it.each([
    'end_turn',
    'refusal',
    'max_tokens',
    'max_turn_requests'
  ] as const)('notifies when a turn finishes with %s', (stopReason) => {
    const prev = snapshot({ sessions: { s1: session({ busy: true, turnEndSeq: 1 }) } })
    const next = snapshot({
      sessions: { s1: session({ busy: false, turnEndSeq: 2, stopReason }) }
    })
    expect(decideAgentChatNotifications(prev, next)).toEqual([
      { kind: 'turn-finished', sessionId: 's1' }
    ])
  })

  it('defers the finished ping while a background command still runs', () => {
    const prev = snapshot({ sessions: { s1: session({ busy: true, turnEndSeq: 1 }) } })
    const next = snapshot({
      sessions: {
        s1: session({ turnEndSeq: 2, stopReason: 'end_turn', backgroundRunning: true })
      }
    })
    expect(decideAgentChatNotifications(prev, next)).toEqual([
      { kind: 'turn-finish-deferred', sessionId: 's1' }
    ])
  })

  it('settles a deferred finish only once the chat is idle with no background command', () => {
    const deferredFinishes = new Set(['s1'])
    const base = { turnEndSeq: 2, stopReason: 'end_turn' as const }
    const waiting = snapshot({
      deferredFinishes,
      sessions: { s1: session({ ...base, backgroundRunning: true }) }
    })
    const wokeUp = snapshot({
      deferredFinishes,
      sessions: { s1: session({ ...base, busy: true }) }
    })
    const idle = snapshot({ deferredFinishes, sessions: { s1: session(base) } })

    expect(decideAgentChatNotifications(waiting, waiting)).toEqual([])
    expect(decideAgentChatNotifications(waiting, wokeUp)).toEqual([])
    expect(decideAgentChatNotifications(wokeUp, idle)).toEqual([
      { kind: 'turn-finished', sessionId: 's1', deferred: true }
    ])
  })

  it('stays silent for a cancelled turn', () => {
    const prev = snapshot({ sessions: { s1: session({ busy: true }) } })
    const next = snapshot({
      sessions: { s1: session({ turnEndSeq: 1, stopReason: 'cancelled' }) }
    })
    expect(decideAgentChatNotifications(prev, next)).toEqual([])
  })

  it('stays silent while a prompt is still queued', () => {
    const prev = snapshot({ sessions: { s1: session({ busy: true }) } })
    const next = snapshot({
      sessions: { s1: session({ turnEndSeq: 1, stopReason: 'end_turn', queueLength: 1 }) }
    })
    expect(decideAgentChatNotifications(prev, next)).toEqual([])
  })

  it('stays silent when the user is already viewing that chat', () => {
    const prev = snapshot()
    const next = snapshot({
      sessions: { s1: session({ turnEndSeq: 1, stopReason: 'end_turn' }) },
      viewingSessionId: 's1',
      permissions: [{ id: 'req-1', sessionId: 's1' }]
    })
    expect(decideAgentChatNotifications(prev, next)).toEqual([])
  })

  it('stays silent for an ephemeral session', () => {
    const prev = snapshot({ sessions: { s1: session({ ephemeral: true, busy: true }) } })
    const next = snapshot({
      sessions: { s1: session({ ephemeral: true, turnEndSeq: 1, stopReason: 'end_turn' }) },
      permissions: [{ id: 'req-1', sessionId: 's1' }]
    })
    expect(decideAgentChatNotifications(prev, next)).toEqual([])
  })

  it('notifies once for a new permission and a new question', () => {
    const prev = snapshot()
    const next = snapshot({
      permissions: [{ id: 'req-1', sessionId: 's1' }],
      questions: [{ id: 'q-1', sessionId: 's1' }]
    })
    expect(decideAgentChatNotifications(prev, next)).toEqual([
      { kind: 'needs-approval', sessionId: 's1', requestId: 'req-1' },
      { kind: 'has-question', sessionId: 's1', questionId: 'q-1' }
    ])
    expect(decideAgentChatNotifications(next, next)).toEqual([])
  })

  it('lets each switch silence only its own group', () => {
    const prev = snapshot({ sessions: { s1: session({ busy: true }) } })
    const finishedOff = snapshot({
      sessions: { s1: session({ turnEndSeq: 1, stopReason: 'end_turn' }) },
      permissions: [{ id: 'req-1', sessionId: 's1' }],
      notifyTurnFinished: false
    })
    expect(decideAgentChatNotifications(prev, finishedOff)).toEqual([
      { kind: 'needs-approval', sessionId: 's1', requestId: 'req-1' }
    ])

    const needsOff = snapshot({
      sessions: { s1: session({ turnEndSeq: 1, stopReason: 'end_turn' }) },
      permissions: [{ id: 'req-1', sessionId: 's1' }],
      notifyNeedsYou: false
    })
    expect(decideAgentChatNotifications(prev, needsOff)).toEqual([
      { kind: 'turn-finished', sessionId: 's1' }
    ])
  })
})
