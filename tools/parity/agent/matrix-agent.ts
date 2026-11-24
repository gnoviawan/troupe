/**
 * The matrix's ACP agent: the chat scenario agent (`scenario-agent.ts`)
 * behind a proxy that answers three extra prompts itself, so the chat surfaces
 * the scenario agent cannot reach still get a scripted reference:
 *
 * - `matrix plan` streams a plan update (the plan panel) and a short reply.
 * - `matrix permission` starts a tool call and asks the client for permission
 *   (the permission prompt), then replies with the option the user picked.
 * - `matrix hold` replies, then keeps the turn open for `HOLD_MS` (or until
 *   cancelled), so a second prompt lands in the prompt queue.
 * - `matrix question` asks a structured question (`_session/question`, the
 *   ask-user card), then replies with the values the user picked.
 * - `matrix subagent` announces two running subagents (`subagent_update`, the
 *   subagent panel); the first offers a stop. A `session/cancel` for a child
 *   ends it as cancelled, and the turn ends once no child runs.
 *
 * Every other frame passes through to the scenario agent unchanged. With
 * `PARITY_PROMPT_LOG` set, each prompt's blocks are appended to that file as
 * one JSON line, so two clients' sends can be compared byte for byte.
 */

import { spawn } from 'node:child_process'
import { appendFileSync } from 'node:fs'
import path from 'node:path'
import { createInterface } from 'node:readline'

type Id = number | string

interface Frame {
  id?: Id
  method?: string
  params?: { sessionId?: string; prompt?: Array<{ text?: string }> }
  result?: {
    outcome?: { outcome?: string; optionId?: string }
    values?: string[]
    cancelled?: boolean
  }
}

export const MATRIX_PROMPTS = {
  plan: 'matrix plan',
  permission: 'matrix permission',
  hold: 'matrix hold',
  question: 'matrix question',
  subagent: 'matrix subagent'
} as const
export const PLAN_ENTRIES = [
  'Read the router module',
  'Write the hash router tests',
  'Update the router docs'
]
export const PERMISSION_TITLE = 'Run the router test suite'
export const PERMISSION_GRANTED = 'Permission answer: allow-once'
export const HOLD_REPLY = 'Holding this turn open for the queue.'
export const QUESTION = 'Which branch should the router tests run on?'
export const QUESTION_OPTIONS = [
  { value: 'main', label: 'main (Recommended)', description: 'The default branch.' },
  { value: 'feature', label: 'feature/router', description: 'The branch with the route change.' }
]
export const SUBAGENTS = [
  { id: 'child-explore', title: 'Explore the router', stoppable: true },
  { id: 'child-tests', title: 'Write the router tests', stoppable: false }
]
const SUBAGENT_RUN_MS = 60_000
const HOLD_MS = 25_000

function send(frame: object): void {
  process.stdout.write(`${JSON.stringify({ jsonrpc: '2.0', ...frame })}\n`)
}

const update = (sessionId: string, body: Record<string, unknown>) =>
  send({ method: 'session/update', params: { sessionId, update: body } })
const say = (sessionId: string, text: string) =>
  update(sessionId, { sessionUpdate: 'agent_message_chunk', content: { type: 'text', text } })
const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms))

const pending = new Map<Id, (frame: Frame) => void>()
let requestCounter = 0
let cancelled = false

function request(method: string, params: object): Promise<Frame> {
  const id = `matrix-${++requestCounter}`
  return new Promise((resolve) => {
    pending.set(id, resolve)
    send({ id, method, params })
  })
}

async function plan(sessionId: string): Promise<void> {
  const statuses = ['completed', 'in_progress', 'pending']
  update(sessionId, {
    sessionUpdate: 'plan',
    entries: PLAN_ENTRIES.map((content, i) => ({
      content,
      priority: 'medium',
      status: statuses[i]
    }))
  })
  await sleep(200)
  say(sessionId, 'Here is the plan for the router work.')
}

async function permission(sessionId: string): Promise<void> {
  const toolCallId = `matrix-permission-${requestCounter}`
  const toolCall = {
    toolCallId,
    title: PERMISSION_TITLE,
    kind: 'execute',
    status: 'pending',
    rawInput: { command: 'bun test router' }
  }
  update(sessionId, { sessionUpdate: 'tool_call', ...toolCall })
  const reply = await request('session/request_permission', {
    sessionId,
    toolCall,
    options: [
      { optionId: 'allow-once', name: 'Allow', kind: 'allow_once' },
      { optionId: 'reject-once', name: 'Reject', kind: 'reject_once' }
    ]
  })
  const outcome = reply.result?.outcome
  const answer =
    outcome?.outcome === 'selected' ? (outcome.optionId ?? '') : (outcome?.outcome ?? 'none')
  update(sessionId, {
    sessionUpdate: 'tool_call_update',
    toolCallId,
    status: answer === 'allow-once' ? 'completed' : 'failed'
  })
  say(sessionId, `Permission answer: ${answer}`)
}

async function hold(sessionId: string): Promise<void> {
  say(sessionId, HOLD_REPLY)
  const until = Date.now() + HOLD_MS
  while (!cancelled && Date.now() < until) await sleep(100)
}

async function question(sessionId: string): Promise<void> {
  const reply = await request('_session/question', {
    sessionId,
    question: QUESTION,
    options: QUESTION_OPTIONS
  })
  const values = reply.result?.values ?? []
  say(sessionId, reply.result?.cancelled ? 'Question answer: cancelled' : `Question answer: ${values.join(', ')}`)
}

const running = new Set<string>()

function subagentUpdate(sessionId: string, child: string, body: Record<string, unknown>): void {
  update(sessionId, { sessionUpdate: 'subagent_update', sessionId: child, ...body })
}

async function subagent(sessionId: string): Promise<void> {
  for (const child of SUBAGENTS) {
    running.add(child.id)
    subagentUpdate(sessionId, child.id, {
      title: child.title,
      capabilities: child.stoppable ? { cancel: {} } : {},
      state: { state: 'running' }
    })
  }
  say(sessionId, 'Two subagents are working on the router.')
  const until = Date.now() + SUBAGENT_RUN_MS
  while (!cancelled && running.has(SUBAGENTS[0].id) && Date.now() < until) await sleep(100)
  for (const child of SUBAGENTS) {
    if (!running.delete(child.id)) continue
    subagentUpdate(sessionId, child.id, {
      capabilities: { cancel: null },
      state: { state: 'idle', stopReason: 'end_turn' }
    })
  }
}

/** A cancel for a running child ends that child, not the turn. */
function cancelChild(parent: string, child: string): boolean {
  if (!running.delete(child)) return false
  subagentUpdate(parent, child, {
    capabilities: { cancel: null },
    state: { state: 'idle', stopReason: 'cancelled' }
  })
  say(parent, `Stopped ${child}.`)
  return true
}

let turnSession = ''

const HANDLERS: Record<string, (sessionId: string) => Promise<void>> = {
  [MATRIX_PROMPTS.plan]: plan,
  [MATRIX_PROMPTS.permission]: permission,
  [MATRIX_PROMPTS.hold]: hold,
  [MATRIX_PROMPTS.question]: question,
  [MATRIX_PROMPTS.subagent]: subagent
}

async function answer(
  id: Id,
  sessionId: string,
  handler: (sessionId: string) => Promise<void>
): Promise<void> {
  cancelled = false
  turnSession = sessionId
  try {
    await handler(sessionId)
    send({ id, result: { stopReason: cancelled ? 'cancelled' : 'end_turn' } })
  } catch (error) {
    console.error(`[matrix-agent] prompt failed: ${error}`)
    send({ id, error: { code: -32603, message: String(error) } })
  }
}

/** True when the proxy handled the frame itself. */
function intercept(frame: Frame): boolean {
  if (frame.method === undefined && frame.id !== undefined && pending.has(frame.id)) {
    pending.get(frame.id)?.(frame)
    pending.delete(frame.id)
    return true
  }
  if (frame.method === 'session/cancel') {
    if (cancelChild(turnSession, frame.params?.sessionId ?? '')) return true
    cancelled = true
  }
  if (frame.method !== 'session/prompt' || frame.id === undefined) return false
  // Lane 4 compares the prompt each client sent, block for block.
  const promptLog = process.env.PARITY_PROMPT_LOG
  if (promptLog) appendFileSync(promptLog, `${JSON.stringify(frame.params?.prompt ?? [])}\n`)
  const text = (frame.params?.prompt ?? [])
    .map((block) => block.text ?? '')
    .join('')
    .trim()
  const handler = Object.hasOwn(HANDLERS, text) ? HANDLERS[text] : undefined
  if (!handler) return false
  void answer(frame.id, frame.params?.sessionId ?? '', handler)
  return true
}

if (import.meta.main) {
  const child = spawn('bun', [path.join(import.meta.dir, 'scenario-agent.ts')], {
    stdio: ['pipe', 'pipe', 'inherit']
  })
  void (async () => {
    for await (const line of createInterface({ input: child.stdout })) {
      if (line.trim()) process.stdout.write(`${line}\n`)
    }
  })()
  for await (const line of createInterface({ input: process.stdin })) {
    if (!line.trim()) continue
    try {
      if (!intercept(JSON.parse(line) as Frame)) child.stdin.write(`${line}\n`)
    } catch (error) {
      console.error(`[matrix-agent] bad frame: ${error}`)
    }
  }
  child.kill()
}
