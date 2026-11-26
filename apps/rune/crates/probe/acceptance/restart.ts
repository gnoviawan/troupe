/**
 * troupe-probe against a seeded troupe-server: `projects`, then `chat --follow`
 * through a network cut and a server restart in the middle of streamed turns.
 * Exits non-zero unless the probe printed every seq of the server's own session
 * log exactly once, in order, with the same payloads and message text.
 *
 *   bun apps/rune/crates/probe/acceptance/restart.ts [--port 8193] [--out DIR]
 *
 * The run writes to DIR/accept-PORT (DIR defaults to /tmp/rune-r2a) and clears
 * only that directory.
 *
 * TROUPE_SERVER_BIN picks the server (default: this checkout's release build),
 * TROUPE_PROBE_BIN the probe (default: $CARGO_TARGET_DIR/debug/troupe-probe).
 */

import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync } from 'node:fs'
import net from 'node:net'
import path from 'node:path'
import { parseArgs } from 'node:util'
import { SCRIPT_PROMPTS } from '../../../../../tools/parity/agent/script'
import { defaultServerBinary } from '../../../../../tools/parity/server'
import { commandId, connectWs, type WsClient } from '../../../../../tools/parity/ws'

const { values } = parseArgs({
  options: { port: { type: 'string', default: '8193' }, out: { type: 'string' } }
})
const PORT = Number(values.port)
const PROXY_PORT = PORT + 1
const ROOT = path.join(values.out ?? '/tmp/rune-r2a', `accept-${PORT}`)
const STATE = path.join(ROOT, 'state')
const TOKEN = 'rune-probe-acceptance'
const BASE = `http://127.0.0.1:${PORT}`
const SERVER = defaultServerBinary()
const PROBE =
  process.env.TROUPE_PROBE_BIN ??
  path.join(
    process.env.CARGO_TARGET_DIR ?? path.resolve(import.meta.dir, '../../../target'),
    'debug/troupe-probe'
  )
const AGENT = {
  configId: 'scenario',
  name: 'Scenario',
  command: 'bun',
  args: [path.resolve(import.meta.dir, '../../../../../tools/parity/agent/scenario-agent.ts')],
  env: {},
  allowTerminal: false
}
const log = (message: string) => console.log(`[accept] ${message}`)

function startServer(): Bun.Subprocess {
  const home = path.join(STATE, 'home')
  mkdirSync(home, { recursive: true })
  const child = Bun.spawn(
    [
      SERVER,
      ...['--host', '127.0.0.1', '--port', String(PORT), '--state-dir', STATE],
      ...['--sessions-dir', path.join(STATE, 'sessions')],
      ...['--projects-file', path.join(STATE, 'projects.json')],
      ...['--store-file', path.join(STATE, 'store.json')]
    ],
    {
      env: { ...process.env, HOME: home, TROUPE_WEB_AUTH_TOKEN: TOKEN },
      stdout: Bun.file(path.join(ROOT, `server-${Date.now()}.log`)),
      stderr: 'inherit'
    }
  )
  log(`server pid ${child.pid}`)
  return child
}

async function healthy(): Promise<void> {
  const deadline = Date.now() + 15_000
  while (Date.now() < deadline) {
    const signal = AbortSignal.timeout(Math.max(1, deadline - Date.now()))
    const ok = await fetch(`${BASE}/health`, { signal }).then(
      (r) => r.ok,
      () => false
    )
    if (ok) return
    await Bun.sleep(100)
  }
  throw new Error('server never became healthy')
}

async function stopServer(child: Bun.Subprocess): Promise<void> {
  child.kill('SIGTERM')
  const exited = await Promise.race([
    child.exited.then(() => true),
    Bun.sleep(5000).then(() => false)
  ])
  if (!exited) child.kill('SIGKILL')
  await child.exited
  log(`server pid ${child.pid} exited ${child.exitCode ?? child.signalCode}`)
}

// The probe talks through this proxy, so the run can cut its network while
// the server keeps streaming.
const piped = new Set<net.Socket>()
let proxyOpen = true
const proxy = net.createServer((client) => {
  if (!proxyOpen) return client.destroy()
  const upstream = net.connect(PORT, '127.0.0.1')
  for (const socket of [client, upstream]) {
    piped.add(socket)
    socket.on('close', () => piped.delete(socket))
    socket.on('error', () => {})
  }
  client.pipe(upstream).pipe(client)
  client.on('close', () => upstream.destroy())
  upstream.on('close', () => client.destroy())
})
await new Promise<void>((resolve) => proxy.listen(PROXY_PORT, '127.0.0.1', resolve))

type Line = { at: number; frame: Record<string, unknown> & { seq: number; type: string } }
const lines: Line[] = []
const marks = { cut: 0, restored: 0, stopped: 0, restarted: 0 }
let sessionId = ''
let serverTranscript: { metadata: { lastSeq: number }; messages: ServerMessage[] } | null = null

rmSync(ROOT, { recursive: true, force: true })
mkdirSync(ROOT, { recursive: true })
let server = startServer()
let probe: Bun.Subprocess<'ignore', 'pipe', 'inherit'> | null = null
let reader: Promise<void> | null = null
try {
  await healthy()
  for (const key of ['a', 'b', 'c']) {
    const dir = path.join(ROOT, 'work', `proj-${key}`)
    mkdirSync(dir, { recursive: true })
    const res = await fetch(`${BASE}/projects`, {
      method: 'POST',
      headers: { Authorization: `Bearer ${TOKEN}`, 'content-type': 'application/json' },
      body: JSON.stringify({ id: `e2e-proj-${key}`, name: `proj-${key}`, path: dir, color: 'blue' })
    })
    if (!res.ok) throw new Error(`project ${key}: HTTP ${res.status}`)
  }
  const projects = Bun.spawnSync([PROBE, '--url', BASE, '--token', TOKEN, 'projects'])
  console.log(projects.stdout.toString())
  if (projects.exitCode !== 0) throw new Error(`probe projects: ${projects.stderr}`)

  const cwd = path.join(ROOT, 'work', 'proj-a')
  const connect = () =>
    connectWs({ baseUrl: BASE, token: TOKEN, deviceId: 'accept', clientInstanceId: 'accept' })
  let ws: WsClient = await connect()
  const { agentId } = await ws.request<{ agentId: string }>(
    'spawn_agent',
    { config: AGENT, projectId: 'e2e-proj-a' },
    { commandId: commandId('spawn') }
  )
  ;({ sessionId } = await ws.request<{ sessionId: string }>(
    'create_session',
    { agentId, cwd, projectId: 'e2e-proj-a' },
    { commandId: commandId('session') }
  ))
  log(`session ${sessionId}`)

  const follow = [`http://127.0.0.1:${PROXY_PORT}`, 'chat', sessionId, '--follow']
  const follower = Bun.spawn([PROBE, '--token', TOKEN, '--url', ...follow], { stdout: 'pipe' })
  probe = follower
  log(`probe pid ${follower.pid}`)
  reader = (async () => {
    let buffer = ''
    for await (const chunk of follower.stdout.pipeThrough(new TextDecoderStream())) {
      buffer += chunk
      for (let nl = buffer.indexOf('\n'); nl >= 0; nl = buffer.indexOf('\n')) {
        lines.push({ at: Date.now(), frame: JSON.parse(buffer.slice(0, nl)) })
        buffer = buffer.slice(nl + 1)
      }
    }
  })()
  const waitFor = async (what: string, done: () => boolean, ms = 30_000) => {
    const deadline = Date.now() + ms
    while (!done()) {
      if (Date.now() > deadline) throw new Error(`timed out waiting for ${what}`)
      await Bun.sleep(5)
    }
  }
  const count = (type: string) => lines.filter((l) => l.frame.type === type).length
  const prompt = (text: string) => {
    const turnId = commandId('turn')
    return ws.request(
      'send_prompt',
      { agentId, sessionId, text, turnId },
      { commandId: turnId, timeoutMs: 60_000 }
    )
  }

  const turn1 = prompt(SCRIPT_PROMPTS[0])
  await waitFor('turn 1 to stream', () => count('message_chunk') >= 10)
  proxyOpen = false
  for (const socket of piped) socket.destroy()
  marks.cut = Date.now()
  await Bun.sleep(1500)
  proxyOpen = true
  marks.restored = Date.now()
  log('network cut for 1.5 s while turn 1 streamed')
  await turn1
  await waitFor('the probe to catch up on turn 1', () => count('prompt_complete') === 1)

  const before = count('message_chunk')
  const turn2 = prompt(SCRIPT_PROMPTS[1]).then(
    () => 'completed',
    (e: Error) => e.message
  )
  await waitFor('turn 2 to stream', () => count('message_chunk') >= before + 10)
  await stopServer(server)
  marks.stopped = Date.now()
  const turn2Outcome = await turn2
  log(`turn 2 request: ${turn2Outcome}`)
  if (turn2Outcome === 'completed') {
    throw new Error('turn 2 completed before the server stopped; the restart was not mid-stream')
  }
  await Bun.sleep(2000)
  server = startServer()
  await healthy()
  marks.restarted = Date.now()

  ws.close()
  ws = await connect()
  serverTranscript = await ws.request('get_session_payload', { sessionId })
  ws.close()
  const lastSeq = serverTranscript?.metadata.lastSeq
  // Reconnect backoff reaches 8 s.
  await waitFor('the probe to reconnect after the restart', () =>
    lines.some((l) => l.at > marks.stopped && l.frame.type === 'auth_required')
  )
  await waitFor(`the probe to print seq ${lastSeq}`, () =>
    lines.some((l) => l.frame.seq === lastSeq)
  )
  await Bun.sleep(2000)
} finally {
  if (probe) {
    probe.kill('SIGTERM')
    await probe.exited
    await reader
  }
  await stopServer(server)
  proxy.close()
  await Bun.write(
    path.join(ROOT, 'probe-chat.jsonl'),
    lines.map((l) => JSON.stringify(l)).join('\n')
  )
}

type LogRecord = { seq: number; type: string; payload: Record<string, unknown> }
type ServerMessage = { role: string; seq: number; blocks: Array<{ text?: string }> }
type Message = { role: string; seq: number; text: string }

const canon = (value: unknown): string =>
  JSON.stringify(value, (_key, v) =>
    v && typeof v === 'object' && !Array.isArray(v)
      ? Object.fromEntries(Object.entries(v).sort())
      : v
  )

const disk = new Map<number, LogRecord>()
const sessions = path.join(STATE, 'sessions')
for (const dir of readdirSync(sessions, { withFileTypes: true }).filter((d) => d.isDirectory())) {
  for (const file of ['messages.jsonl', 'tool-calls.jsonl']) {
    const logPath = path.join(sessions, dir.name, file)
    if (!existsSync(logPath)) continue
    for (const record of readFileSync(logPath, 'utf8')
      .split('\n')
      .filter(Boolean)
      .map((l) => JSON.parse(l))) {
      if (record.sessionId === sessionId) disk.set(record.seq, record)
    }
  }
}

const events = lines.map((l) => l.frame).filter((f) => f.sid === sessionId && f.seq > 0)
const printed = new Map<number, number>()
for (const e of events) printed.set(e.seq, (printed.get(e.seq) ?? 0) + 1)
const diskSeqs = [...disk.keys()].sort((a, b) => a - b)

// The server keeps a slim tool record on disk (no locations, input or output),
// so tool events match on type, id and status; the rest match byte for byte.
const identity = (r: LogRecord) => {
  const tool = (r.payload.toolCall ?? r.payload.update) as Record<string, unknown> | undefined
  return tool ? canon([r.type, tool.toolCallId, tool.status]) : canon([r.type, r.payload])
}

// Mirrors the server's `fold_session_records`: a tool card, a finished turn
// (not an `interrupted` marker), a prompt or an agent switch closes the open
// chunk run; `tool_call_update` and the other events do not.
const fold = (records: LogRecord[]): Message[] => {
  const out: Message[] = []
  let open = null as Message | null
  for (const r of records) {
    type Block = { type?: string; text?: string }
    const content = r.payload.content as Block | Block[] | null
    if (r.type === 'message_chunk') {
      if (r.payload.parentToolCallId || content == null || Array.isArray(content)) continue
      const role = r.payload.role === 'thought' ? 'thought' : 'agent'
      if (open?.role !== role) {
        if (content.type === 'text' && !content.text) continue
        open = { role, seq: r.seq, text: '' }
        out.push(open)
      }
      open.text += content.text ?? ''
    } else if (r.type === 'user_prompt') {
      open = null
      if (Array.isArray(content)) {
        out.push({ role: 'user', seq: r.seq, text: content.map((c) => c.text ?? '').join('') })
      }
    } else if (
      r.type === 'agent_switch' ||
      r.type === 'tool_call' ||
      (r.type === 'prompt_complete' && r.payload.stopReason !== 'interrupted')
    ) {
      open = null
    }
  }
  return out
}
const probeMessages = fold(events as unknown as LogRecord[])
const serverMessages = (serverTranscript?.messages ?? []).map((m) => ({
  role: m.role,
  seq: m.seq,
  text: m.blocks.map((b) => b.text ?? '').join('')
}))

const connections = lines.flatMap((l, i) => {
  if (l.frame.type !== 'auth_required') return []
  const prior = lines.slice(0, i).filter((p) => p.frame.seq > 0)
  const burst = lines.filter((b) => b.at >= l.at && b.at < l.at + 500 && b.frame.seq > 0)
  return [
    {
      atMs: l.at - lines[0].at,
      lastSeqBefore: prior.at(-1)?.frame.seq ?? 0,
      seqsInFirst500ms: burst.length ? [burst[0].frame.seq, burst.at(-1)?.frame.seq] : []
    }
  ]
})

// The restart counts only when the probe reconnected to the new server and
// then received the turn's tail, or had every seq before the stop.
const sessionLines = lines.filter((l) => l.frame.sid === sessionId && l.frame.seq > 0)
const seqBeforeStop = Math.max(
  0,
  ...sessionLines.filter((l) => l.at <= marks.stopped).map((l) => l.frame.seq)
)
const restart = {
  reconnected: lines.some((l) => l.at > marks.stopped && l.frame.type === 'auth_required'),
  seqBeforeStop,
  sessionEventsAfterStop: sessionLines.filter((l) => l.at > marks.stopped).length
}

const result = {
  session: sessionId,
  printed: events.length,
  snapshots: lines.filter((l) => 'watermark' in l.frame).length,
  diskSeqs: [diskSeqs[0], diskSeqs.at(-1)],
  serverLastSeq: serverTranscript?.metadata.lastSeq,
  inOrder: events.every((e, i) => i === 0 || e.seq > events[i - 1].seq),
  repeated: [...printed].filter(([, n]) => n > 1).map(([seq]) => seq),
  missing: diskSeqs.filter((seq) => !printed.has(seq)),
  extra: [...printed.keys()].filter((seq) => !disk.has(seq)),
  payloadMismatches: events
    .filter((e) => {
      const d = disk.get(e.seq)
      return d && identity(d) !== identity(e as unknown as LogRecord)
    })
    .map((e) => e.seq),
  transcript: {
    probeMessages: probeMessages.length,
    serverMessages: serverMessages.length,
    equal: canon(probeMessages) === canon(serverMessages)
  },
  cutMs: marks.restored - marks.cut,
  serverDownMs: marks.restarted - marks.stopped,
  restart,
  connections
}
console.log(JSON.stringify(result, null, 2))
const clean =
  result.inOrder &&
  result.repeated.length === 0 &&
  result.missing.length === 0 &&
  result.extra.length === 0 &&
  result.payloadMismatches.length === 0 &&
  result.transcript.equal &&
  result.serverLastSeq === diskSeqs.at(-1) &&
  restart.reconnected &&
  (restart.sessionEventsAfterStop > 0 || seqBeforeStop === result.serverLastSeq)
log(clean ? 'PASS' : 'FAIL')
process.exit(clean ? 0 : 1)
