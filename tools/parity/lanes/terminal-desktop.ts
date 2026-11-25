/**
 * The R7b `troupe-dev` lane: the native Rune app under a private Xvfb (lavapipe), driven by
 * the live driver. It opens a terminal, runs `depth/term-box.sh` and an `echo`, snaps the
 * window, opens the command history modal with Ctrl+R (the desktop default), and reads the
 * window's accessibility tree for the terminal and its caret (plan R7 lane 10, desktop half).
 *
 *   bun tools/parity/lanes/terminal-desktop.ts --port 8197 --root /tmp/rune-r7b/desktop \
 *     --server-bin <troupe-server> [--bin <troupe-dev>]
 */

import path from 'node:path'
import { parseArgs } from 'node:util'
import { runeBinary } from '../drivers/rune-apps'
import { openRuneDesktop } from '../drivers/rune-desktop'
import { REPO_DIR } from '../server'
import { createWorld, prepareDevice, seedCommandHistory, TOKEN } from '../world'

const { values } = parseArgs({
  args: Bun.argv.slice(2),
  options: {
    port: { type: 'string', default: '8197' },
    root: { type: 'string', default: '/tmp/troupe-parity/terminal-desktop' },
    'server-bin': { type: 'string' },
    bin: { type: 'string' }
  }
})
const root = path.resolve(values.root as string)
const DEVICE = 'terminal-desktop-rune'
const MARK = 'rows-reach-the-tree'

const notes: string[] = []
const check = (ok: boolean, what: string) => {
  notes.push(`${ok ? 'ok' : 'FAIL'} ${what}`)
  console.log(`troupe-dev: ${ok ? 'ok' : 'FAIL'} ${what}`)
  return ok
}

const world = await createWorld({ port: Number(values.port), root, binary: values['server-bin'] })
let ok = true
try {
  await prepareDevice(world, DEVICE)
  await seedCommandHistory(world, DEVICE)
  const session = await openRuneDesktop({
    bin: values.bin ?? runeBinary('troupe'),
    stateDir: path.join(root, 'app'),
    ready: 'What should we do in alpha?',
    env: {
      TROUPE_SERVER_URL: world.server.baseUrl,
      TROUPE_SERVER_TOKEN: TOKEN,
      TROUPE_DEVICE_ID: DEVICE,
      TROUPE_ROUTE: ''
    }
  })
  const { socket } = session
  try {
    await socket.must({ cmd: 'click', key: 'tool-strip-terminal' })
    ok =
      check(
        (await socket.send({ cmd: 'wait', text: 'Terminal 1', timeout_ms: 30_000 })).ok,
        'a terminal tab opened'
      ) && ok
    await Bun.sleep(2500)
    const screen = (await socket.must({ cmd: 'tree' })).elements as Array<{
      key: string | null
      rect: number[]
    }>
    const grid = screen.find(
      (e) => !!e.key && e.key.startsWith('terminal-') && e.key.endsWith('-grid')
    )
    if (grid) await socket.must({ cmd: 'click', x: grid.rect[0] + 200, y: grid.rect[1] + 200 })
    await socket.must({
      cmd: 'type',
      text: `clear; bash ${REPO_DIR}/tools/parity/depth/term-box.sh; echo ${MARK}`
    })
    await socket.must({ cmd: 'key', name: 'Enter' })
    await Bun.sleep(3000)
    await socket.must({ cmd: 'snap', path: path.join(root, 'rune-desktop-box.png') })
    // The live driver's `a11y` dump leaves out text runs (`accessibility_json`), so it shows
    // the screen as a `terminal` node whose caret is the shell's cursor row; the rows' text is
    // checked against `accessibility_tree` in `terminal_tests.rs`.
    const a11y = JSON.stringify(await socket.must({ cmd: 'a11y' }))
    const caret = /"role":"terminal"[^}]*"selection":\[(\d+),(\d+)/.exec(a11y)
    ok =
      check(
        !!caret && Number(caret[1]) >= 7,
        `the tree has the terminal, its caret on row ${caret?.[1]} under the output`
      ) && ok
    await Bun.write(path.join(root, 'rune-desktop-a11y.json'), a11y)
    await socket.must({ cmd: 'key', name: 'r', mods: ['ctrl'] })
    ok =
      check(
        (await socket.send({ cmd: 'wait', text: 'Command History', timeout_ms: 10_000 })).ok,
        'Ctrl+R opened the history modal'
      ) && ok
    ok =
      check(
        (
          await socket.send({
            cmd: 'wait',
            text: 'bun vitest run src/renderer/lib',
            timeout_ms: 10_000
          })
        ).ok,
        'it lists the seeded command'
      ) && ok
    await socket.must({ cmd: 'snap', path: path.join(root, 'rune-desktop-history.png') })
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
console.log(`troupe-dev: ${ok ? 'PASS' : 'FAIL'}`)
process.exitCode = ok ? 0 : 1
