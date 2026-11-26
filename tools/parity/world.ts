/**
 * The one seeded server every matrix surface runs against.
 *
 * - alpha: a git repo with three commits, a second branch, a staged edit, an
 *   unstaged edit and an untracked file, plus source and markdown files for
 *   the explorer and the editors.
 * - beta, gamma and delta: plain folders. Surfaces that rename or remove a
 *   project re-add it in `prepare`.
 * - A chat in alpha holding the scripted agent's three turns. The agent is
 *   `agent/matrix-agent.ts`, which adds plan, permission and hold prompts.
 *
 * Every client run uses a fresh device id, so device-scoped UI state (open
 * tabs, side-panel tab, layout) never leaks from one run into the next.
 */

import { mkdirSync, rmSync, writeFileSync } from 'node:fs'
import path from 'node:path'
import { scopedKey } from '../../src/shared/types/persistence.types'
import { SCRIPT_PROMPTS } from './agent/script'
import { addProject, serverFetch, startTestServer, type TestServer } from './server'
import { commandId, connectWs, type WsClient } from './ws'

export const TOKEN = 'troupe-parity-matrix'
export const AGENT_NAME = 'Chat Scenario Agent'

export const PROJECTS = {
  alpha: { id: 'matrix-alpha', name: 'alpha', color: 'blue' },
  beta: { id: 'matrix-beta', name: 'beta', color: 'green' },
  gamma: { id: 'matrix-gamma', name: 'gamma', color: 'purple' },
  delta: { id: 'matrix-delta', name: 'delta', color: 'orange' }
} as const
export type ProjectKey = keyof typeof PROJECTS

export interface AgentConfig {
  configId: string
  name: string
  command: string
  args: string[]
  env: Record<string, string>
  allowTerminal: boolean
}

export const AGENT: AgentConfig = {
  configId: 'custom-chat-scenario',
  name: AGENT_NAME,
  command: 'bun',
  args: [path.join(import.meta.dir, 'agent/matrix-agent.ts')],
  env: {},
  allowTerminal: false
}

export interface World {
  server: TestServer
  ws: WsClient
  root: string
  dirs: Record<ProjectKey, string>
  chat: { agentId: string; sessionId: string }
  /** alpha's seeded HEAD, so a surface that commits can be rewound. */
  alphaHead: string
  /**
   * The wall clock every client reads (epoch ms), fixed a while after the
   * seed, so relative times ("3m", "2h") read the same in every run.
   */
  clock: number
}

const CLOCK_AFTER_SEED_MS = 6 * 60 * 60 * 1000

const GIT_ID = ['-c', 'user.name=Matrix', '-c', 'user.email=matrix@example.invalid']
const TURN_TIMEOUT_MS = 120_000

function git(cwd: string, ...args: string[]): string {
  const proc = Bun.spawnSync(['git', ...GIT_ID, ...args], { cwd, stdout: 'pipe', stderr: 'pipe' })
  if (proc.exitCode !== 0) {
    throw new Error(`git ${args.join(' ')} failed: ${proc.stderr.toString()}`)
  }
  return proc.stdout.toString().trim()
}

function write(dir: string, file: string, text: string): void {
  mkdirSync(path.dirname(path.join(dir, file)), { recursive: true })
  writeFileSync(path.join(dir, file), text)
}

const APP_TS = `export function greet(name: string): string {
  return \`Hello, \${name}!\`
}

export const VERSION = '1.0.0'
`

const GUIDE_MD = `# Alpha guide

Alpha is the matrix's seeded project.

## Setup

- Install the tools
- Run the tests
`

/** The one skill the slash menu lists: the server's HOME is otherwise empty. */
const MATRIX_SKILL = `---
name: matrix-review
description: Review the router changes in the matrix project.
---

Review the router changes.
`

function seedAlphaRepo(dir: string): string {
  write(dir, 'README.md', '# alpha\n\nThe matrix project.\n')
  write(dir, 'src/app.ts', APP_TS)
  write(dir, 'docs/guide.md', GUIDE_MD)
  git(dir, 'init', '-q', '-b', 'main')
  git(dir, 'add', '.')
  git(dir, 'commit', '-q', '-m', 'initial commit')
  write(dir, 'src/utils.ts', 'export const add = (a: number, b: number) => a + b\n')
  git(dir, 'add', '.')
  git(dir, 'commit', '-q', '-m', 'add utils')
  git(dir, 'branch', 'feature/login')
  write(dir, 'README.md', '# alpha\n\nThe matrix project.\n\nSee docs/guide.md.\n')
  git(dir, 'commit', '-q', '-am', 'link the guide')
  const head = git(dir, 'rev-parse', 'HEAD')
  resetAlphaChanges(dir, head)
  return head
}

/** Put alpha's working tree back to the seeded changes: one staged, one unstaged, one untracked. */
function resetAlphaChanges(dir: string, head: string): void {
  git(dir, 'checkout', '-q', '-f', 'main')
  git(dir, 'reset', '-q', '--hard', head)
  git(dir, 'clean', '-fdq')
  write(dir, 'src/app.ts', APP_TS.replace("'1.0.0'", "'1.1.0'"))
  git(dir, 'add', 'src/app.ts')
  write(
    dir,
    'README.md',
    '# alpha\n\nThe matrix project.\n\nSee docs/guide.md.\n\nAn unstaged line.\n'
  )
  write(dir, 'notes.txt', 'untracked notes\n')
}

/** With this set to `1`, a device also gets `SECOND_AGENT`, an agent a chat can switch to. */
export const SECOND_AGENT_ENV = 'PARITY_SECOND_AGENT'

/** The scripted agent again under another config, for the in-chat agent switch. */
export const SECOND_AGENT: AgentConfig = {
  ...AGENT,
  configId: 'custom-second-agent',
  name: 'Second Agent'
}

/** Save `AGENT` (and `SECOND_AGENT` when asked) as the device's custom agents, `AGENT` its launcher pick. */
async function registerAgent(ws: WsClient, deviceId: string): Promise<void> {
  const agentsKey = scopedKey('acp/agents', deviceId)
  const current = await ws.request<{ revision?: number }>('store_read', { key: agentsKey })
  const agents = process.env[SECOND_AGENT_ENV] === '1' ? [AGENT, SECOND_AGENT] : [AGENT]
  await ws.request('store_write', {
    key: agentsKey,
    value: { _version: 1, data: agents.map((agent) => ({ id: agent.configId, ...agent })) },
    baseRevision: current.revision ?? 0
  })
  await ws.request('store_write', {
    key: scopedKey('agents/last-selected', deviceId),
    value: { _version: 1, data: { mode: 'acp', agentId: AGENT.configId } }
  })
}

async function sendPrompt(
  ws: WsClient,
  target: { agentId: string; sessionId: string },
  text: string
): Promise<void> {
  const turnId = commandId('turn')
  await ws.request(
    'send_prompt',
    { ...target, text, turnId },
    { commandId: turnId, timeoutMs: TURN_TIMEOUT_MS }
  )
}

export async function createWorld(opts: {
  port: number
  root: string
  binary?: string
}): Promise<World> {
  const server = await startTestServer({
    port: opts.port,
    token: TOKEN,
    stateDir: path.join(opts.root, 'server'),
    binary: opts.binary
  })
  rmSync(path.join(opts.root, 'projects'), { recursive: true, force: true })
  const dirs = Object.fromEntries(
    (Object.keys(PROJECTS) as ProjectKey[]).map((key) => [
      key,
      path.join(opts.root, 'projects', PROJECTS[key].name)
    ])
  ) as Record<ProjectKey, string>
  try {
    for (const key of Object.keys(PROJECTS) as ProjectKey[]) {
      await addProject(server, { ...PROJECTS[key], path: dirs[key] })
    }
    const alphaHead = seedAlphaRepo(dirs.alpha)
    write(dirs.beta, 'index.js', "console.log('beta')\n")
    write(dirs.gamma, 'todo.md', '# gamma\n')
    write(dirs.delta, 'todo.md', '# delta\n')
    write(server.home, '.claude/skills/matrix-review/SKILL.md', MATRIX_SKILL)

    const ws = await connectWs({
      baseUrl: server.baseUrl,
      token: TOKEN,
      deviceId: 'matrix-seed',
      clientInstanceId: 'matrix-seed'
    })
    await ws.request('set_default_project', { projectId: PROJECTS.alpha.id })
    await registerAgent(ws, 'matrix-seed')
    const { agentId } = await ws.request<{ agentId: string }>(
      'spawn_agent',
      { config: AGENT, projectId: PROJECTS.alpha.id },
      { commandId: commandId('spawn') }
    )
    const { sessionId } = await ws.request<{ sessionId: string }>(
      'create_session',
      { agentId, cwd: dirs.alpha, projectId: PROJECTS.alpha.id },
      { commandId: commandId('session') }
    )
    for (const prompt of SCRIPT_PROMPTS) await sendPrompt(ws, { agentId, sessionId }, prompt)
    // Opening the chat marks it read on the host for every device, and the
    // marker never moves back, so start read: every run then paints the same row.
    await ws.request('mark_chat_read', { sessionId, seq: Number.MAX_SAFE_INTEGER })
    console.error(`[parity] seeded chat ${sessionId} on agent ${agentId}`)
    const clock = Math.ceil((Date.now() + CLOCK_AFTER_SEED_MS) / 60_000) * 60_000
    return { server, ws, root: opts.root, dirs, chat: { agentId, sessionId }, alphaHead, clock }
  } catch (error) {
    await server.stop()
    throw error
  }
}

/** Give a fresh device the scripted agent as its launcher pick. */
export async function prepareDevice(world: World, deviceId: string): Promise<void> {
  // A device key is writable only from a connection that authenticated as that device.
  const ws = await connectWs({
    baseUrl: world.server.baseUrl,
    token: TOKEN,
    deviceId,
    clientInstanceId: `${deviceId}-seed`
  })
  try {
    await registerAgent(ws, deviceId)
  } finally {
    ws.close()
  }
}

/** Open `filePath` in one editor tab of alpha's only pane on `deviceId` (the workspace record). */
export async function seedEditorTab(
  world: World,
  deviceId: string,
  filePath: string,
  viewMode: 'code' | 'preview' = 'code'
): Promise<void> {
  const tabId = `edit-${filePath}`
  await writeDevice(world, deviceId, `workspace/${PROJECTS.alpha.id}`, {
    openFiles: [
      {
        filePath,
        cursorPosition: { line: 1, col: 1 },
        scrollTop: 0,
        viewMode,
        isDirty: false,
        lastModified: 0
      }
    ],
    activeFilePath: filePath,
    expandedDirs: [],
    activeTabId: tabId,
    paneLayout: {
      type: 'leaf',
      id: 'editor-lane-pane',
      tabs: [{ type: 'editor', filePath }],
      activeTabId: tabId
    },
    activePaneId: 'editor-lane-pane'
  })
}

/** The workspace snapshot `seedSnapshot` saves for alpha. */
export const SNAPSHOT_NAME = 'Before the route refactor'

/** Write a device-scoped key the way its store writes it, from a connection as that device. */
async function writeDevice(
  world: World,
  deviceId: string,
  logicalKey: string,
  data: unknown
): Promise<void> {
  // A device key is writable only from a connection that authenticated as that device.
  const ws = await connectWs({
    baseUrl: world.server.baseUrl,
    token: TOKEN,
    deviceId,
    clientInstanceId: `${deviceId}-seed-${logicalKey}`
  })
  try {
    const key = scopedKey(logicalKey, deviceId)
    const current = await ws.request<{ revision?: number }>('store_read', { key })
    await ws.request('store_write', {
      key,
      value: { _version: 1, data },
      baseRevision: current.revision ?? 0
    })
  } finally {
    ws.close()
  }
}

/** Save one workspace snapshot of alpha on `deviceId` (snapshots are kept per device). */
export async function seedSnapshot(world: World, deviceId: string): Promise<void> {
  const createdAt = new Date(world.clock - 3_600_000).toISOString()
  await writeDevice(world, deviceId, `snapshots/${PROJECTS.alpha.id}`, {
    snapshots: [
      {
        id: 'matrix-snapshot',
        projectId: PROJECTS.alpha.id,
        name: SNAPSHOT_NAME,
        description: 'Two terminals on main',
        createdAt,
        terminals: [],
        activeTerminalId: null
      }
    ],
    updatedAt: createdAt
  })
}

/** The command `seedCommandHistory` records in alpha's terminal history. */
export const HISTORY_COMMAND = 'bun vitest run src/renderer/lib'

/** Record one terminal command for alpha on `deviceId` (command history is kept per device). */
export async function seedCommandHistory(world: World, deviceId: string): Promise<void> {
  await writeDevice(world, deviceId, `projects/${PROJECTS.alpha.id}/command-history`, [
    {
      id: 'matrix-command',
      command: HISTORY_COMMAND,
      terminalName: 'Terminal 1',
      terminalId: 'matrix-terminal',
      projectId: PROJECTS.alpha.id,
      timestamp: world.clock - 600_000
    }
  ])
}

/** The automation `seedAutomation` saves in alpha: a manual trigger and one HTTP step. */
export const AUTOMATION_NAME = 'Nightly digest'

/** Save `AUTOMATION_NAME` in alpha unless an earlier run already did. */
export async function seedAutomation(world: World): Promise<void> {
  const route = '/api/automations'
  const json = async (res: Response) => ((await res.json()) as { data: unknown }).data
  const list = (await json(
    await serverFetch(world.server, `${route}?project_id=${PROJECTS.alpha.id}`)
  )) as { id: string; name: string; graph: { nodes: { id: string }[] } }[]
  const existing = list.find((row) => row.name === AUTOMATION_NAME)
  if (existing?.graph.nodes.some((node) => node.id === 'fetch')) return
  // A seed whose step request failed: start it over.
  if (existing) await serverFetch(world.server, `${route}/${existing.id}`, { method: 'DELETE' })
  const created = (await json(
    await serverFetch(world.server, route, {
      method: 'POST',
      body: JSON.stringify({ project_id: PROJECTS.alpha.id, name: AUTOMATION_NAME })
    })
  )) as { id: string; revision: number }
  const fetchStep = {
    id: 'fetch',
    label: 'Fetch the feed',
    spec: {
      kind: 'http',
      method: 'GET',
      url: 'https://example.invalid/feed.json',
      headers: [],
      body: null,
      allowed_hosts: ['example.invalid']
    }
  }
  await serverFetch(world.server, `${route}/${created.id}/ops`, {
    method: 'POST',
    body: JSON.stringify({
      base_revision: created.revision,
      ops: [
        { op: 'add_node', node: fetchStep },
        { op: 'connect', edge: { from: 'trigger', port: 'out', to: 'fetch' } }
      ]
    })
  })
}

/**
 * Stop every agent but the seeded chat's. Each client run starts the scripted
 * agent for its fresh device, and the host keeps it until the idle timeout,
 * so a full run would otherwise hold hundreds of agent processes.
 */
export async function closeSpareAgents(world: World): Promise<number> {
  const agents = await world.ws.request<{ id: string }[]>('list_agents', {})
  const spare = agents.filter((agent) => agent.id !== world.chat.agentId)
  for (const agent of spare) await world.ws.request('kill_agent', { agentId: agent.id })
  return spare.length
}

/** Re-add `key` when an earlier run removed it, and restore its seeded name and colour. */
export async function ensureProject(world: World, key: ProjectKey): Promise<void> {
  const project = PROJECTS[key]
  const list = (await (await serverFetch(world.server, '/projects')).json()) as unknown
  const rows = (
    Array.isArray(list) ? list : ((list as { projects?: unknown[] }).projects ?? [])
  ) as { id: string }[]
  if (!rows.some((row) => row.id === project.id)) {
    await addProject(world.server, { ...project, path: world.dirs[key] })
    return
  }
  await serverFetch(world.server, `/projects/${project.id}`, {
    method: 'PUT',
    body: JSON.stringify({ name: project.name, color: project.color })
  })
}

/** Rewind alpha to the seeded commit and working-tree changes. */
export async function resetAlpha(world: World): Promise<void> {
  resetAlphaChanges(world.dirs.alpha, world.alphaHead)
}

interface StoreRead {
  revision?: number
  value?: { _version?: number; data?: Record<string, unknown> } | null
}

/** Replace a shared last-writer-wins key the way its store writes it. */
export async function writeShared(world: World, logicalKey: string, data: unknown): Promise<void> {
  const key = scopedKey(logicalKey, 'matrix-seed')
  const current = await world.ws.request<StoreRead>('store_read', { key })
  await world.ws.request('store_write', {
    key,
    value: { _version: 1, data },
    baseRevision: current.revision ?? 0
  })
}

/**
 * Put the shared settings every client run starts from: the default theme in
 * dark mode at 100 % zoom, the default key bindings and the default outline, so a surface that
 * changes them does not leak into the next run.
 */
export async function resetSharedSettings(world: World): Promise<void> {
  const key = scopedKey('settings/app', 'matrix-seed')
  const current = await world.ws.request<StoreRead>('store_read', { key })
  await writeShared(world, 'settings/app', {
    ...(current.value?.data ?? {}),
    colorTheme: 'termul',
    appearanceMode: 'dark',
    uiZoomLevel: 1
  })
  await writeShared(world, 'settings/keyboard-shortcuts', {})
  await writeShared(world, 'settings/toc', { isVisible: true, maxHeadingLevel: 3, width: 240 })
}
