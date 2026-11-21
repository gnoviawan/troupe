// Runs from `prepare` on `bun install`: fetches the pinned hk release, checks it
// against the sha256 below, and installs the Git hooks defined in hk.pkl.
// Set HK=0 to skip. CI skips it because CI runs the checks itself.
import { existsSync } from 'node:fs'
import { mkdir, mkdtemp, rename, rm } from 'node:fs/promises'
import { join } from 'node:path'

const VERSION = '2.13.0'

// sha256 of each release archive, taken from https://github.com/jdx/hk/releases/tag/v2.13.0.
// hk publishes no Intel macOS build.
const ASSETS: Record<string, { file: string; sha256: string }> = {
  'linux-x64': {
    file: 'hk-x86_64-unknown-linux-musl.tar.gz',
    sha256: '3d5ae62e2befe95fa6fc7ffd63a0876c3efe3409634fbd6561b8bb0be787eeae'
  },
  'linux-arm64': {
    file: 'hk-aarch64-unknown-linux-musl.tar.gz',
    sha256: 'cff2d433a82104ace7084a23970ff05b0bf3ca529e00c61da20780104106a3dd'
  },
  'darwin-arm64': {
    file: 'hk-aarch64-apple-darwin.tar.gz',
    sha256: '64efc441dc0858a175909fd6253ddd6c540da80e7d1bd2a2ad0ec9c0cdab3d97'
  },
  'win32-x64': {
    file: 'hk-x86_64-pc-windows-msvc.zip',
    sha256: '2908de6281368b8128f1d4d7a03905d3ab0eb86a82641d6055be88806bf9ffac'
  },
  'win32-arm64': {
    file: 'hk-aarch64-pc-windows-msvc.zip',
    sha256: '54d34ae2e9ce7307658c69021801a2aad64149614eab60640dab7a63a18e2025'
  }
}

const log = (message: string) => console.log(`[hk] ${message}`)
const root = join(import.meta.dir, '..', '..')
const git = (...args: string[]) => Bun.spawnSync(['git', ...args], { cwd: root })

async function fetchArchive(file: string): Promise<Uint8Array | null> {
  const url = `https://github.com/jdx/hk/releases/download/v${VERSION}/${file}`
  try {
    const response = await fetch(url)
    if (!response.ok) throw new Error(`GET ${url} returned ${response.status}`)
    return new Uint8Array(await response.arrayBuffer())
  } catch (error) {
    log(`download failed (${error}); Git hooks not installed, rerun bun install to retry`)
    return null
  }
}

// Returns false when this machine cannot get hk; throws when the download is not the pinned file.
async function installBinary(dir: string, exe: string): Promise<boolean> {
  const asset = ASSETS[`${process.platform}-${process.arch}`]
  if (!asset) {
    log(`no hk ${VERSION} build for ${process.platform}-${process.arch}; Git hooks not installed`)
    return false
  }
  const archive = await fetchArchive(asset.file)
  if (!archive) return false
  const actual = new Bun.CryptoHasher('sha256').update(archive).digest('hex')
  if (actual !== asset.sha256) {
    throw new Error(`checksum mismatch for ${asset.file}: expected ${asset.sha256}, got ${actual}`)
  }
  await mkdir(join(dir, '..'), { recursive: true })
  const staging = await mkdtemp(`${dir}.tmp-`)
  try {
    const archivePath = join(staging, asset.file)
    await Bun.write(archivePath, archive)
    const tar = Bun.spawnSync(['tar', '-xf', archivePath, '-C', staging, exe])
    if (tar.exitCode !== 0) throw new Error(`tar failed: ${tar.stderr.toString()}`)
    await rm(archivePath)
    // A concurrent install may have won the rename; its binary passed the same check.
    await rename(staging, dir).catch(() => {})
  } finally {
    await rm(staging, { recursive: true, force: true })
  }
  return true
}

if (process.env.CI === 'true' || process.env.HK === '0') process.exit(0)
if (git('rev-parse', '--git-dir').exitCode !== 0) process.exit(0)

const dir = join(root, 'node_modules', '.cache', 'hk', VERSION)
const exe = process.platform === 'win32' ? 'hk.exe' : 'hk'
if (!existsSync(join(dir, exe)) && !(await installBinary(dir, exe))) process.exit(0)

// husky pointed Git at .husky/_, which would hide the hooks hk writes to .git/hooks.
if (git('config', '--get', 'core.hooksPath').stdout.toString().trim() === '.husky/_') {
  git('config', '--unset', 'core.hooksPath')
}

const install = Bun.spawnSync([join(dir, exe), 'install'], {
  cwd: root,
  stdio: ['inherit', 'inherit', 'inherit']
})
if (install.exitCode !== 0) log('hk install failed; Git hooks not installed')
