/**
 * The R8a `troupe-dev` lane: the native Rune app under a private Xvfb (lavapipe), driven by
 * the live driver. It opens a Rust file in an editor tab (seeded in the workspace record),
 * types at its end, saves with Ctrl+S and compares the file on disk; then it opens the Explorer
 * panel and creates a file from the header's New File.
 *
 *   bun tools/parity/lanes/editor-desktop.ts --port 8191 --root /tmp/rune-r8a/desktop \
 *     --server-bin <troupe-server> [--bin <troupe-dev>]
 */

import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import path from 'node:path'
import { parseArgs } from 'node:util'
import { runeBinary } from '../drivers/rune-apps'
import { openRuneDesktop } from '../drivers/rune-desktop'
import { createWorld, prepareDevice, seedEditorTab, TOKEN } from '../world'

const { values } = parseArgs({
  args: Bun.argv.slice(2),
  options: {
    port: { type: 'string', default: '8191' },
    root: { type: 'string', default: '/tmp/troupe-parity/editor-desktop' },
    'server-bin': { type: 'string' },
    bin: { type: 'string' }
  }
})
const root = path.resolve(values.root as string)
const DEVICE = 'editor-desktop-rune'
const TYPED = '// saved from troupe-dev'

const notes: string[] = []
const check = (ok: boolean, what: string) => {
  notes.push(`${ok ? 'ok' : 'FAIL'} ${what}`)
  console.log(`troupe-dev: ${ok ? 'ok' : 'FAIL'} ${what}`)
  return ok
}
const until = async (test: () => boolean, ms: number) => {
  const end = Date.now() + ms
  while (Date.now() < end) {
    if (test()) return true
    await Bun.sleep(200)
  }
  return test()
}

const world = await createWorld({ port: Number(values.port), root, binary: values['server-bin'] })
const file = path.join(world.dirs.alpha, 'src', 'desktop_lane.rs')
mkdirSync(path.dirname(file), { recursive: true })
writeFileSync(file, 'pub fn desktop() -> u32 {\n    7\n}\n')
let ok = true
try {
  await prepareDevice(world, DEVICE)
  await seedEditorTab(world, DEVICE, file)
  const session = await openRuneDesktop({
    bin: values.bin ?? runeBinary('troupe'),
    stateDir: path.join(root, 'app'),
    ready: 'desktop_lane.rs',
    env: {
      TROUPE_SERVER_URL: world.server.baseUrl,
      TROUPE_SERVER_TOKEN: TOKEN,
      TROUPE_DEVICE_ID: DEVICE,
      TROUPE_ROUTE: ''
    }
  })
  const { socket } = session
  try {
    ok =
      check(
        (await socket.send({ cmd: 'wait', text: 'pub fn desktop() -> u32 {', timeout_ms: 30_000 }))
          .ok,
        'the file shows in its tab'
      ) && ok
    await socket.must({ cmd: 'click', key: `editor-surface-${file}` }).catch(() => undefined)
    await socket.must({ cmd: 'key', name: 'End', mods: ['ctrl'] })
    await socket.must({ cmd: 'type', text: TYPED })
    await socket.must({ cmd: 'key', name: 's', mods: ['ctrl'] })
    ok =
      check(
        await until(() => readFileSync(file, 'utf8').endsWith(TYPED), 10_000),
        'Ctrl+S wrote the typed line to disk'
      ) && ok
    await socket.must({ cmd: 'snap', path: path.join(root, 'rune-desktop-saved.png') })

    await socket.must({ cmd: 'click', key: 'tool-strip-explorer' })
    ok =
      check(
        (await socket.send({ cmd: 'wait', text: 'README.md', timeout_ms: 20_000 })).ok,
        'the Explorer lists the project'
      ) && ok
    await socket.must({ cmd: 'click', key: 'explorer-new-file' })
    await Bun.sleep(500)
    await socket.must({ cmd: 'type', text: 'from_desktop.rs' })
    await socket.must({ cmd: 'key', name: 'Enter' })
    ok =
      check(
        await until(() => existsSync(path.join(world.dirs.alpha, 'from_desktop.rs')), 10_000),
        'New File created from_desktop.rs on disk'
      ) && ok
    await Bun.sleep(1000)
    await socket.must({ cmd: 'snap', path: path.join(root, 'rune-desktop-explorer.png') })
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
