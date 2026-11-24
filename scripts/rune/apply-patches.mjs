#!/usr/bin/env node
// Applies Troupe's numbered patches (apps/rune/patches/NNNN-*.patch) on top of
// the pinned Rune rev (apps/rune/rune.pin) and exits 1 at the first patch that
// does not apply. The result is a branch `troupe/<rev>` in a scratch clone,
// ready to push to a fork; `--out DIR` keeps that clone.
//   node scripts/rune/apply-patches.mjs [--root DIR] [--source URL|PATH] [--out DIR]

import { execFileSync } from 'node:child_process'
import { mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

export const PIN_FILE = 'apps/rune/rune.pin'
export const PATCH_DIR = 'apps/rune/patches'
export const RUNE_URL = 'https://github.com/anomalyco/rune.git'

const PATCH_NAME = /^\d{4}-.+\.patch$/

// The series in apply order. Files that are not `NNNN-name.patch` are not patches
// (the README, for one).
export function series(patchDir) {
  return readdirSync(patchDir)
    .filter((name) => PATCH_NAME.test(name))
    .sort()
}

const git = (cwd, args) =>
  execFileSync(
    'git',
    ['-c', 'user.name=troupe-patches', '-c', 'user.email=troupe-patches@localhost', ...args],
    { cwd, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }
  )

// Builds `troupe/<rev>` in `out` from `source` at `rev` plus every patch, in
// order. Returns the name of the first patch that fails, or null.
export function build({ source, rev, patchDir, patches, out }) {
  mkdirSync(out, { recursive: true })
  git(out, ['init', '--quiet'])
  git(out, ['fetch', '--quiet', '--depth', '1', source, rev])
  git(out, ['checkout', '--quiet', '-b', `troupe/${rev}`, 'FETCH_HEAD'])
  for (const name of patches) {
    try {
      git(out, ['am', '--quiet', join(resolve(patchDir), name)])
    } catch (error) {
      console.error(`apply-patches: ${name} does not apply to ${rev}\n${error.stderr ?? ''}`)
      git(out, ['am', '--abort'])
      return name
    }
    console.log(`apply-patches: applied ${name}`)
  }
  return null
}

export function run({ root, source = RUNE_URL, out }) {
  const rev = readFileSync(join(root, PIN_FILE), 'utf8').trim()
  const patchDir = join(root, PATCH_DIR)
  const patches = series(patchDir)
  if (patches.length === 0) {
    console.log(`apply-patches: no patches in ${PATCH_DIR}; Rune ${rev} is used as pinned`)
    return 0
  }
  const dir = out ?? mkdtempSync(join(tmpdir(), 'rune-patches-'))
  try {
    if (build({ source, rev, patchDir, patches, out: dir })) return 1
    console.log(`apply-patches: ${patches.length} patches apply to ${rev}; branch troupe/${rev}`)
    return 0
  } finally {
    if (!out) rmSync(dir, { recursive: true, force: true })
  }
}

if (process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
  const flag = (name) => {
    const i = process.argv.indexOf(name)
    return i > -1 ? process.argv[i + 1] : undefined
  }
  const root = flag('--root') ?? join(dirname(fileURLToPath(import.meta.url)), '../..')
  process.exit(run({ root, source: flag('--source'), out: flag('--out') }))
}
