/**
 * Keep subagent work inside its subagent. Claude stamps
 * `_meta.claudeCode.parentToolUseId` on every tool call and text chunk that a
 * subagent makes (the host stamps the same key on native child-session
 * traffic). The store lifts that id onto the child tool call, and keeps the
 * subagent's text on its dispatch call, split at its tool calls, instead of
 * in the main reply.
 */

import { asRecord } from '@/components/chat/tool-call-summary'
import type { MessageChunkEvent, SubagentMessage, ToolCall } from '@/lib/acp-api'
import { logFrontendError } from '@/lib/log-api'
import { nextSeq } from './shared-state'
import { withSubagentRecord } from './subagent-lifecycle'
import type { AcpState } from './types'

const MAX_PARENT_ID_CHARS = 512
/** Longest ACP `messageId` a subagent message keeps; a longer one counts as absent. */
const MAX_MESSAGE_ID_CHARS = 256
/** Longest parent sender label kept; matches host `subagents::MAX_TEXT_CHARS`. */
const MAX_SENDER_CHARS = 512
/** Text kept across a dispatch's subagent messages; the oldest goes first. */
export const MAX_SUBAGENT_TEXT_CHARS = 8_000
/** Subagent messages kept on a dispatch call; the oldest go first. */
export const MAX_SUBAGENT_MESSAGES = 200

const loggedOrphans = new Set<string>()

function parentToolCallId(meta: unknown): string | undefined {
  const id = asRecord(asRecord(meta)?.claudeCode)?.parentToolUseId
  return typeof id === 'string' && id.length > 0 && id.length <= MAX_PARENT_ID_CHARS
    ? id
    : undefined
}

/** `call` with the subagent that made it; the same object when none did. */
export function withParentToolCallId<T extends { toolCallId: string; _meta?: unknown }>(
  call: T
): T {
  const parent = parentToolCallId(call._meta)
  return parent && parent !== call.toolCallId ? { ...call, parentToolCallId: parent } : call
}

/** Every ingest-time subagent field: the host's record and control marker, parent dispatch, and the subagent's messages. */
export function withSubagentIngest<T extends { toolCallId: string; _meta?: unknown }>(call: T): T {
  return withParsedSubagentMessages(withParentToolCallId(withSubagentRecord(call)))
}

function isSubagentMessage(value: unknown): value is SubagentMessage {
  const entry = asRecord(value)
  return (
    typeof entry?.seq === 'number' &&
    Number.isFinite(entry.seq) &&
    typeof entry.timestamp === 'number' &&
    Number.isFinite(entry.timestamp) &&
    typeof entry.text === 'string'
  )
}

/** A wire `subagentMessages` value as messages: well-formed entries only, within the bounds. */
export function parseSubagentMessages(value: unknown): SubagentMessage[] | undefined {
  if (!Array.isArray(value)) return undefined
  const messages = value
    .filter(isSubagentMessage)
    .map(({ seq, timestamp, text, messageId, sender }) => ({
      seq,
      timestamp,
      text,
      ...boundedMessageId(messageId),
      ...boundedSender(sender)
    }))
  return messages.length > 0 ? boundSubagentMessages(messages) : undefined
}

/** `call` with its wire `subagentMessages` parsed, or the key removed when nothing parses. */
function withParsedSubagentMessages<T extends object>(call: T): T {
  if (!('subagentMessages' in call)) return call
  const { subagentMessages, ...rest } = call as T & { subagentMessages?: unknown }
  const parsed = parseSubagentMessages(subagentMessages)
  return (parsed ? { ...rest, subagentMessages: parsed } : rest) as T
}

/** The newest messages within both bounds; the oldest kept one loses its head. */
function boundSubagentMessages(messages: SubagentMessage[]): SubagentMessage[] {
  const kept =
    messages.length > MAX_SUBAGENT_MESSAGES ? messages.slice(-MAX_SUBAGENT_MESSAGES) : messages
  let budget = MAX_SUBAGENT_TEXT_CHARS
  for (let i = kept.length - 1; i >= 0; i--) {
    const { text } = kept[i]
    if (text.length >= budget) {
      const oldest = text.length > budget ? { ...kept[i], text: text.slice(-budget) } : kept[i]
      return [oldest, ...kept.slice(i + 1)]
    }
    budget -= text.length
  }
  return kept
}

/** Counted in code points, as the host counts `chars()`, so both keep the same ids. */
function boundedMessageId(id: unknown): { messageId?: string } {
  if (typeof id !== 'string' || id.length === 0 || id.length > 2 * MAX_MESSAGE_ID_CHARS) return {}
  let count = 0
  for (const _ of id) count++
  return count <= MAX_MESSAGE_ID_CHARS ? { messageId: id } : {}
}

/** Parent agent label, counted in code points to match the host bound. */
function boundedSender(sender: unknown): { sender?: string } {
  if (typeof sender !== 'string') return {}
  const trimmed = sender.trim()
  if (trimmed.length === 0 || trimmed.length > 2 * MAX_SENDER_CHARS) return {}
  let count = 0
  for (const _ of trimmed) count++
  return count <= MAX_SENDER_CHARS ? { sender: trimmed } : {}
}

/**
 * The subagent text a chunk adds: agent text chunks, or `user` text chunks
 * stamped with a parent `sender` (`subagent_send`). Returns null for thought,
 * unattributed user echoes, and non-text chunks.
 */
export function subagentChunkText(
  role: unknown,
  content: unknown,
  sender?: unknown
): { text: string; sender?: string } | null {
  const block = asRecord(content)
  if (block?.type !== 'text' || typeof block.text !== 'string' || block.text.length === 0) {
    return null
  }
  const { sender: validSender } = boundedSender(sender)
  if (role === 'agent' && validSender === undefined) {
    return { text: block.text }
  }
  if (role === 'user' && validSender !== undefined) {
    return { text: block.text, sender: validSender }
  }
  return null
}

/**
 * `call` with a subagent text chunk (or parent follow-up) added. An agent chunk
 * extends the last agent message, unless it carries a different ACP `messageId`,
 * or one of the subagent's tool calls arrived after that message started. A
 * parent-attributed message (`sender` present) always opens its own message,
 * and the next agent chunk starts a fresh message after it.
 */
export function withSubagentText(
  call: ToolCall,
  text: string,
  stamp: { seq: number; timestamp: number; messageId?: string; sender?: string },
  calls: readonly ToolCall[]
): ToolCall {
  const messages = call.subagentMessages ?? []
  const last = messages[messages.length - 1]
  const { messageId } = boundedMessageId(stamp.messageId)
  const { sender } = boundedSender(stamp.sender)
  const opens =
    !last ||
    sender !== undefined ||
    last.sender !== undefined ||
    (messageId !== undefined && last.messageId !== messageId) ||
    calls.some(
      (tc) =>
        tc.parentToolCallId === call.toolCallId && typeof tc.seq === 'number' && tc.seq > last.seq
    )
  const next = opens
    ? [
        ...messages,
        {
          seq: stamp.seq,
          timestamp: stamp.timestamp,
          text,
          ...(messageId ? { messageId } : {}),
          ...(sender ? { sender } : {})
        }
      ]
    : [...messages.slice(0, -1), { ...last, text: last.text + text }]
  return { ...call, subagentMessages: boundSubagentMessages(next) }
}

/**
 * Append a subagent text chunk to its dispatch call. Thought chunks,
 * unattributed user echoes, and non-text blocks are not kept. A chunk whose
 * dispatch call is unknown is dropped (logged once per parent id): it never
 * joins the main reply.
 */
export function appendSubagentChunk(
  s: AcpState,
  e: MessageChunkEvent & { parentToolCallId: string },
  /** Host seq a live-turn replay keeps; a live chunk takes the next local seq. */
  replaySeq: number | null
): Partial<AcpState> {
  const session = s.sessions[e.sessionId]
  if (!session || (session.status === 'closed' && !session.replaying)) return {}
  const parsed = subagentChunkText(e.role, e.content, e.sender)
  if (parsed === null) return {}
  const list = s.toolCalls[e.sessionId] ?? []
  const idx = list.findIndex((tc) => tc.toolCallId === e.parentToolCallId)
  if (idx === -1) {
    const key = e.parentToolCallId.slice(0, 128)
    if (!loggedOrphans.has(key)) {
      if (loggedOrphans.size >= 256) loggedOrphans.clear()
      loggedOrphans.add(key)
      void logFrontendError({
        level: 'info',
        source: 'acp-store.subagentMessages',
        message: `Dropped subagent text for unknown dispatch ${key} in session ${e.sessionId}`
      })
    }
    return {}
  }
  const next = [...list]
  next[idx] = withSubagentText(
    list[idx],
    parsed.text,
    {
      seq: replaySeq ?? nextSeq(),
      timestamp: Date.now(),
      messageId: e.messageId,
      sender: parsed.sender
    },
    list
  )
  return { toolCalls: { ...s.toolCalls, [e.sessionId]: next } }
}

/** Reset the orphan log dedup (tests). */
export function _resetSubagentTranscriptLogsForTesting(): void {
  loggedOrphans.clear()
}
