/**
 * ACP stdio agent that plays back the scripted chat in `script.ts`, one turn
 * per prompt, with word-chunked streaming: thoughts, prose, tool calls, an
 * edit with its diff and a table in one transcript.
 */

import { createInterface } from 'node:readline'
import { type Step, scriptedTurn, type ToolStep } from './script'

type Id = number | string

interface Incoming {
  id?: Id
  method?: string
  params?: {
    sessionId?: string
    cwd?: string
  }
}

const CHUNK_WORDS = 4
const CHUNK_DELAY_MS = 18
const TOOL_RUN_MS = 250

const sessions = new Map<string, { cwd: string; prompts: number }>()
let sessionCounter = 0
let cancelled = false

function send(message: object): void {
  process.stdout.write(`${JSON.stringify({ jsonrpc: '2.0', ...message })}\n`)
}

function update(sessionId: string, body: Record<string, unknown>): void {
  send({ method: 'session/update', params: { sessionId, update: body } })
}

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms))

/** Split prose into runs of a few words, keeping every whitespace character. */
function chunks(text: string): string[] {
  const parts = text.split(/(\s+)/)
  const out: string[] = []
  for (let i = 0; i < parts.length; i += CHUNK_WORDS * 2) {
    out.push(parts.slice(i, i + CHUNK_WORDS * 2).join(''))
  }
  return out.filter((chunk) => chunk.length > 0)
}

async function stream(sessionId: string, kind: string, text: string): Promise<void> {
  for (const chunk of chunks(text)) {
    if (cancelled) return
    update(sessionId, { sessionUpdate: kind, content: { type: 'text', text: chunk } })
    await sleep(CHUNK_DELAY_MS)
  }
}

async function runTool(sessionId: string, tool: ToolStep, turn: number): Promise<void> {
  const toolCallId = `${tool.id}-${turn}`
  update(sessionId, {
    sessionUpdate: 'tool_call',
    toolCallId,
    title: tool.title,
    kind: tool.kind,
    status: 'in_progress',
    locations: tool.locations,
    rawInput: tool.rawInput,
    ...(tool.content && { content: tool.content })
  })
  await sleep(TOOL_RUN_MS)
  update(sessionId, {
    sessionUpdate: 'tool_call_update',
    toolCallId,
    status: 'completed',
    ...(tool.output !== undefined && {
      rawOutput: { output: tool.output },
      content: [
        ...(tool.content ?? []),
        { type: 'content', content: { type: 'text', text: tool.output } }
      ]
    })
  })
}

async function playTurn(sessionId: string, steps: Step[], turn: number): Promise<void> {
  for (const step of steps) {
    if (cancelled) return
    if (step.step === 'thought') await stream(sessionId, 'agent_thought_chunk', step.text)
    else if (step.step === 'say') await stream(sessionId, 'agent_message_chunk', step.text)
    else await runTool(sessionId, step, turn)
  }
}

async function handlePrompt(id: Id, sessionId: string): Promise<void> {
  const session = sessions.get(sessionId)
  if (!session) {
    send({ id, error: { code: -32602, message: `unknown session ${sessionId}` } })
    return
  }
  cancelled = false
  try {
    const turn = session.prompts++
    await playTurn(sessionId, scriptedTurn(turn, session.cwd), turn)
    send({ id, result: { stopReason: cancelled ? 'cancelled' : 'end_turn' } })
  } catch (error) {
    console.error(`[chat-agent] prompt failed: ${error}`)
    send({ id, error: { code: -32603, message: String(error) } })
  }
}

function handle(message: Incoming): void {
  const { id, method, params } = message
  if (method === 'initialize') {
    send({
      id,
      result: {
        protocolVersion: 1,
        agentCapabilities: {
          loadSession: false,
          // As Claude Code's and Codex's adapters: the composer embeds attached files.
          promptCapabilities: { image: true, embeddedContext: true },
          sessionCapabilities: { list: false, resume: false, close: false }
        },
        authMethods: []
      }
    })
  } else if (method === 'session/new') {
    const sessionId = `chat-${process.pid}-${++sessionCounter}`
    sessions.set(sessionId, { cwd: params?.cwd ?? process.cwd(), prompts: 0 })
    send({ id, result: { sessionId } })
  } else if (method === 'session/prompt' && id !== undefined) {
    void handlePrompt(id, params?.sessionId ?? '')
  } else if (method === 'session/cancel') {
    cancelled = true
  } else if (id !== undefined && method !== undefined) {
    send({ id, error: { code: -32601, message: `method not found: ${method}` } })
  }
}

if (import.meta.main) {
  for await (const line of createInterface({ input: process.stdin })) {
    if (!line.trim()) continue
    try {
      handle(JSON.parse(line) as Incoming)
    } catch (error) {
      console.error(`[chat-agent] bad frame: ${error}`)
    }
  }
}
