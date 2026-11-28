/**
 * Scripted ACP agent for Lane 7 (`canvas-desktop.ts`).
 * On `session/new` it records Troupe's injected stdio MCP server (`troupe`); on `session/prompt`
 * it spawns that MCP server, calls `create_node` to add a visible "Agent Banner" rectangle inside
 * the Sample frame, and calls `save_file` so the change lands on disk.
 */

import { spawn } from 'node:child_process'
import { createInterface } from 'node:readline'

interface McpServerSpec {
  name?: string
  command?: string
  args?: string[]
  env?: Array<{ name: string; value: string }> | Record<string, string>
}

interface Frame {
  id?: number | string
  method?: string
  params?: {
    sessionId?: string
    mcpServers?: McpServerSpec[]
  }
}

let sessionCounter = 0
const sessionMcp = new Map<string, McpServerSpec>()

function send(frame: object): void {
  process.stdout.write(`${JSON.stringify({ jsonrpc: '2.0', ...frame })}\n`)
}

function say(sessionId: string, text: string): void {
  send({
    method: 'session/update',
    params: {
      sessionId,
      update: { sessionUpdate: 'agent_message_chunk', content: { type: 'text', text } }
    }
  })
}

function envOf(spec: McpServerSpec): Record<string, string> {
  const out: Record<string, string> = {}
  for (const [k, v] of Object.entries(process.env)) {
    if (v !== undefined) out[k] = v
  }
  if (Array.isArray(spec.env)) {
    for (const entry of spec.env) out[entry.name] = entry.value
  } else if (spec.env && typeof spec.env === 'object') {
    for (const [k, v] of Object.entries(spec.env)) out[k] = v
  }
  return out
}

async function runCanvasEdit(spec: McpServerSpec): Promise<string> {
  if (!spec.command) throw new Error('injected MCP server has no command')
  const child = spawn(spec.command, spec.args ?? [], {
    env: envOf(spec),
    stdio: ['pipe', 'pipe', 'inherit']
  })
  const pending = new Map<number, (msg: Record<string, unknown>) => void>()
  let nextId = 0

  const reader = createInterface({ input: child.stdout })
  const reading = (async () => {
    for await (const line of reader) {
      if (!line.trim()) continue
      try {
        const msg = JSON.parse(line) as Record<string, unknown>
        if (typeof msg.id === 'number' && pending.has(msg.id)) {
          pending.get(msg.id)?.(msg)
          pending.delete(msg.id)
        }
      } catch {
        // Ignore non-JSON lines on stdout.
      }
    }
  })()

  const call = (method: string, params: object): Promise<Record<string, unknown>> => {
    const id = ++nextId
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        pending.delete(id)
        reject(new Error(`MCP ${method} timed out`))
      }, 30_000)
      pending.set(id, (msg) => {
        clearTimeout(timer)
        resolve(msg)
      })
      child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', id, method, params })}\n`)
    })
  }

  try {
    await call('initialize', {
      protocolVersion: '2024-11-05',
      capabilities: {},
      clientInfo: { name: 'canvas-agent', version: '1.0.0' }
    })
    child.stdin.write(
      `${JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized', params: {} })}\n`
    )

    const created = await call('tools/call', {
      name: 'create_node',
      arguments: {
        type: 'rectangle',
        name: 'Agent Banner',
        parent: 2,
        x: 40,
        y: 285,
        width: 470,
        height: 75,
        props: { fill: '#FF6B00', cornerRadius: 12 }
      }
    })
    if (created.error) throw new Error(`create_node failed: ${JSON.stringify(created.error)}`)
    if ((created.result as { isError?: boolean } | undefined)?.isError) {
      throw new Error(`create_node returned error: ${JSON.stringify(created.result)}`)
    }

    const saved = await call('tools/call', {
      name: 'save_file',
      arguments: {}
    })
    if (saved.error) throw new Error(`save_file failed: ${JSON.stringify(saved.error)}`)
    if ((saved.result as { isError?: boolean } | undefined)?.isError) {
      throw new Error(`save_file returned error: ${JSON.stringify(saved.result)}`)
    }

    return `Created Agent Banner and saved: ${JSON.stringify(created.result)}`
  } finally {
    child.kill()
    await reading.catch(() => undefined)
  }
}

for await (const line of createInterface({ input: process.stdin })) {
  if (!line.trim()) continue
  let frame: Frame
  try {
    frame = JSON.parse(line) as Frame
  } catch {
    continue
  }
  if (frame.method === 'initialize' && frame.id !== undefined) {
    send({
      id: frame.id,
      result: {
        protocolVersion: 1,
        agentCapabilities: { loadSession: false, mcpCapabilities: { http: false, sse: false } },
        agentInfo: { name: 'canvas-agent', version: '1.0.0' }
      }
    })
  } else if (frame.method === 'session/new' && frame.id !== undefined) {
    const sessionId = `canvas-session-${++sessionCounter}`
    const servers = frame.params?.mcpServers ?? []
    const hostMcp = servers.find(
      (s) =>
        s.name === 'troupe' ||
        s.name === 'termul' ||
        (s.args ?? []).includes('--internal-mcp-plan-server')
    )
    if (hostMcp) sessionMcp.set(sessionId, hostMcp)
    send({ id: frame.id, result: { sessionId } })
  } else if (frame.method === 'session/prompt' && frame.id !== undefined) {
    const sessionId = frame.params?.sessionId ?? ''
    const spec = sessionMcp.get(sessionId)
    if (!spec) {
      send({
        id: frame.id,
        error: { code: -32603, message: 'No injected Troupe MCP server in session/new' }
      })
      continue
    }
    try {
      const summary = await runCanvasEdit(spec)
      say(sessionId, summary)
      send({ id: frame.id, result: { stopReason: 'end_turn' } })
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error)
      send({ id: frame.id, error: { code: -32603, message } })
    }
  } else if (frame.id !== undefined) {
    send({ id: frame.id, result: {} })
  }
}
