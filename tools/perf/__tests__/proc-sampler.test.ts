import { execFileSync } from 'node:child_process'
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import path from 'node:path'
import { afterAll, describe, expect, it } from 'vitest'
import {
  chromiumRole,
  descendants,
  parseProcStat,
  parseSmapsRollup,
  parseStatm,
  readCmdline,
  startProcSampler
} from '../sampling/proc-sampler.ts'

/**
 * A fixture /proc root: a fake chromium tree (200 root browser + renderer +
 * gpu children) and an unrelated troupe-server (100, child of init).
 *   200 chromium
 *   201 chromium --type=renderer      (child of 200)
 *   202 chromium --type=gpu-process   (child of 200)
 *   100 troupe-server                 (child of 1)
 */
const ROOT = mkdtempSync(path.join(tmpdir(), 'perf-proc-'))

const PROC: Record<number, { stat: string; statm: string; smaps: string; cmdline: string }> = {
  100: {
    stat: '100 (troupe-server) S 1 100 100 0 -1 4194304 100 0 0 0 30 6 0 0 20 0 1 0 1000 0 0 0',
    statm: '8000 4000 1000 0 0 0 0',
    smaps: 'Rss:               16000 kB\nPss:               12000 kB\n',
    cmdline: ['troupe-server', '--port', '8190'].join('\0')
  },
  200: {
    stat: '200 (chromium) S 50 200 200 0 -1 4194304 100 0 0 0 20 4 0 0 20 0 5 0 2000 0 0 0',
    statm: '4000 2560 512 0 0 0 0',
    smaps: 'Rss:               2048 kB\nPss:               1536 kB\n',
    cmdline: ['chromium', '--enable-features'].join('\0')
  },
  201: {
    stat: '201 (Chromium) S 200 200 200 0 -1 4194304 50 0 0 0 8 2 0 0 20 0 8 0 2001 0 0 0',
    statm: '6000 5120 256 0 0 0 0',
    smaps: 'Rss:               4096 kB\nPss:               3072 kB\n',
    cmdline: ['Chromium', '--type=renderer', '--foo'].join('\0')
  },
  202: {
    stat: '202 (chromium) S 200 200 200 0 -1 4194304 60 0 0 0 4 1 0 0 20 0 6 0 2002 0 0 0',
    statm: '2000 1024 256 0 0 0 0',
    smaps: 'Rss:               1024 kB\nPss:                768 kB\n',
    cmdline: ['chromium', '--type=gpu-process'].join('\0')
  }
}

for (const [pid, files] of Object.entries(PROC)) {
  const dir = path.join(ROOT, pid)
  mkdirSync(dir, { recursive: true })
  writeFileSync(path.join(dir, 'stat'), files.stat)
  writeFileSync(path.join(dir, 'statm'), files.statm)
  writeFileSync(path.join(dir, 'smaps_rollup'), files.smaps)
  writeFileSync(path.join(dir, 'cmdline'), files.cmdline)
}
// Not a pid directory; the walker must skip it.
mkdirSync(path.join(ROOT, 'self'), { recursive: true })

afterAll(() => rmSync(ROOT, { recursive: true, force: true }))

const PAGE = Number(execFileSync('getconf', ['PAGESIZE']).toString().trim()) || 4096

describe('/proc fixture parsing', () => {
  it('parseProcStat reads ppid and utime+stime past the comm field', () => {
    // comm is "(Chromium)" — parsing past ')' keeps real-world names safe.
    expect(parseProcStat(PROC[200].stat)).toEqual({ ppid: 50, ticks: 24 })
    expect(parseProcStat(PROC[201].stat)).toEqual({ ppid: 200, ticks: 10 })
    expect(parseProcStat('1 (a b) S 0 1 1 0 -1 1 0 0 0 0 1 2 0 0 0 0 1 0 0 0 0 0')).toEqual({
      ppid: 0,
      ticks: 3
    })
  })

  it('parseStatm converts resident pages to bytes', () => {
    expect(parseStatm(PROC[200].statm)).toBe(2560 * PAGE)
  })

  it('parseSmapsRollup reads Pss in bytes', () => {
    expect(parseSmapsRollup(PROC[201].smaps)).toBe(3072 * 1024)
    expect(parseSmapsRollup('Rss: 10 kB\n')).toBe(0)
  })

  it('descendants walks the fixture process tree by ppid', () => {
    expect(descendants(200, ROOT).sort()).toEqual([200, 201, 202])
    expect(descendants(100, ROOT)).toEqual([100])
  })

  it('readCmdline flattens NUL-separated argv', () => {
    expect(readCmdline(201, ROOT)).toBe('Chromium --type=renderer --foo')
  })

  it('startProcSampler aggregates rss/pss by chromiumRole', async () => {
    const sampler = startProcSampler(200, chromiumRole, 10, ROOT)
    await new Promise((r) => setTimeout(r, 30))
    const samples = sampler.stop()
    expect(samples.length).toBeGreaterThanOrEqual(1)
    const last = samples[samples.length - 1]
    expect(last.total.count).toBe(3)
    expect(last.total.rss).toBe((2560 + 5120 + 1024) * PAGE)
    expect(last.total.pss).toBe((1536 + 3072 + 768) * 1024)
    expect(last.roles.renderer.rss).toBe(5120 * PAGE)
    expect(last.roles['gpu-process'].pss).toBe(768 * 1024)
    expect(last.roles.browser.count).toBe(1)
    // The fixture's ticks never move: CPU is 0 for every pid.
    expect(last.total.cpu).toBe(0)
    const roles = Object.fromEntries(sampler.processes().map((p) => [p.pid, p.role]))
    expect(roles).toEqual({ 200: 'browser', 201: 'renderer', 202: 'gpu-process' })
  })
})
