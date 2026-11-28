/**
 * R11c Lane 7: open `apps/rune/fixtures/sample.figcraft` in `troupe-preview` (under a private
 * Xvfb with lavapipe), run a real agent canvas edit through `troupe-server`'s injected MCP
 * server (`create_node` + `save_file`), and verify the new node is in both the rendered frame
 * and the saved `.figcraft` archive on disk.
 *
 *   bun tools/parity/lanes/canvas-desktop.ts --port 8198 --root /tmp/rune-r11c/desktop \
 *     --server-bin <troupe-server> --bin <troupe-preview> [--out <r11-canvas.png>]
 */

import { copyFileSync, existsSync, mkdirSync, readFileSync } from 'node:fs'
import path from 'node:path'
import { parseArgs } from 'node:util'
import { runeBinary } from '../drivers/rune-apps'
import { openRuneDesktop } from '../drivers/rune-desktop'
import { REPO_DIR } from '../server'
import { createWorld, PROJECTS, prepareDevice, TOKEN } from '../world'
import { commandId } from '../ws'

const { values } = parseArgs({
  args: Bun.argv.slice(2),
  options: {
    port: { type: 'string', default: '8198' },
    root: { type: 'string', default: '/tmp/rune-r11c/desktop' },
    'server-bin': { type: 'string' },
    bin: { type: 'string' },
    out: { type: 'string' }
  }
})

const root = path.resolve(values.root as string)
const DEVICE = 'canvas-desktop-lane'
const FIXTURE = path.join(REPO_DIR, 'apps/rune/fixtures/sample.figcraft')
const CANVAS_AGENT = path.join(REPO_DIR, 'tools/parity/agent/canvas-agent.ts')

const notes: string[] = []
const check = (ok: boolean, what: string): boolean => {
  notes.push(`${ok ? 'ok' : 'FAIL'} ${what}`)
  console.log(`canvas-desktop: ${ok ? 'ok' : 'FAIL'} ${what}`)
  return ok
}

function readDocumentJson(figcraftPath: string): string {
  const proc = Bun.spawnSync(
    [
      'python3',
      '-c',
      'import sys, zipfile; print(zipfile.ZipFile(sys.argv[1]).read("document.json").decode("utf-8"))',
      figcraftPath
    ],
    { stdout: 'pipe', stderr: 'pipe' }
  )
  if (proc.exitCode !== 0) {
    throw new Error(`unzip ${figcraftPath}: ${proc.stderr.toString()}`)
  }
  return proc.stdout.toString()
}

const world = await createWorld({
  port: Number(values.port),
  root,
  binary: values['server-bin'] as string | undefined
})
const docPath = path.join(world.dirs.alpha, 'sample.figcraft')
copyFileSync(FIXTURE, docPath)
let ok = true
try {
  await prepareDevice(world, DEVICE)
  const previewBin = (values.bin as string | undefined) ?? runeBinary('troupe')
  const previewData = path.join(root, 'preview-data')
  mkdirSync(previewData, { recursive: true })
  const session = await openRuneDesktop({
    bin: previewBin,
    stateDir: path.join(root, 'app'),
    ready: 'What should we do in alpha?',
    env: {
      TROUPE_SERVER_URL: world.server.baseUrl,
      TROUPE_SERVER_TOKEN: TOKEN,
      TROUPE_DEVICE_ID: DEVICE,
      TROUPE_ROUTE: '',
      TROUPE_PREVIEW_DATA_DIR: previewData
    }
  })
  const { socket } = session
  try {
    await socket.must({ cmd: 'click', key: 'rail-canvas' })
    ok =
      check(
        (await socket.send({ cmd: 'wait', text: 'Ready', timeout_ms: 45_000 })).ok,
        'canvas tab opened sample.figcraft and reached Ready'
      ) && ok
    // Let FigCraft's render worker finish its initial tile render.
    await Bun.sleep(2500)
    const beforePath = path.join(root, 'r11-canvas-before.png')
    await socket.must({ cmd: 'snap', path: beforePath })

    // Run a real agent canvas edit through troupe-server's injected MCP server.
    const spawned = await world.ws.request<{ agentId?: string; id?: string }>(
      'spawn_agent',
      {
        config: {
          configId: 'canvas-lane-agent',
          name: 'Canvas Lane Agent',
          command: 'bun',
          args: [CANVAS_AGENT],
          env: {},
          allowTerminal: false
        },
        projectId: PROJECTS.alpha.id
      },
      { commandId: commandId('canvas-spawn') }
    )
    const agentId = spawned.agentId ?? spawned.id
    if (!agentId) throw new Error(`spawn_agent returned no agentId: ${JSON.stringify(spawned)}`)
    const created = await world.ws.request<{ sessionId?: string }>(
      'create_session',
      {
        agentId,
        cwd: world.dirs.alpha,
        projectId: PROJECTS.alpha.id
      },
      { commandId: commandId('canvas-session') }
    )
    const sessionId = created.sessionId
    if (!sessionId)
      throw new Error(`create_session returned no sessionId: ${JSON.stringify(created)}`)

    const turnId = commandId('canvas-turn')
    await world.ws.request(
      'send_prompt',
      { agentId, sessionId, text: 'Add Agent Banner and save', turnId },
      { commandId: turnId, timeoutMs: 60_000 }
    )

    // Allow the compositor and render worker to paint the new node.
    await Bun.sleep(2500)
    const afterPath = values.out
      ? path.resolve(values.out as string)
      : path.join(root, 'r11-canvas.png')
    mkdirSync(path.dirname(afterPath), { recursive: true })
    await socket.must({ cmd: 'snap', path: afterPath })

    const beforeBytes = readFileSync(beforePath)
    const afterBytes = readFileSync(afterPath)
    ok =
      check(
        Buffer.compare(beforeBytes, afterBytes) !== 0,
        'rendered frame changed after agent edit'
      ) && ok

    const savedJson = readDocumentJson(docPath)
    ok =
      check(savedJson.includes('Agent Banner'), 'saved sample.figcraft contains "Agent Banner"') &&
      ok
    ok = check(existsSync(afterPath), `saved screenshot ${afterPath}`) && ok
  } finally {
    await session.close()
  }
} catch (error) {
  ok = check(false, String(error))
} finally {
  world.ws.close()
  await world.server.stop()
}

await Bun.write(path.join(root, 'results.json'), JSON.stringify({ ok, notes }, null, 2))
console.log(`canvas-desktop: ${ok ? 'PASS' : 'FAIL'}`)
process.exitCode = ok ? 0 : 1
