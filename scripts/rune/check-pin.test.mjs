import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { mkdirSync, mkdtempSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import test from 'node:test'
import { check } from './check-pin.mjs'

const PIN = '9d4b4636f1f7e6d172053a0b6469a334c0bfc227'
const OTHER = '0123456789abcdef0123456789abcdef01234567'
const SCRIPT = new URL('./check-pin.mjs', import.meta.url).pathname

const toml = (runeRev, patchRevs = {}) =>
  `
[workspace]
members = ["crates/hello"]

[workspace.dependencies]
rune = { git = "https://github.com/anomalyco/rune.git", rev = "${runeRev}" }

[patch.crates-io]
winit = { git = "https://github.com/anomalyco/rune.git", rev = "${patchRevs.winit ?? PIN}" }
sctk-adwaita = { git = "https://github.com/anomalyco/rune.git", rev = "${patchRevs['sctk-adwaita'] ?? PIN}" }
parley_engine = { git = "https://github.com/anomalyco/rune.git", rev = "${patchRevs.parley_engine ?? PIN}" }
wgpu-hal = { git = "https://github.com/anomalyco/rune.git", rev = "${patchRevs['wgpu-hal'] ?? PIN}" }
accesskit_windows = { git = "https://github.com/anomalyco/rune.git", rev = "${patchRevs.accesskit_windows ?? PIN}" }
accesskit_macos = { git = "https://github.com/anomalyco/rune.git", rev = "${patchRevs.accesskit_macos ?? PIN}" }
`

const fixture = (tomlText, pin) => {
  const dir = mkdtempSync(join(tmpdir(), 'check-pin-'))
  mkdirSync(join(dir, 'apps/rune'), { recursive: true })
  writeFileSync(join(dir, 'apps/rune/Cargo.toml'), tomlText)
  writeFileSync(join(dir, 'apps/rune/rune.pin'), `${pin}\n`)
  return dir
}

test('passes when the dependency and all six patch revs equal rune.pin', () => {
  assert.deepEqual(check(toml(PIN), PIN), [])
})

test('allows non-Rune [patch.crates-io] entries such as FigCraft egui forks', () => {
  const withEgui = `${toml(PIN)}egui = { git = "https://github.com/storytold/egui.git", rev = "${OTHER}" }\n`
  assert.deepEqual(check(withEgui, PIN), [])
})

test('fails when one patch rev differs', () => {
  const errors = check(toml(PIN, { 'sctk-adwaita': OTHER }), PIN)
  assert.equal(errors.length, 1)
  assert.match(errors[0], /sctk-adwaita/)
})

test('fails when the rune dependency rev differs', () => {
  const errors = check(toml(OTHER), PIN)
  assert.equal(errors.length, 1)
  assert.match(errors[0], /rune rev/)
})

test('fails when a patch entry is missing', () => {
  const errors = check(toml(PIN).replace(/^accesskit_macos = .*$/m, ''), PIN)
  assert.deepEqual(errors, ['missing [patch.crates-io] entry accesskit_macos'])
})

test('the script exits 0 on a matching tree and 1 on a differing patch rev', () => {
  execFileSync(process.execPath, [SCRIPT, '--root', fixture(toml(PIN), PIN)], { stdio: 'pipe' })
  assert.throws(
    () =>
      execFileSync(
        process.execPath,
        [SCRIPT, '--root', fixture(toml(PIN, { winit: OTHER }), PIN)],
        { stdio: 'pipe' }
      ),
    (error) => error.status === 1
  )
})
