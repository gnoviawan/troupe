/**
 * The R8b `troupe-dev` video lane: the native Rune app under a private Xvfb (lavapipe), driven
 * by the live driver. It opens an MP4 in an editor tab, presses Play, and compares two frames
 * taken 1 s apart: the picture must move.
 *
 *   bun tools/parity/lanes/viewers-desktop.ts --port 8193 --root /tmp/rune-r8b/desktop \
 *     --server-bin <troupe-server> --mp4 clip.mp4 [--bin <troupe-dev>]
 */

import { copyFileSync, readFileSync } from 'node:fs'
import path from 'node:path'
import { parseArgs } from 'node:util'
import { runeBinary } from '../drivers/rune-apps'
import { openRuneDesktop } from '../drivers/rune-desktop'
import { createWorld, prepareDevice, seedEditorTab, TOKEN } from '../world'

const { values } = parseArgs({
  args: Bun.argv.slice(2),
  options: {
    port: { type: 'string', default: '8193' },
    root: { type: 'string', default: '/tmp/troupe-parity/viewers-desktop' },
    'server-bin': { type: 'string' },
    mp4: { type: 'string' },
    bin: { type: 'string' }
  }
})
const root = path.resolve(values.root as string)
const DEVICE = 'viewers-desktop-rune'
const notes: string[] = []

function check(ok: boolean, what: string): boolean {
  notes.push(`${ok ? 'ok' : 'FAIL'} ${what}`)
  console.log(`troupe-dev: ${ok ? 'ok' : 'FAIL'} ${what}`)
  return ok
}

const world = await createWorld({ port: Number(values.port), root, binary: values['server-bin'] })
const file = path.join(world.dirs.alpha, 'clip.mp4')
copyFileSync(values.mp4 as string, file)
let ok = true
try {
  await prepareDevice(world, DEVICE)
  await seedEditorTab(world, DEVICE, file)
  const session = await openRuneDesktop({
    bin: values.bin ?? runeBinary('troupe'),
    stateDir: path.join(root, 'app'),
    ready: 'clip.mp4',
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
        (await socket.send({ cmd: 'wait', text: 'Play', timeout_ms: 30_000 })).ok,
        'the video opens with its Play button'
      ) && ok
    await socket.must({ cmd: 'snap', path: path.join(root, 'video-0-still.png') })
    await socket.must({ cmd: 'click', key: 'viewer-video-play' })
    await Bun.sleep(1000)
    await socket.must({ cmd: 'snap', path: path.join(root, 'video-1.png') })
    await Bun.sleep(1000)
    await socket.must({ cmd: 'snap', path: path.join(root, 'video-2.png') })
    const [a, b] = ['video-1.png', 'video-2.png'].map((f) => readFileSync(path.join(root, f)))
    ok = check(Buffer.compare(a, b) !== 0, 'two frames 1 s apart differ') && ok
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
