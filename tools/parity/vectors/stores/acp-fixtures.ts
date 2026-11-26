// Builders the ACP scenarios share: a seeded session and its events.
import type { VectorEvent } from '../record'

export const ev = (action: string, ...args: unknown[]): VectorEvent => ({ action, args })

export const S = 's1'
export const AGENT = 'agent-1'

function session(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    id: S,
    agentId: AGENT,
    cwd: '/work',
    projectId: 'p1',
    status: 'active',
    title: null,
    activeTurn: true,
    openTurnId: 'seed-turn',
    modes: null,
    models: null,
    configOptions: [],
    lastError: null,
    createdAt: 1,
    ...overrides
  }
}

export const idle = { activeTurn: false, openTurnId: null }

export function seeded(
  overrides: Record<string, unknown> = {},
  extra: Record<string, unknown> = {}
): Record<string, unknown> {
  return { sessions: { [S]: session(overrides) }, messages: { [S]: [] }, ...extra }
}

export function message(
  id: string,
  role: string,
  text: string,
  extra: Record<string, unknown> = {}
): Record<string, unknown> {
  return {
    id,
    role,
    blocks: [{ type: 'text', text }],
    streaming: false,
    timestamp: 1,
    ...extra
  }
}

export const text = (value: string) => ({ type: 'text', text: value })

export const chunk = (value: string, extra: Record<string, unknown> = {}, seq?: number) =>
  ev(
    '_onMessageChunk',
    { agentId: AGENT, sessionId: S, role: 'agent', content: text(value), ...extra },
    ...(seq === undefined ? [] : [seq])
  )

export const prompt = (content: unknown[], extra: Record<string, unknown> = {}, seq?: number) =>
  ev(
    '_onUserPrompt',
    { agentId: AGENT, sessionId: S, content, ...extra },
    ...(seq === undefined ? [] : [seq])
  )

export const toolCall = (call: Record<string, unknown>, seq?: number, sessionId = S) =>
  ev(
    '_onToolCall',
    { agentId: AGENT, sessionId, toolCall: call },
    ...(seq === undefined ? [] : [seq])
  )

export const toolUpdate = (update: Record<string, unknown>, seq?: number) =>
  ev(
    '_onToolCallUpdate',
    { agentId: AGENT, sessionId: S, update },
    ...(seq === undefined ? [] : [seq])
  )

export const watermark = (lastSeq: number, messages: unknown[] = [], toolCalls: unknown[] = []) =>
  ev('noteHistoryWatermark', S, { metadata: { id: S, lastSeq }, messages, toolCalls })

export const big = (length: number, char = 'x') => char.repeat(length)
