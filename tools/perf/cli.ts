/**
 * tools/perf CLI (entry point) — `bun tools/perf/cli.ts <command> [flags]`
 * (wired as `bun run perf -- <command>`).
 *
 * Commands:
 *   stream-storm   K fake agents streaming sessionUpdates (CAP-1/2)
 *   project-switch switch cycles under live load
 *   pty-flood      canned PTY output flood
 *   soak           long-duration stream + periodic switches (ask-first)
 *   compare        two clients interleaved (or two result dirs → deltas)
 *   browser        Linux client lane on Xvfb (launch, stream-storm,
 *                  pty-flood, soak, idle, project-switch)
 *   run            client comparison engine, direct flags
 *   webkit-startup Tauri/WebKitGTK desktop startup + idle reference
 *   self-test      fake-agent determinism + report smoke, no app needed
 *
 * Help prints usage without launching anything (acceptance: `--help` works
 * with no app, no exe, no CDP).
 */

import { writeFileSync } from 'node:fs'
import path from 'node:path'
import { runBrowserScenario } from './browser/browser-lane.ts'
import { driveProjectSwitchBrowser } from './browser/browser-scenarios.ts'
import { SCENARIOS } from './client-scenarios.ts'
import { selfTest } from './fake-agent/fake-agent.ts'
import { compareRuns, formatCompare } from './report/compare.ts'
import { renderReport } from './report/html-report.ts'
import { parseRunArgs, runClientComparison, toPerfRunResult } from './run.ts'
import { runScenario, type ScenarioDef } from './runner/scenario-harness.ts'
import { formatRunsReport, loadRunSet } from './runs-report.ts'
import { projectSwitchScenario } from './scenarios/project-switch.ts'
import { ptyFloodScenario } from './scenarios/pty-flood.ts'
import { soakScenario } from './scenarios/soak.ts'
import { streamStormScenario } from './scenarios/stream-storm.ts'
import { type PerfRunResult, summarize } from './types.ts'
import { webkitStartup } from './webkit-startup.ts'

const USAGE = `Troupe perf toolkit

Usage: node tools/perf/cli.ts <command> [flags]

Commands:
  stream-storm    K fake ACP agents streaming sessionUpdates at rate/s.
                  --agents 4 --sessions-per-agent 2 --rate 20 --seed 42
                  --warmup 10 --sustain 30 --measure 20 --switch-every-ms 5000
  project-switch  Repeated project switches under live load.
                  --projects 3 --cycles 10 --ui (click the sidebar; default: store action)
  pty-flood       T terminals running the canned flood script.
                  --terminals 3 --lines-per-sec 200 --resize-every 200 --with-agent-stream
  soak            Long-duration stream-storm with periodic switches (>15 min;
                  requires --yes). --minutes 30 --switch-every-sec 120
  compare         Two clients interleaved N runs → medians, p95, rank-sum p:
                  --clients chromium:a,chromium:b --scenarios launch --runs 3
                  Or two existing result dirs → per-metric deltas + verdicts:
                  --base tools/perf/results/<id> --against tools/perf/results/<id2>
  browser         Linux client lane: headed Chromium on Xvfb against a seeded
                  troupe-server. Scenarios: launch, idle, stream-storm,
                  pty-flood, soak (project-switch stays headless).
                  --server <url> attaches a running server; --server-bin <path>
                  spawns one per run. Scenario flags pass through (--rate,
                  --terminals, --settle, --measure, ...).
  run             The client engine directly: --clients <spec> --scenarios <list>
                  --runs 3 --display :181 --port 8196 --out <dir>
                  Clients: chromium (the React client), rune-web (a Rune wasm
                  build in Chromium) and rune-desktop (a native Rune window).
                  The Rune clients open --rune-app hello (default; gallery runs
                  on rune-web only) and run the launch and idle scenarios until
                  Rune has Troupe screens.
  webkit-startup  Startup + idle of the shipped Tauri/WebKitGTK binary.
                  --bin <troupe> --runs 3 --display :182
  self-test       Fake-agent determinism + report/compare smoke. No app.

Shared flags:
  --exe <path>    App exe (default: worktree release build, then main checkout)
  --seed 42       Determinism seed (same seed → identical sessionUpdate stream)
  --keep-state    Keep the per-run scratch user-data dir after teardown

Environment:
  TERMUL_PERF_PROFILING=1   Build with the react-dom/profiling alias
                            (vite.config.tauri.ts gate) for release-grade commit timing.
  PERF_AGENT_TRACE=1        Fake agent logs each emitted update (stderr JSONL).
`

async function runCompare(argv: string[]): Promise<number> {
  if (argv.includes('--clients')) {
    // Live comparison: two clients interleaved over N runs → medians, p95
    // and a two-sided rank-sum p per key (runs-report).
    const opts = parseRunArgs(argv)
    if (opts.clients.length < 2) {
      console.error('compare --clients needs two client specs, e.g. chromium:a,chromium:b')
      return 1
    }
    const set = await runClientComparison(opts)
    const report = formatRunsReport(loadRunSet(opts.outDir))
    console.log(report)
    writeFileSync(path.join(set.outDir, 'runs-report.md'), report)
    console.log(`[perf] wrote ${path.join(set.outDir, 'runs-report.md')}`)
    return 0
  }
  let base: string | undefined
  let against: string | undefined
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === '--base') base = argv[i + 1]
    if (argv[i] === '--against') against = argv[i + 1]
  }
  if (!base || !against) {
    console.error('compare requires --clients <a>,<b> or --base <dir> and --against <dir>')
    console.error(USAGE)
    return 1
  }
  const report = compareRuns(base, against)
  console.log(formatCompare(report))
  return 0
}

async function runBrowser(argv: string[]): Promise<number> {
  let scenario: string | undefined
  let serverUrl: string | undefined
  const rest: string[] = []
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === '--server') {
      serverUrl = argv[i + 1]
      i++
      continue
    }
    if (!argv[i].startsWith('--') && scenario === undefined) {
      scenario = argv[i]
      continue
    }
    rest.push(argv[i])
  }
  // The headless CDP lane, kept for project-switch (it needs two seeded
  // projects and measures store-switch latency, not a client under X).
  if (scenario === 'project-switch') {
    const flags = parseFlagsInto(rest, projectSwitchScenario.defaults)
    await runBrowserScenario({ scenario, flags, serverUrl }, driveProjectSwitchBrowser)
    return 0
  }
  if (!scenario || !(scenario in SCENARIOS)) {
    console.error(`browser lane supports: ${Object.keys(SCENARIOS).join(', ')}, project-switch`)
    return 1
  }
  // Client-lane scenarios run through the shared engine: Xvfb, seeded
  // troupe-server (spawned per run, or --server attaches one), byte proxy,
  // proc/thread/page/screen metrics.
  const runArgv = ['--scenarios', scenario, '--clients', 'chromium', '--runs', '1']
  if (serverUrl) runArgv.push('--server', serverUrl)
  runArgv.push(...rest)
  const opts = parseRunArgs(runArgv)
  const set = await runClientComparison(opts)
  const record = set.records[set.records.length - 1]
  if (record) {
    // The browser lane's result.json + report.html, in the classic schema.
    const result = toPerfRunResult(record, serverUrl ?? opts.serverBin ?? '')
    writeFileSync(path.join(set.outDir, 'result.json'), JSON.stringify(result))
    writeFileSync(path.join(set.outDir, 'report.html'), renderReport(result))
    console.log(`[perf] wrote ${path.join(set.outDir, 'result.json')} + report.html`)
  }
  return 0
}

function parseFlagsInto(
  argv: string[],
  defaults: Record<string, string | number | boolean>
): Record<string, string | number | boolean> {
  const flags: Record<string, string | number | boolean> = { ...defaults }
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i]
    if (!arg.startsWith('--')) continue
    const key = arg.slice(2)
    const next = argv[i + 1]
    if (next === undefined || next.startsWith('--')) {
      flags[key] = true
    } else {
      const asNum = Number(next)
      const value: string | number = Number.isFinite(asNum) ? asNum : next
      flags[key] = value
      i++
    }
  }
  return flags
}

/** Report/compare smoke: synthesize a tiny result, render, summarize, compare. */
async function runSelfTest(): Promise<number> {
  selfTest()
  const result: PerfRunResult = {
    meta: {
      runId: 'selftest',
      scenario: 'selftest',
      startedAt: new Date().toISOString(),
      params: {},
      seed: 1,
      lane: 'desktop',
      appTarget: 'selftest',
      reactProfiling: false,
      status: 'ok'
    },
    phases: [
      {
        name: 'measure',
        startedAtMs: 0,
        durationMs: 1000,
        page: {
          loaf: [
            {
              t: 10,
              duration: 80,
              blockingDuration: 60,
              scripts: [{ name: 'test.js', duration: 50 }]
            }
          ],
          longtasks: [{ t: 20, duration: 50, name: 'task' }],
          interactions: [{ t: 30, duration: 40, name: 'click' }],
          frames: [
            { t: 0, deltaMs: 16 },
            { t: 16, deltaMs: 33 }
          ],
          heapUsed: [
            { t: 0, value: 50_000_000 },
            { t: 500, value: 60_000_000 },
            { t: 1000, value: 70_000_000 }
          ],
          heapTotal: [],
          cdp: [{ t: 0, metrics: { Nodes: 900, LayoutCount: 30, JSHeapUsedSize: 55_000_000 } }]
        },
        react: {
          components: [
            { component: 'ChatMessage', commits: 40, totalDurationMs: 120, maxDurationMs: 8 }
          ],
          totalCommits: 40,
          profilingActive: true
        },
        gaps: []
      }
    ],
    summary: {}
  }
  result.summary = summarize(result)
  const html = renderReport(result)
  if (!html.includes('Top components') && !html.includes('ChatMessage')) {
    throw new Error('self-test: report missing component attribution')
  }
  if (!html.includes('Top LoAF scripts')) {
    throw new Error('self-test: report missing LoAF attribution')
  }
  if (result.summary.heapSlopeBytesPerSec <= 0) {
    throw new Error('self-test: heap slope regression failed')
  }
  console.log('report + summarize self-test ok (heap slope, attribution sections present)')
  return 0
}

async function main(): Promise<number> {
  const argv = process.argv.slice(2)
  if (argv.length === 0 || argv[0] === '--help' || argv[0] === 'help' || argv[0] === '-h') {
    console.log(USAGE)
    return 0
  }
  const command = argv[0]
  const rest = argv.slice(1)

  switch (command) {
    case 'stream-storm':
      return await runScenarioExit(streamStormScenario, rest)
    case 'project-switch':
      return await runScenarioExit(projectSwitchScenario, rest)
    case 'pty-flood':
      return await runScenarioExit(ptyFloodScenario, rest)
    case 'soak': {
      // Ask-first: the soak is long. Require explicit consent.
      const minutes = minutesOf(rest)
      if (minutes >= 15 && !rest.includes('--yes')) {
        console.error(
          `soak for ${minutes} minutes is a long run. Re-run with --yes to confirm, or pass --minutes <n> for a shorter smoke.`
        )
        return 1
      }
      return await runScenarioExit(soakScenario, rest)
    }
    case 'compare':
      return await runCompare(rest)
    case 'browser':
      return await runBrowser(rest)
    case 'run':
      await runClientComparison(parseRunArgs(rest))
      return 0
    case 'webkit-startup':
      await webkitStartup(rest)
      return 0
    case 'self-test':
      return await runSelfTest()
    default:
      console.error(`unknown command: ${command}`)
      console.error(USAGE)
      return 1
  }
}

function minutesOf(argv: string[]): number {
  const idx = argv.indexOf('--minutes')
  if (idx >= 0 && argv[idx + 1] !== undefined) {
    const n = Number(argv[idx + 1])
    if (Number.isFinite(n)) return n
  }
  return 30
}

async function runScenarioExit(scenario: ScenarioDef, argv: string[]): Promise<number> {
  try {
    await runScenario({ scenario, argv })
    return 0
  } catch (err) {
    console.error(String(err))
    return 1
  }
}

const exitCode = await main()
process.exit(exitCode)
