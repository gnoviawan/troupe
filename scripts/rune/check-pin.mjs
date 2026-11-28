#!/usr/bin/env node
// Checks that apps/rune pins one Rune rev: the `rune` dependency's rev, every
// [patch.crates-io] entry's rev and `apps/rune/rune.pin` must all be the same
// 40-character commit. Prints each disagreement and exits 1 when they differ.
// `node scripts/rune/check-pin.mjs [--root DIR]` (default root: the repo).

import { readFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

export const PIN_FILE = 'apps/rune/rune.pin'
export const WORKSPACE_TOML = 'apps/rune/Cargo.toml'
export const EXPECTED_PATCHES = [
  'winit',
  'sctk-adwaita',
  'parley_engine',
  'wgpu-hal',
  'accesskit_windows',
  'accesskit_macos'
]

const REV = /rev\s*=\s*"([0-9a-f]{40})"/
const GIT = /git\s*=\s*"([^"]+)"/
const TABLE = /^\[\s*([^\]]+?)\s*\]/
const ENTRY = /^([A-Za-z0-9_-]+)\s*=/
const RUNE_GIT = /github\.com\/anomalyco\/rune(?:\.git)?$/

// Every `{ ... rev = "..." }` dependency entry in the TOML text, tagged with the
// table it sits under. Dep lines in this repo are single-line; a `rev` inside a
// comment never reaches here because comments are stripped first.
export function entries(toml) {
  const out = []
  let table = ''
  for (const raw of toml.split('\n')) {
    const line = raw.replace(/#.*$/, '').trim()
    if (!line) continue
    const header = line.match(TABLE)
    if (header) {
      table = header[1].trim()
      continue
    }
    const name = line.match(ENTRY)
    const rev = line.match(REV)
    const git = line.match(GIT)
    if (name && rev) out.push({ table, name: name[1], rev: rev[1], git: git ? git[1] : null })
  }
  return out
}

// The disagreements between a workspace Cargo.toml and the pinned rev.
export function check(toml, pin) {
  const errors = []
  const all = entries(toml)
  const rune = all.filter((e) => e.name === 'rune' && e.table.endsWith('dependencies'))
  if (rune.length === 0) errors.push('no `rune` dependency with a rev found')
  for (const e of rune) {
    if (e.rev !== pin) errors.push(`[${e.table}] rune rev ${e.rev} != rune.pin ${pin}`)
  }
  const patches = all.filter((e) => e.table === 'patch.crates-io')
  for (const name of EXPECTED_PATCHES) {
    const entry = patches.find((p) => p.name === name)
    if (!entry) errors.push(`missing [patch.crates-io] entry ${name}`)
  }
  for (const p of patches) {
    const isRunePatch = EXPECTED_PATCHES.includes(p.name) || p.git === null || RUNE_GIT.test(p.git)
    if (isRunePatch && p.rev !== pin) {
      errors.push(`[patch.crates-io] ${p.name} rev ${p.rev} != rune.pin ${pin}`)
    }
  }
  return errors
}

// Reads the workspace files under `root` and returns the process exit code.
export function run(root) {
  let toml
  let pin
  try {
    toml = readFileSync(join(root, WORKSPACE_TOML), 'utf8')
    pin = readFileSync(join(root, PIN_FILE), 'utf8').trim()
  } catch (error) {
    console.error(`check-pin: ${error.message}`)
    return 1
  }
  if (!/^[0-9a-f]{40}$/.test(pin)) {
    console.error(`check-pin: ${PIN_FILE} holds ${JSON.stringify(pin)}, not a 40-char rev`)
    return 1
  }
  const errors = check(toml, pin)
  for (const error of errors) console.error(`check-pin: ${error}`)
  if (errors.length) return 1
  console.log(
    `check-pin: rune pinned at ${pin} (dependency and ${EXPECTED_PATCHES.length} patches agree)`
  )
  return 0
}

if (process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
  const flag = process.argv.indexOf('--root')
  const root =
    flag > -1 ? process.argv[flag + 1] : join(dirname(fileURLToPath(import.meta.url)), '../..')
  process.exit(run(root))
}
