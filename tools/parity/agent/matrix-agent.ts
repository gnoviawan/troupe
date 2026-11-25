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
 * - `matrix tools` runs one call of each card shape (a file read, an edit
 *   with its diff, a command with output, a terminal, a failed call), then
 *   replies `TOOLS_REPLY`.
 * - `matrix error` fails the turn with `TURN_ERROR` (the error notice).
 * - `matrix form` asks a form elicitation (`elicitation/create`: a required
 *   whole number and an optional pick), then replies with the answer.
 *
 * With `MATRIX_AGENT_OPTIONS=1` in its environment, each new session also
 * offers modes and a model option (the composer's mode chip and model
 * selector), the proxy answers `session/set_mode` and
 * `session/set_config_option` itself, and every turn it answers reports a
 * growing context usage (the context ring). Off by default, so the matrix's
 * other surfaces keep their reference frames.
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
  subagent: 'matrix subagent',
  tools: 'matrix tools',
  error: 'matrix error',
  form: 'matrix form'
} as const
export const FORM_MESSAGE = 'How many test runs should I do?'
export const TOOLS_REPLY = 'The tool calls are done.'
export const TURN_ERROR = 'Stream closed: NGHTTP2_INTERNAL_ERROR'
const OLD_ROUTE = Array.from({ length: 14 }, (_, n) => `const route${n + 1} = '/r${n + 1}'`).join(
  '\n'
)
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
  say(
    sessionId,
    reply.result?.cancelled ? 'Question answer: cancelled' : `Question answer: ${values.join(', ')}`
  )
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

/** One call: announced in progress, then completed (or failed) with `done`. */
async function toolCall(
  sessionId: string,
  call: Record<string, unknown>,
  done: Record<string, unknown>
): Promise<void> {
  update(sessionId, { sessionUpdate: 'tool_call', status: 'in_progress', ...call })
  await sleep(150)
  update(sessionId, { sessionUpdate: 'tool_call_update', toolCallId: call.toolCallId, ...done })
}

async function tools(sessionId: string): Promise<void> {
  const file = 'src/renderer/lib/routes.ts'
  await toolCall(
    sessionId,
    {
      toolCallId: 'matrix-read',
      title: `Read ${file}`,
      kind: 'read',
      locations: [{ path: file }],
      rawInput: { path: file, offset: 1, limit: 14 }
    },
    { status: 'completed', rawOutput: { output: OLD_ROUTE } }
  )
  const newRoute = OLD_ROUTE.replace("route9 = '/r9'", "route9 = '/nine'")
  await toolCall(
    sessionId,
    {
      toolCallId: 'matrix-edit',
      title: `Edit ${file}`,
      kind: 'edit',
      locations: [{ path: file }],
      rawInput: { path: file }
    },
    {
      status: 'completed',
      content: [{ type: 'diff', path: file, oldText: OLD_ROUTE, newText: newRoute }]
    }
  )
  await toolCall(
    sessionId,
    {
      toolCallId: 'matrix-run',
      title: 'Run bun test router',
      kind: 'execute',
      rawInput: { command: 'bun test router' }
    },
    {
      status: 'completed',
      rawOutput: { stdout: ' 6 pass\n 0 fail\nRan 6 tests across 1 file. [812ms]' }
    }
  )
  await toolCall(
    sessionId,
    {
      toolCallId: 'matrix-terminal',
      title: 'Run bun run dev',
      kind: 'execute',
      rawInput: { command: 'bun run dev' }
    },
    { status: 'completed', content: [{ type: 'terminal', terminalId: 'term-1' }] }
  )
  await toolCall(
    sessionId,
    {
      toolCallId: 'matrix-failed',
      title: 'Search "routeX"',
      kind: 'search',
      rawInput: { pattern: 'routeX' }
    },
    { status: 'failed', rawOutput: { output: 'rg: no matches for routeX' } }
  )
  say(sessionId, TOOLS_REPLY)
}

async function form(sessionId: string): Promise<void> {
  const reply = (await request('elicitation/create', {
    sessionId,
    mode: 'form',
    message: FORM_MESSAGE,
    requestedSchema: {
      type: 'object',
      properties: {
        runs: { type: 'integer', title: 'Runs', description: 'Whole runs of the suite.' },
        branch: { type: 'string', title: 'Branch', enum: ['main', 'dev'] }
      },
      required: ['runs']
    }
  })) as Frame & { result?: { action?: string; content?: unknown } }
  say(
    sessionId,
    `Form answer: ${reply.result?.action} ${JSON.stringify(reply.result?.content ?? null)}`
  )
}

async function failTurn(_sessionId: string): Promise<void> {
  throw new Error(TURN_ERROR)
}

let turnSession = ''

export const OPTIONS_ENV = 'MATRIX_AGENT_OPTIONS'
const OPTIONS = process.env[OPTIONS_ENV] === '1'
export const MODES = [
  { id: 'default', name: 'Default', description: 'Ask before edits and commands' },
  { id: 'acceptEdits', name: 'Accept edits', description: 'Edit files without asking' },
  { id: 'plan', name: 'Plan', description: 'Read and plan, change nothing' }
]
export const MODELS = [
  { value: 'matrix-large', name: 'Matrix Large', description: 'The most capable model' },
  { value: 'matrix-fast', name: 'Matrix Fast', description: 'Quicker answers' }
]
const CONTEXT_SIZE = 200_000
const sessionModels = new Map<string, string>()
const usageTurns = new Map<string, number>()

function modelOption(sessionId: string): object {
  return {
    id: 'model',
    name: 'Model',
    category: 'model',
    type: 'select',
    currentValue: sessionModels.get(sessionId) ?? MODELS[0].value,
    options: MODELS
  }
}

/** A context report that grows by a turn's worth each time. */
function reportUsage(sessionId: string): void {
  if (!OPTIONS) return
  const turns = (usageTurns.get(sessionId) ?? 0) + 1
  usageTurns.set(sessionId, turns)
  update(sessionId, {
    sessionUpdate: 'usage_update',
    used: 18_000 + turns * 24_000,
    size: CONTEXT_SIZE
  })
}

/** `session/new` from the scenario agent, with the modes and the model option added. */
function withOptions(line: string, newSessionIds: Set<Id>): string {
  if (!OPTIONS || newSessionIds.size === 0) return line
  const frame = JSON.parse(line) as { id?: Id; result?: { sessionId?: string } }
  if (frame.id === undefined || !newSessionIds.delete(frame.id) || !frame.result?.sessionId)
    return line
  const sessionId = frame.result.sessionId
  usageTurns.set(sessionId, 0)
  return JSON.stringify({
    ...frame,
    result: {
      ...frame.result,
      modes: { currentModeId: MODES[0].id, availableModes: MODES },
      configOptions: [modelOption(sessionId)]
    }
  })
}

/** Answers the option setters the scenario agent lacks; true when it did. */
function setOption(frame: Frame & { params?: Record<string, unknown> }): boolean {
  if (!OPTIONS || frame.id === undefined) return false
  const sessionId = String(frame.params?.sessionId ?? '')
  if (frame.method === 'session/set_mode') {
    send({ id: frame.id, result: {} })
    update(sessionId, { sessionUpdate: 'current_mode_update', currentModeId: frame.params?.modeId })
    return true
  }
  if (frame.method === 'session/set_config_option' && frame.params?.configId === 'model') {
    sessionModels.set(sessionId, String(frame.params?.value))
    send({ id: frame.id, result: { configOptions: [modelOption(sessionId)] } })
    return true
  }
  return false
}

const HANDLERS: Record<string, (sessionId: string) => Promise<void>> = {
  [MATRIX_PROMPTS.plan]: plan,
  [MATRIX_PROMPTS.permission]: permission,
  [MATRIX_PROMPTS.hold]: hold,
  [MATRIX_PROMPTS.question]: question,
  [MATRIX_PROMPTS.subagent]: subagent,
  [MATRIX_PROMPTS.tools]: tools,
  [MATRIX_PROMPTS.error]: failTurn,
  [MATRIX_PROMPTS.form]: form
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
    reportUsage(sessionId)
    send({ id, result: { stopReason: cancelled ? 'cancelled' : 'end_turn' } })
  } catch (error) {
    console.error(`[matrix-agent] prompt failed: ${error}`)
    const message = error instanceof Error ? error.message : String(error)
    send({ id, error: { code: -32603, message, data: { retryable: true } } })
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
  const newSessionIds = new Set<Id>()
  void (async () => {
    for await (const line of createInterface({ input: child.stdout })) {
      if (line.trim()) process.stdout.write(`${withOptions(line, newSessionIds)}\n`)
    }
  })()
  for await (const line of createInterface({ input: process.stdin })) {
    if (!line.trim()) continue
    try {
      const frame = JSON.parse(line) as Frame
      if (frame.method === 'session/new' && frame.id !== undefined) newSessionIds.add(frame.id)
      if (!setOption(frame) && !intercept(frame)) child.stdin.write(`${line}\n`)
    } catch (error) {
      console.error(`[matrix-agent] bad frame: ${error}`)
    }
  }
  child.kill()
}
