import assert from 'node:assert/strict'
import { execFile } from 'node:child_process'
import { mkdtempSync } from 'node:fs'
import { createServer } from 'node:net'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { createInterface } from 'node:readline'
import test from 'node:test'
import { drive, parseArgs } from './drive.mjs'

const SCRIPT = new URL('./drive.mjs', import.meta.url).pathname

const sockPath = () => join(mkdtempSync(join(tmpdir(), 'rune-drive-')), 'drive.sock')

// A fake driver: records each request line and answers with `answer(cmd)`.
function fakeDriver(path, answer = (cmd) => ({ ok: true, cmd: cmd.cmd })) {
  const received = []
  const server = createServer((conn) => {
    createInterface({ input: conn }).on('line', (line) => {
      received.push(line)
      conn.write(`${JSON.stringify(answer(JSON.parse(line)))}\n`)
    })
  })
  return new Promise((resolve) => server.listen(path, () => resolve({ server, received })))
}

const close = (server) => new Promise((resolve) => server.close(resolve))

test('sends each command as one line and prints every reply in order', async () => {
  const path = sockPath()
  const { server, received } = await fakeDriver(path)
  const printed = []
  const ok = await drive(
    path,
    [
      { cmd: 'click', key: 'press' },
      { cmd: 'find', text: 'Pressed 1' }
    ],
    {
      print: (line) => printed.push(line)
    }
  )
  await close(server)
  assert.equal(ok, true)
  assert.deepEqual(received, ['{"cmd":"click","key":"press"}', '{"cmd":"find","text":"Pressed 1"}'])
  assert.deepEqual(printed, ['{"ok":true,"cmd":"click"}', '{"ok":true,"cmd":"find"}'])
})

test('stops at the first reply that is not ok', async () => {
  const path = sockPath()
  const { server, received } = await fakeDriver(path, (cmd) =>
    cmd.cmd === 'find' ? { ok: false, error: 'not found' } : { ok: true }
  )
  const printed = []
  const ok = await drive(
    path,
    [
      { cmd: 'click', key: 'press' },
      { cmd: 'find', key: 'x' },
      { cmd: 'snap', path: 'a.png' }
    ],
    {
      print: (line) => printed.push(line)
    }
  )
  await close(server)
  assert.equal(ok, false)
  assert.deepEqual(received, ['{"cmd":"click","key":"press"}', '{"cmd":"find","key":"x"}'])
  assert.deepEqual(printed, ['{"ok":true}', '{"ok":false,"error":"not found"}'])
})

test('retries the connection until the socket appears', async () => {
  const path = sockPath()
  let started
  setTimeout(() => {
    started = fakeDriver(path)
  }, 300)
  const ok = await drive(path, [{ cmd: 'tree' }], { timeoutMs: 5000, print: () => {} })
  const { server, received } = await started
  await close(server)
  assert.equal(ok, true)
  assert.deepEqual(received, ['{"cmd":"tree"}'])
})

test('gives up when no socket appears before the timeout', async () => {
  await assert.rejects(drive(sockPath(), [{ cmd: 'tree' }], { timeoutMs: 200 }), { code: 'ENOENT' })
})

test('parseArgs reads the timeout, the socket and JSON commands', () => {
  assert.deepEqual(parseArgs(['--connect-timeout', '500', '/tmp/s', '{"cmd":"tree"}']), {
    path: '/tmp/s',
    commands: [{ cmd: 'tree' }],
    timeoutMs: 500
  })
  assert.throws(() => parseArgs(['/tmp/s']), /usage/)
  assert.throws(() => parseArgs(['/tmp/s', '{cmd']), /not JSON: \{cmd/)
})

test('the CLI exits 1 when a reply is not ok, after printing it', async () => {
  const path = sockPath()
  const { server } = await fakeDriver(path, () => ({ ok: false, error: 'timeout' }))
  const result = await new Promise((resolve) =>
    execFile(process.execPath, [SCRIPT, path, '{"cmd":"wait","text":"never"}'], (error, stdout) =>
      resolve({ code: error?.code ?? 0, stdout })
    )
  )
  await close(server)
  assert.deepEqual(result, { code: 1, stdout: '{"ok":false,"error":"timeout"}\n' })
})
