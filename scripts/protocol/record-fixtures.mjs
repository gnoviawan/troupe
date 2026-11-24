#!/usr/bin/env node
// Records one real frame per /ws and /terminal/ws message type from a seeded
// troupe-server into crates/troupe-protocol/fixtures/.
//   node scripts/protocol/record-fixtures.mjs --server <troupe-server> [--port 8191] [--dir /tmp/rune-r1a]
// The server runs on loopback with its state, workspace and HOME under --dir,
// so it never reads the operator's accounts. Paths and ids that name this run
// are replaced with fixed placeholders before a frame is written.
import { execFileSync, spawn } from 'node:child_process'
import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { setTimeout as sleep } from 'node:timers/promises'
import { fileURLToPath } from 'node:url'
import { parseArgs } from 'node:util'

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..', '..')
const FIXTURES = join(ROOT, 'crates/troupe-protocol/fixtures')
const PROTOCOL_VERSION = 7
const TOKEN = 'fixture-token'
const IDENTITY = { deviceId: 'fixture-device', clientInstanceId: 'fixture-tab' }

const { values: args } = parseArgs({
  options: {
    server: { type: 'string' },
    port: { type: 'string', default: '8191' },
    dir: { type: 'string', default: '/tmp/rune-r1a' }
  }
})
if (!args.server) throw new Error('--server <path to troupe-server> is required')
const port = Number(args.port)
if (port < 8190 || port > 8199) throw new Error('--port must be in 8190-8199')

const RUN = args.dir
const MARKER = join(RUN, '.troupe-protocol-recorder')
const STATE = join(RUN, 'state')
const WORK = join(RUN, 'work')
const HOME = join(RUN, 'home')
const PROJ_A = join(WORK, 'proj-a')
const PROJ_W = join(WORK, 'proj-w')
// The fake agent advertises modes and config options for a cwd with this name.
const PROJ_COMPOSER = join(WORK, 'composer-row-e2e')
const BASE = `127.0.0.1:${port}`

/** Run-specific strings and their stable stand-ins, longest first. */
const SCRUB = [
  [STATE, '/fixture/state'],
  [WORK, '/fixture/work'],
  [HOME, '/fixture/home'],
  [RUN, '/fixture'],
  [ROOT, '/fixture/repo']
]

function scrub(value) {
  let text = JSON.stringify(value)
  for (const [from, to] of SCRUB) text = text.split(from).join(to)
  return JSON.parse(text)
}

const saved = new Map()

/** Keep the first frame per fixture path; `prefer` replaces an earlier one. */
function keep(path, frame, { prefer = false } = {}) {
  if (saved.has(path) && !prefer) return
  saved.set(path, scrub(frame))
}

/** Arrays in reply payloads keep their first three items, so a catalog
 * listing stays a readable fixture without changing its shape. */
function trim(value) {
  if (Array.isArray(value)) return value.slice(0, 3).map(trim)
  if (value && typeof value === 'object') {
    return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, trim(item)]))
  }
  return value
}

/** `ws/reply/x` lands in `ws-reply.json` under `x`, one file per kind. */
function writeFixtures() {
  const files = new Map()
  for (const [path, frame] of [...saved].sort(([a], [b]) => a.localeCompare(b))) {
    const [socket, kind, name] = path.split('/')
    const file = `${socket}-${kind}.json`
    if (!files.has(file)) files.set(file, {})
    files.get(file)[name] = kind === 'reply' ? trim(frame) : frame
  }
  mkdirSync(FIXTURES, { recursive: true })
  for (const name of readdirSync(FIXTURES)) {
    if (/^(ws|terminal)-[a-z]+\.json$/.test(name)) rmSync(join(FIXTURES, name))
  }
  for (const [file, frames] of files) {
    writeFileSync(join(FIXTURES, file), `${JSON.stringify(frames, null, 2)}\n`)
  }
  const biome = join(ROOT, 'node_modules', '.bin', 'biome')
  if (existsSync(biome)) execFileSync(biome, ['format', '--write', FIXTURES], { stdio: 'ignore' })
  console.log(`wrote ${saved.size} live fixtures to ${files.size} files under ${FIXTURES}`)
  const handwritten = join(FIXTURES, 'handwritten.json')
  if (!existsSync(handwritten)) return
  for (const entry of JSON.parse(readFileSync(handwritten, 'utf8'))) {
    if (files.get(entry.file)?.[entry.name]) {
      console.warn(
        `${entry.file} ${entry.name} is now recorded live; drop it from handwritten.json`
      )
    }
  }
}

function prepareWorkspace() {
  if (existsSync(RUN) && !existsSync(MARKER))
    throw new Error(`${RUN} exists and was not created by this recorder; refusing to remove it`)
  rmSync(RUN, { recursive: true, force: true })
  for (const dir of [STATE, PROJ_A, PROJ_W, PROJ_COMPOSER, HOME])
    mkdirSync(dir, { recursive: true })
  writeFileSync(MARKER, '')
  const git = (...argv) =>
    execFileSync('git', ['-c', 'user.email=fixture@troupe', '-c', 'user.name=fixture', ...argv], {
      cwd: PROJ_W,
      stdio: 'ignore'
    })
  git('init', '-q', '-b', 'main')
  writeFileSync(join(PROJ_W, 'notes.txt'), 'one\n')
  git('add', 'notes.txt')
  git('commit', '-q', '-m', 'init')
  writeFileSync(join(PROJ_W, 'notes.txt'), 'one\ntwo\n')
}

async function startServer() {
  const child = spawn(
    args.server,
    [
      '--host',
      '127.0.0.1',
      '--port',
      String(port),
      '--state-dir',
      STATE,
      '--sessions-dir',
      join(STATE, 'sessions'),
      '--projects-file',
      join(STATE, 'projects.json'),
      '--store-file',
      join(STATE, 'store.json'),
      '--web-auth-token',
      TOKEN
    ],
    {
      stdio: ['ignore', 'pipe', 'pipe'],
      env: {
        ...process.env,
        HOME,
        CLAUDE_CONFIG_DIR: join(HOME, '.claude'),
        CODEX_HOME: join(HOME, '.codex'),
        GEMINI_HOME: join(HOME, '.gemini'),
        RUST_LOG: 'warn'
      }
    }
  )
  const log = []
  child.stdout.on('data', (chunk) => log.push(String(chunk)))
  child.stderr.on('data', (chunk) => log.push(String(chunk)))
  const deadline = Date.now() + 30_000
  while (Date.now() < deadline) {
    if (child.exitCode !== null) throw new Error(`troupe-server exited:\n${log.join('')}`)
    try {
      if ((await fetch(`http://${BASE}/health`)).ok) return child
    } catch {}
    await sleep(200)
  }
  child.kill('SIGKILL')
  throw new Error('troupe-server never became healthy')
}

async function stopServer(child) {
  if (child.exitCode !== null) return
  const exited = new Promise((resolve) => child.once('exit', resolve))
  child.kill('SIGTERM')
  if (!(await Promise.race([exited.then(() => true), sleep(5_000).then(() => false)]))) {
    child.kill('SIGKILL')
    await exited
  }
}

async function addProject(id, name, path) {
  const response = await fetch(`http://${BASE}/projects`, {
    method: 'POST',
    headers: { authorization: `Bearer ${TOKEN}`, 'content-type': 'application/json' },
    body: JSON.stringify({ id, name, path, color: 'blue' })
  })
  if (!response.ok) throw new Error(`POST /projects ${id}: ${response.status}`)
}

/** One socket: sends requests, records every frame both ways, and lets a
 * step wait for a frame that matches a predicate. */
class Socket {
  constructor(path, classify) {
    this.classify = classify
    this.frames = []
    this.waiters = []
    this.pending = new Map()
    this.next = 0
    this.ws = new WebSocket(`ws://${BASE}${path}`)
    this.ws.addEventListener('message', (event) => this.receive(JSON.parse(String(event.data))))
    this.opened = new Promise((resolve, reject) => {
      this.ws.addEventListener('open', resolve, { once: true })
      this.ws.addEventListener('error', reject, { once: true })
    })
  }

  receive(frame) {
    this.frames.push(frame)
    const pending = frame.id === undefined ? undefined : this.pending.get(frame.id)
    if (pending) {
      this.pending.delete(frame.id)
      this.classify(frame, pending.type)
      pending.resolve(frame)
    } else {
      this.classify(frame, undefined)
    }
    this.notify(frame)
  }

  notify(frame) {
    if (frame.type === 'events' && Array.isArray(frame.events)) {
      for (const event of frame.events) {
        this.frames.push(event)
        this.notify(event)
      }
    }
    this.waiters = this.waiters.filter((waiter) => !waiter.accept(frame) || waiter.resolve(frame))
  }

  waitFor(match, timeoutMs = 15_000, label = 'frame') {
    const seen = this.frames.find(match)
    if (seen) return Promise.resolve(seen)
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error(`timed out waiting for ${label}`)), timeoutMs)
      this.waiters.push({
        accept: match,
        resolve: (frame) => {
          clearTimeout(timer)
          resolve(frame)
          return true
        }
      })
    })
  }

  async request(type, payload, { commandId, key = 'command_id', save = true } = {}) {
    await this.opened
    const id = `req-${++this.next}`
    const frame = { id, type, payload }
    if (commandId) frame[key] = commandId
    if (save) this.classify(frame, type, 'request')
    const reply = new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id)
        reject(new Error(`${type}: no reply`))
      }, 30_000)
      this.pending.set(id, {
        type,
        resolve: (value) => {
          clearTimeout(timer)
          resolve(value)
        }
      })
    })
    this.ws.send(JSON.stringify(frame))
    return reply
  }

  close() {
    this.ws.close()
  }
}

function classifyWs(frame, type, direction) {
  if (direction === 'request') return keep(`ws/request/${type}`, frame)
  if (frame.id !== undefined && 'ok' in frame) {
    return keep(`ws/reply/${type}`, frame, {
      prefer: frame.ok && !saved.get(`ws/reply/${type}`)?.ok
    })
  }
  switch (frame.type) {
    case 'events':
      keep('ws/frame/events', frame)
      for (const event of frame.events) classifyWs(event)
      return
    case 'watch': {
      const event = typeof frame.delta?.event === 'string' ? `.${frame.delta.event}` : ''
      return keep(`ws/watch/${frame.watch}.${frame.reset ? 'state' : `delta${event}`}`, frame)
    }
    case 'gap':
    case 'heartbeat':
    case 'canvas_call':
    case 'canvas_lease_lost':
      if (frame.seq === undefined) return keep(`ws/frame/${frame.type}`, frame)
  }
  if (typeof frame.seq === 'number') return keep(`ws/event/${frame.type}`, frame)
  console.warn('unclassified /ws frame', JSON.stringify(frame).slice(0, 200))
}

function classifyTerminal(frame, type, direction) {
  if (direction === 'request') return keep(`terminal/request/${type}`, frame)
  if (frame.id !== undefined && 'success' in frame) {
    const path = `terminal/reply/${type}`
    return keep(path, frame, { prefer: frame.success && !saved.get(path)?.success })
  }
  if (frame.type === 'event') return keep(`terminal/frame/event.${frame.payload.type}`, frame)
  if (['data', 'replay', 'gap'].includes(frame.type)) {
    return keep(`terminal/frame/${frame.type}`, frame)
  }
  console.warn('unclassified /terminal/ws frame', JSON.stringify(frame).slice(0, 200))
}

const fakeAgent = {
  configId: 'fake-longrun',
  name: 'Fake Longrun',
  command: 'bun',
  args: [join(ROOT, 'tests/e2e/fake-longrun-agent.ts')],
  env: { RATE: '5' },
  allowTerminal: false
}

/** Every `/ws` request type, in an order whose earlier steps seed the later ones. */
async function recordWs() {
  const ws = new Socket('/ws', classifyWs)
  const ok = (reply, what) => {
    if (!reply.ok) throw new Error(`${what}: ${JSON.stringify(reply.err)}`)
    return reply.payload
  }
  const event = (type, match = () => true) =>
    ws.waitFor((f) => f.type === type && typeof f.seq === 'number' && match(f), 20_000, type)

  ok(
    await ws.request('authenticate', {
      token: TOKEN,
      protocolVersion: PROTOCOL_VERSION,
      ...IDENTITY
    }),
    'authenticate'
  )
  await ws.request('ping', {})
  const storeKey = `device/${IDENTITY.deviceId}/fixture/key`
  for (const [watch, scope] of [
    ['projects', ''],
    ['chat_history', ''],
    ['agents', ''],
    ['chat_activity', ''],
    ['fs', 'fixture-proj-a'],
    ['store', storeKey],
    ['terminals', 'fixture-proj-a']
  ]) {
    ok(await ws.request('watch', { watch, scope }), `watch ${watch}`)
  }

  await recordTerminal()
  await ws.request('store_write', { key: storeKey, value: { theme: 'dark' } })
  await ws.request('store_read', { key: storeKey })
  await ws.request('store_write', {
    key: 'shared/cas/fixture-cas',
    value: { n: 1 },
    expected: null,
    commandId: 'fixture-cas-write'
  })
  await ws.request('store_delete', { key: storeKey })
  writeFileSync(join(PROJ_A, 'touched.txt'), 'fs watch\n')

  await ws.request('add_project', {
    id: 'fixture-proj-b',
    name: 'proj-b',
    path: PROJ_W,
    color: 'green'
  })
  await ws.request('update_project', { projectId: 'fixture-proj-b', name: 'proj-b renamed' })
  await ws.request('set_default_project', { projectId: 'fixture-proj-a' })

  const spawned = ok(
    await ws.request(
      'spawn_agent',
      { config: fakeAgent, projectId: 'fixture-proj-a', sessionId: null },
      { commandId: 'fixture-spawn' }
    ),
    'spawn_agent'
  )
  const agentId = spawned.agentId ?? spawned.id ?? spawned
  await ws.request('list_agents', {})
  const created = ok(
    await ws.request(
      'create_session',
      { agentId, cwd: PROJ_A, mcpServers: [], ephemeral: false, projectId: 'fixture-proj-a' },
      { commandId: 'fixture-create' }
    ),
    'create_session'
  )
  const sessionId = created.sessionId
  const subscribed = ok(await ws.request('subscribe', { sessionId }), 'subscribe')
  await ws.request('get_session_cursor', { sessionId })

  await ws.request(
    'send_prompt',
    {
      agentId,
      sessionId,
      text: '[DOCK] [USAGE] [ASK:permission] [DURATION:3] record fixtures',
      turnId: 'fixture-turn-1'
    },
    { commandId: 'fixture-turn-1' }
  )
  const permission = await event('permission_request')
  await ws.request(
    'respond_permission',
    {
      agentId,
      requestId: permission.payload.requestId,
      optionId: permission.payload.options?.[0]?.optionId ?? 'allow'
    },
    { commandId: 'fixture-permission' }
  )
  const done = await event('prompt_complete')

  await ws.request(
    'send_prompt',
    { agentId, sessionId, text: '[ASK:question] [DURATION:2] ask', turnId: 'fixture-turn-2' },
    { commandId: 'fixture-turn-2' }
  )
  const question = await event('question_request').catch(() => undefined)
  const questionPayload = question?.payload ?? {}
  await ws.request(
    'answer_question',
    { agentId, questionId: questionPayload.questionId ?? 'missing', values: ['yes'] },
    { commandId: 'fixture-answer' }
  )
  await event('prompt_complete', (f) => f.seq > done.seq)

  await ws.request(
    'send_prompt',
    { agentId, sessionId, text: '[ASK:elicitation] [DURATION:2] elicit', turnId: 'fixture-turn-3' },
    { commandId: 'fixture-turn-3' }
  )
  const elicitation = await event('elicitation_request').catch(() => undefined)
  const elicitationPayload = elicitation?.payload ?? {}
  await ws.request(
    'respond_elicitation',
    {
      agentId,
      requestId: elicitationPayload.requestId ?? 'missing',
      action: 'cancel'
    },
    { commandId: 'fixture-elicitation' }
  )
  await ws.request('cancel_prompt', { agentId, sessionId })

  const composer = ok(
    await ws.request(
      'create_session',
      { agentId, cwd: PROJ_COMPOSER, mcpServers: [], ephemeral: false },
      { commandId: 'fixture-create-composer' }
    ),
    'create_session composer'
  )
  await ws.request('subscribe', { sessionId: composer.sessionId }, { save: false })
  await ws.request('set_mode', { agentId, sessionId: composer.sessionId, modeId: 'plan' })
  await ws.request('set_model', { agentId, sessionId: composer.sessionId, modelId: 'opus-5-5' })
  await ws.request('set_config_option', {
    agentId,
    sessionId: composer.sessionId,
    configId: 'thought_level',
    valueId: 'high'
  })
  await ws.request(
    'send_prompt',
    {
      agentId,
      sessionId: composer.sessionId,
      text: '[SUBAGENTS:3] [BG_TAIL:1] subagents',
      turnId: 'fixture-turn-4'
    },
    { commandId: 'fixture-turn-4', save: false }
  )
  await event('tool_call_update').catch(() => {})
  await ws.request('stop_subagent', {
    agentId,
    rootSessionId: composer.sessionId,
    subagentId: 'missing-subagent'
  })
  await event('prompt_complete', (f) => f.sid === composer.sessionId).catch(() => {})
  const cursor = ok(await ws.request('get_session_cursor', { sessionId }), 'cursor')
  await ws.request('mark_chat_read', {
    sessionId,
    seq: cursor.watermark,
    logEpoch: subscribed.logEpoch
  })
  await ws.request('list_persisted_sessions', {})
  await ws.request('get_session_payload', { sessionId })
  await ws.request('get_session_payload_tail', { sessionId, limit: 2 })
  await ws.request('recover_session_snapshot', { sessionId })
  await ws.request('open_persisted_session', { sessionId, lastSeq: 0 })
  await ws.request('list_sessions', { agentId, cwd: PROJ_A })
  await ws.request('rename_session', { sessionId, title: 'Fixture chat' }, { save: false })
  const quick = ok(
    await ws.request(
      'create_session',
      { agentId, kind: 'quick', mcpServers: [], ephemeral: false },
      { commandId: 'fixture-create-quick' }
    ),
    'create_session quick'
  )
  await ws.request('subscribe', { sessionId: quick.sessionId }, { save: false })
  await ws.request('rename_session', { sessionId: quick.sessionId, title: 'Quick fixture chat' })
  await ws.request('browser_stop', { sessionId })

  const second = ok(
    await ws.request(
      'create_session',
      { agentId, cwd: PROJ_A, mcpServers: [], ephemeral: false, projectId: 'fixture-proj-a' },
      { commandId: 'fixture-create-2' }
    ),
    'create_session 2'
  )
  await ws.request(
    'record_agent_switch',
    {
      sessionId,
      fromConfigId: 'fake-longrun',
      toConfigId: 'fake-longrun',
      newSessionId: second.sessionId,
      summaryText: 'Switched for the fixture run'
    },
    { commandId: 'fixture-switch' }
  )
  const ephemeral = ok(
    await ws.request(
      'create_session',
      { agentId, cwd: PROJ_A, mcpServers: [], ephemeral: true, promotable: true },
      { commandId: 'fixture-create-3' }
    ),
    'create_session 3'
  )
  await ws.request(
    'promote_session',
    { agentId, sessionId: ephemeral.sessionId },
    { commandId: 'fixture-promote' }
  )
  const disposable = ok(
    await ws.request(
      'create_session',
      { agentId, cwd: PROJ_A, mcpServers: [], ephemeral: true },
      { commandId: 'fixture-create-4' }
    ),
    'create_session 4'
  )
  await ws.request(
    'dispose_ephemeral_session',
    { agentId, sessionId: disposable.sessionId },
    { commandId: 'fixture-dispose' }
  )
  await ws.request(
    'load_session',
    { agentId, sessionId, cwd: PROJ_A },
    { commandId: 'fixture-load' }
  )
  await ws.request(
    'resume_session',
    { agentId, sessionId, cwd: PROJ_A },
    { commandId: 'fixture-resume' }
  )
  await ws.request(
    'register_discovered_session',
    { agentId, sessionId: 'discovered-session', cwd: PROJ_A, title: 'Discovered' },
    { commandId: 'fixture-register' }
  )
  await ws.request(
    'import_agent_session',
    { agentId, sessionId: 'imported-session', cwd: PROJ_A },
    { commandId: 'fixture-import' }
  )
  await ws.request(
    'authenticate_agent',
    { agentId, methodId: 'fixture-method' },
    { commandId: 'fixture-auth-agent' }
  )
  await ws.request('logout_agent', { agentId })
  await ws.request('delete_agent_session', { agentId, sessionId: 'missing-session' })

  await ws.request('set_mcp_catalog_sync', { enabled: false })
  await ws.request('get_mcp_catalog_status', {})
  await ws.request('search_mcp_catalog', { query: 'fixture', limit: 1 })
  await ws.request('describe_mcp_install', { serverId: 'missing-server' })
  await ws.request('plan_mcp_install', { serverId: 'missing-server', target: 'global' })
  await ws.request(
    'commit_mcp_install',
    { planId: 'missing-plan' },
    { commandId: 'fixture-commit-mcp' }
  )
  await ws.request('cancel_mcp_install', { planId: 'missing-plan' })
  await ws.request('get_mcp_catalog_icon', { serverId: 'missing-server' })
  await ws.request(
    'answer_mcp_install',
    { requestId: 'missing-request', approved: false },
    { commandId: 'fixture-answer-mcp' }
  )
  await ws.request('set_catalog_opt_in', { enabled: false })
  await ws.request('list_acp_catalog', {})
  await ws.request('fetch_acp_registry_snapshot', { reason: 'background' })
  await ws.request(
    'install_acp_agent',
    { agentId: 'missing-agent' },
    { commandId: 'fixture-install' }
  )
  await ws.request(
    'acp_deliver_auth_redirect',
    { agentId, url: 'https://example.invalid/callback' },
    { commandId: 'fixture-redirect' }
  )
  await ws.request('subagent_settings_get', {})
  await ws.request('subagent_settings_set', { maxRunning: 2 })

  await ws.request('list_claude_accounts', {})
  await ws.request('list_claude_usage', {})
  const claude = await ws.request(
    'add_claude_account',
    { name: 'Fixture' },
    { commandId: 'fixture-claude-add' }
  )
  const claudeId = claude.payload?.id ?? claude.payload?.accountId ?? 'missing-account'
  await ws.request('rename_claude_account', { accountId: claudeId, name: 'Fixture 2' })
  await ws.request('set_default_claude_account', { accountId: 'missing-account' })
  await ws.request('set_project_claude_account', { projectId: 'fixture-proj-a', accountId: null })
  await ws.request('set_chat_claude_account', {
    projectId: 'fixture-proj-a',
    sessionId,
    accountId: null
  })
  await ws.request('set_claude_auto_switch', { enabled: false })
  await ws.request('set_claude_resume', { sessionId, at: null })
  await ws.request('start_claude_link_login', { accountId: 'missing-account' })
  await ws.request(
    'finish_claude_link_login',
    { accountId: 'missing-account', code: 'fixture-code' },
    { commandId: 'fixture-claude-finish' }
  )
  await ws.request('cancel_claude_login', { accountId: 'missing-account' })
  await ws.request('remove_claude_account', { accountId: claudeId })

  await ws.request('list_codex_accounts', {})
  await ws.request('list_codex_usage', {})
  const codex = await ws.request(
    'add_codex_account',
    { name: 'Fixture' },
    { commandId: 'fixture-codex-add' }
  )
  const codexId = codex.payload?.id ?? codex.payload?.accountId ?? 'missing-account'
  await ws.request('rename_codex_account', { accountId: codexId, name: 'Fixture 2' })
  await ws.request('set_default_codex_account', { accountId: 'missing-account' })
  await ws.request('set_project_codex_account', { projectId: 'fixture-proj-a', accountId: null })
  await ws.request('set_chat_codex_account', {
    projectId: 'fixture-proj-a',
    sessionId,
    accountId: null
  })
  await ws.request('set_codex_auto_switch', { enabled: false })
  await ws.request('set_codex_resume', { sessionId, at: null })
  await ws.request(
    'start_codex_account_login',
    { accountId: 'missing-account', config: fakeAgent },
    { commandId: 'fixture-codex-login' }
  )
  await ws.request('remove_codex_account', { accountId: codexId })

  await ws.request('list_antigravity_accounts', {})
  await ws.request('list_antigravity_usage', {})
  const antigravity = await ws.request(
    'add_antigravity_account',
    { name: 'Fixture' },
    { commandId: 'fixture-antigravity-add' }
  )
  const antigravityId =
    antigravity.payload?.id ?? antigravity.payload?.accountId ?? 'missing-account'
  await ws.request('rename_antigravity_account', { accountId: antigravityId, name: 'Fixture 2' })
  await ws.request('set_default_antigravity_account', { accountId: 'missing-account' })
  await ws.request('set_project_antigravity_account', {
    projectId: 'fixture-proj-a',
    accountId: null
  })
  await ws.request('set_chat_antigravity_account', {
    projectId: 'fixture-proj-a',
    sessionId,
    accountId: null
  })
  await ws.request('set_antigravity_auto_switch', { enabled: false })
  await ws.request('set_antigravity_resume', { sessionId, at: null })
  await ws.request(
    'start_antigravity_account_login',
    { accountId: 'missing-account', config: fakeAgent },
    { commandId: 'fixture-antigravity-login' }
  )
  await ws.request('remove_antigravity_account', { accountId: antigravityId })

  await ws.request('canvas_mount', { projectId: 'fixture-proj-a', mountSeq: 1 })
  await ws.request('canvas_reply', { callId: 1, mountSeq: 1, ok: true, result: '{}' })
  await ws.request('canvas_unmount', { projectId: 'fixture-proj-a', mountSeq: 1 })

  await ws.request('delete_session', { sessionId: second.sessionId })
  await ws.request('close_session', { agentId, sessionId }, { commandId: 'fixture-close' })
  await ws.request('kill_agent', { agentId })
  await ws.request('remove_project', { projectId: 'fixture-proj-b' })
  await ws.request('unwatch', { watch: 'store', scope: storeKey })
  await ws.waitFor((f) => f.type === 'heartbeat', 15_000, 'heartbeat').catch(() => {})
  ws.close()
}

/** Every `/terminal/ws` request type against one shell. */
async function recordTerminal() {
  const term = new Socket('/terminal/ws', classifyTerminal)
  const ok = (reply, what) => {
    if (!reply.success) throw new Error(`${what}: ${reply.code} ${reply.error}`)
    return reply.data
  }
  const command = { key: 'commandId' }
  ok(
    await term.request('authenticate', {
      token: TOKEN,
      protocolVersion: PROTOCOL_VERSION,
      ...IDENTITY
    }),
    'authenticate'
  )
  const spawned = ok(
    await term.request(
      'spawn',
      { shell: '/bin/bash', cwd: PROJ_W, cols: 80, rows: 24, projectId: 'fixture-proj-a' },
      { ...command, commandId: 'fixture-term-spawn' }
    ),
    'spawn'
  )
  const terminalId = spawned.id
  const attached = ok(
    await term.request('attach', { terminalId, claim: spawned.claim, lastSeq: 0 }),
    'attach'
  )
  await term.request(
    'write',
    { terminalId, data: 'echo ok\r' },
    { ...command, commandId: 'fixture-term-write' }
  )
  await term.waitFor((f) => f.type === 'data', 10_000, 'terminal data')
  await term.request('resize', { terminalId, cols: 100, rows: 30 })
  await term.request(
    'write',
    { terminalId, data: 'cd /tmp && false\r' },
    { ...command, commandId: 'fixture-term-write-2', save: false }
  )
  await sleep(1_500)
  await term.request('get_cwd', { terminalId })
  await term.request('get_git_branch', { terminalId })
  await term.request('get_git_status', { terminalId })
  await term.request('get_exit_code', { terminalId })
  await term.request('command_editor_restore_prompt', { terminalId })
  await term.request('command_editor_submit', { terminalId, draft: 'echo editor' })
  await term.request('command_editor_request_completion', {
    terminalId,
    requestId: 'fixture-completion',
    draft: 'ech'
  })
  await term.request('command_editor_cancel_completion', {
    terminalId,
    requestId: 'fixture-completion'
  })
  await term.request('detach', { terminalId })
  await term.request('attach', {
    terminalId,
    claim: spawned.claim,
    lastSeq: attached.latestSeq,
    logEpoch: attached.logEpoch
  })
  const rotated = ok(
    await term.request(
      'rotate_claim',
      { terminalId, claim: spawned.claim },
      { ...command, commandId: 'fixture-term-rotate' }
    ),
    'rotate_claim'
  )
  await term.request('list_preserved', { projectId: 'fixture-proj-a' })
  const doomed = ok(
    await term.request(
      'spawn',
      { cwd: PROJ_A, cols: 80, rows: 24, projectId: 'fixture-proj-a' },
      { ...command, commandId: 'fixture-term-spawn-2', save: false }
    ),
    'spawn 2'
  )
  await term.request(
    'attach',
    { terminalId: doomed.id, claim: doomed.claim, lastSeq: 0 },
    { save: false }
  )
  await term.request(
    'kill',
    { terminalId: doomed.id },
    { ...command, commandId: 'fixture-term-kill' }
  )
  await term.waitFor((f) => f.payload?.type === 'exit', 10_000, 'exit event').catch(() => {})
  await term.request(
    'revoke_claim',
    { terminalId, claim: rotated.claim },
    { ...command, commandId: 'fixture-term-revoke' }
  )
  await sleep(1_000)
  term.close()
}

async function main() {
  prepareWorkspace()
  const server = await startServer()
  try {
    await addProject('fixture-proj-a', 'proj-a', PROJ_A)
    await addProject('fixture-proj-c', 'composer', PROJ_COMPOSER)
    await recordWs()
  } finally {
    await stopServer(server)
  }
  writeFixtures()
}

await main()
