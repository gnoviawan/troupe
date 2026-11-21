// Runs the frontend tests on a pinned Bun canary. Stable Bun lacks the jsdom
// environment, --parallel and --files-from. The canary is downloaded once into
// node_modules/.cache and checked against its npm sha512 integrity.
//   bun run test                         every test file (scripts/ci/test-files.ts)
//   bun run test src/a.test.ts           only the given files
//   bun run test --seed=123              reproduce a run's test order
import { existsSync, mkdirSync, mkdtempSync, renameSync } from 'node:fs'
import { availableParallelism, tmpdir } from 'node:os'
import { join } from 'node:path'
import { listTestFiles } from './ci/test-files'

const VERSION = '1.4.5-canary.20261120.1'
// npm view @oven/bun-<platform>@<VERSION> dist.integrity
const INTEGRITY: Record<string, string> = {
  'linux-x64':
    'sha512-WeORe6WhB0QEhPu739WCWKFswaplOIoP21RgEk3NmHC5gNOOKmHMiQZuy29bnEwtGqQ8LuBZ+SI57RHxbOhfZQ==',
  'linux-aarch64':
    'sha512-SrxL7YlYt+hKr6AnKEWKR/dmcnJppx+sn3fDkTRm2JOijKjlFTV3hC5aLwwxpbCn+SUtB3bs8qejWTpJBifAMg==',
  'darwin-aarch64':
    'sha512-Hif4uT/VQH8H3s34XkP8BeZ11M3NYE0eIdHughcYRgoCxkojrDkY+Ecu7F4e7GyMzD/vBscwoF7ZASMHg7LkpQ==',
  'darwin-x64':
    'sha512-w/OzDBR7K+3dBSrLWvpEfJ7y6meRJjAI9pHP+HwbOBrxNnVYoUsVWxzRDy3XOjszhU8qXh4jEnm/BYLTLHZ2nA==',
  'windows-x64':
    'sha512-tekkM05MRHeosfwWD26hKJ1c9HB4qmQqQkjPfBQhFAljPoA1hj5N5YqPSNYtgRWn69tPJv7mnnzyDJLE3gLOyQ=='
}

export async function canaryBun(): Promise<string> {
  const platform = `${process.platform === 'win32' ? 'windows' : process.platform}-${process.arch === 'arm64' ? 'aarch64' : process.arch}`
  const integrity = INTEGRITY[platform]
  if (!integrity) throw new Error(`no pinned Bun canary for ${platform}`)
  const dir = join(
    import.meta.dir,
    '..',
    'node_modules',
    '.cache',
    'bun-canary',
    `${VERSION}-${platform}`
  )
  const bin = join(dir, 'package', 'bin', process.platform === 'win32' ? 'bun.exe' : 'bun')
  if (existsSync(bin)) return bin

  const url = `https://registry.npmjs.org/@oven/bun-${platform}/-/bun-${platform}-${VERSION}.tgz`
  console.error(`Downloading Bun ${VERSION} for ${platform}`)
  const response = await fetch(url)
  if (!response.ok) throw new Error(`${url}: HTTP ${response.status}`)
  const tarball = await response.arrayBuffer()
  const actual = `sha512-${new Bun.CryptoHasher('sha512').update(tarball).digest('base64')}`
  if (actual !== integrity) throw new Error(`Bun canary integrity mismatch: got ${actual}`)

  mkdirSync(join(dir, '..'), { recursive: true })
  const staging = mkdtempSync(`${dir}.tmp-`)
  await Bun.write(join(staging, 'bun.tgz'), tarball)
  await Bun.$`tar -xzf bun.tgz package/bin`.cwd(staging)
  try {
    renameSync(staging, dir)
  } catch (error) {
    // Another run installed it first.
    if (!existsSync(bin)) throw error
  }
  return bin
}

if (import.meta.main) {
  const args = process.argv.slice(2)
  if (!args.some((arg) => !arg.startsWith('-') || arg.startsWith('--files-from'))) {
    const list = join(mkdtempSync(join(tmpdir(), 'bun-test-')), 'files.txt')
    await Bun.write(list, `${(await listTestFiles()).join('\n')}\n`)
    args.push(`--files-from=${list}`)
  }
  // --parallel also runs each file in a fresh global. 15 s per test: CI
  // runners are slower than local machines, and async UI tests (waitFor and
  // act chains) need the headroom. Bun's default is 5 s.
  const bun = await canaryBun()
  const command = [bun, 'test', `--parallel=${availableParallelism()}`, '--timeout=15000', ...args]
  const child = Bun.spawn(command, { stdio: ['inherit', 'inherit', 'inherit'] })
  process.exit(await child.exited)
}
