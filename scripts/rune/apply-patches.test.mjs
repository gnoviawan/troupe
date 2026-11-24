import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import test from 'node:test'
import { build, run, series } from './apply-patches.mjs'

const git = (cwd, ...args) =>
  execFileSync('git', ['-c', 'user.name=t', '-c', 'user.email=t@t', ...args], {
    cwd,
    encoding: 'utf8'
  }).trim()

// A one-commit "Rune" repo, its rev, and a patch made against that rev.
function upstream() {
  const dir = mkdtempSync(join(tmpdir(), 'rune-upstream-'))
  git(dir, 'init', '--quiet', '-b', 'dev')
  writeFileSync(join(dir, 'lib.rs'), 'fn one() {}\n')
  git(dir, 'add', '.')
  git(dir, 'commit', '--quiet', '-m', 'base')
  const rev = git(dir, 'rev-parse', 'HEAD')
  git(dir, 'checkout', '--quiet', '-b', 'fix')
  writeFileSync(join(dir, 'lib.rs'), 'fn two() {}\n')
  git(dir, 'commit', '--quiet', '-am', 'rename one to two')
  const patch = git(dir, 'format-patch', '--stdout', 'dev')
  git(dir, 'checkout', '--quiet', 'dev')
  return { dir, rev, patch: `${patch}\n` }
}

function troupe(rev, patches) {
  const root = mkdtempSync(join(tmpdir(), 'rune-troupe-'))
  const patchDir = join(root, 'apps/rune/patches')
  mkdirSync(patchDir, { recursive: true })
  writeFileSync(join(root, 'apps/rune/rune.pin'), `${rev}\n`)
  writeFileSync(join(patchDir, 'README.md'), '# patches\n')
  for (const [name, text] of Object.entries(patches)) writeFileSync(join(patchDir, name), text)
  return root
}

test('passes without touching the source when there are no patches', () => {
  const root = troupe('9d4b4636f1f7e6d172053a0b6469a334c0bfc227', {})
  assert.equal(run({ root, source: '/nonexistent/rune' }), 0)
})

test('series lists numbered patches in order and skips other files', () => {
  const root = troupe('0'.repeat(40), { '0002-b.patch': '', '0001-a.patch': '', 'notes.txt': '' })
  assert.deepEqual(series(join(root, 'apps/rune/patches')), ['0001-a.patch', '0002-b.patch'])
})

test('builds the troupe branch when every patch applies', () => {
  const { dir, rev, patch } = upstream()
  const root = troupe(rev, { '0001-rename.patch': patch })
  const out = join(mkdtempSync(join(tmpdir(), 'rune-out-')), 'fork')
  assert.equal(run({ root, source: dir, out }), 0)
  assert.equal(git(out, 'rev-parse', '--abbrev-ref', 'HEAD'), `troupe/${rev}`)
  assert.equal(readFileSync(join(out, 'lib.rs'), 'utf8'), 'fn two() {}\n')
})

test('exits 1 when a patch no longer applies to the pinned rev', () => {
  const { dir, rev, patch } = upstream()
  const patches = ['0001-rename.patch', '0002-rename-again.patch']
  const root = troupe(rev, { [patches[0]]: patch, [patches[1]]: patch })
  const patchDir = join(root, 'apps/rune/patches')
  const out = mkdtempSync(join(tmpdir(), 'rune-out-'))
  assert.equal(build({ source: dir, rev, patchDir, patches, out }), '0002-rename-again.patch')
  assert.equal(run({ root, source: dir }), 1)
})
