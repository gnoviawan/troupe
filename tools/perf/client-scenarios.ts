import { existsSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import path from 'node:path'
import { setTimeout as sleep } from 'node:timers/promises'
import { fileURLToPath } from 'node:url'
import type { ClientDriver } from './clients/client.ts'

export type ScenarioName = 'launch' | 'idle' | 'stream-storm' | 'pty-flood' | 'soak'
export type Flags = Record<string, number>

/** One measured window. The runner wraps it with proc, byte and page capture. */
export interface PhaseClock {
  phase(name: string, body: () => Promise<void>): Promise<void>
  note(key: string, value: unknown): void
  error(message: string): void
}

export interface ScenarioEnv {
  driver: ClientDriver
  clock: PhaseClock
  flags: Flags
  seed: number
  /** Scratch dir for probe result files. */
  scratch: string
}

export const DEFAULTS: Record<ScenarioName, Flags> = {
  // Boot, navigate, sit on the painted project rail + launcher. The runner's
  // startup window carries the launch timings (startup.*); the phases here
  // hold the just-painted UI's proc/page load.
  launch: { settle: 10, measure: 15 },
  // Boot, let first-run work finish, then sit on the launcher with no input.
  idle: { settle: 20, measure: 20 },
  // Same shape as tools/perf stream-storm: agents × sessions-per-agent chats.
  'stream-storm': {
    agents: 4,
    'sessions-per-agent': 2,
    rate: 20,
    warmup: 10,
    sustain: 30,
    measure: 20,
    'switch-every-ms': 5000
  },
  // Same shape as tools/perf pty-flood: T terminals at lines-per-sec each.
  'pty-flood': {
    terminals: 3,
    'lines-per-sec': 200,
    warmup: 5,
    sustain: 30,
    measure: 15,
    'burst-lines': 20000
  },
  // stream-storm load held for a long window with periodic chat switches
  // (the desktop soak's shape). Long: pass --minutes for a short smoke.
  soak: {
    agents: 2,
    'sessions-per-agent': 1,
    rate: 10,
    warmup: 10,
    minutes: 30,
    'switch-every-sec': 120,
    measure: 30
  }
}

/**
 * Navigation to a painted project rail, then a short measure. The launch
 * numbers themselves live in the `startup.*` metrics the runner records
 * before the scenario body runs.
 */
async function launch({ clock, flags }: ScenarioEnv): Promise<void> {
  await clock.phase('settle', () => sleep(flags.settle * 1000))
  await clock.phase('measure', () => sleep(flags.measure * 1000))
}

async function idle({ clock, flags }: ScenarioEnv): Promise<void> {
  await clock.phase('settle', () => sleep(flags.settle * 1000))
  await clock.phase('measure', () => sleep(flags.measure * 1000))
}

async function streamStorm({ driver, clock, flags }: ScenarioEnv): Promise<void> {
  const chats = flags.agents * flags['sessions-per-agent']
  const startMs: number[] = []
  const start = async (prompt: string) => {
    const t0 = performance.now()
    await driver.startChat(prompt)
    startMs.push(Math.round(performance.now() - t0))
  }
  await clock.phase('warmup', async () => {
    await start('perf warmup turn')
    await sleep(flags.warmup * 1000)
  })
  await clock.phase('open-chats', async () => {
    for (let i = 0; i < chats - 1; i++) {
      await start(`perf stream ${i}`).catch((err) => clock.error(`chat ${i}: ${err}`))
    }
  })
  clock.note('chatStartMs', startMs)
  await clock.phase('sustained-stream', async () => {
    const deadline = Date.now() + flags.sustain * 1000
    let cursor = 0
    while (Date.now() < deadline) {
      await sleep(Math.min(flags['switch-every-ms'], Math.max(0, deadline - Date.now())))
      if (Date.now() >= deadline) break
      await driver.switchToChat(cursor++).catch((err) => clock.error(`switch: ${err}`))
    }
    clock.note('switches', cursor)
  })
  clock.note('chatsOpen', await driver.chatCount())
  await clock.phase('measure', () => sleep(flags.measure * 1000))
}

const PROBE = path.join(path.dirname(fileURLToPath(import.meta.url)), 'terminal-probe.ts')

/**
 * Writes the probe invocation to a script and returns the short command that
 * runs it. Typing into a terminal under flood is slow, and the harness should
 * not spend the measured window typing a 200-character command line.
 */
function probeCommand(script: string, env: Record<string, string | number>): string {
  const vars = Object.entries(env).map(([k, v]) => `PERF_PROBE_${k}=${v}`)
  writeFileSync(script, `${vars.join(' ')} exec bun ${PROBE}\n`)
  return `sh ${script}`
}

/**
 * Finished = JSON content; the probe writes a bare `started` marker first so
 * the caller can tell "probe running" from "the typed command never ran".
 */
async function waitForFile(file: string, timeoutMs: number): Promise<unknown> {
  const deadline = Date.now() + timeoutMs
  while (Date.now() < deadline) {
    if (existsSync(file)) {
      const text = readFileSync(file, 'utf8')
      if (text.startsWith('{')) return JSON.parse(text)
    }
    await sleep(200)
  }
  throw new Error(`probe result ${file} never appeared`)
}

async function ptyFlood({ driver, clock, flags, seed, scratch }: ScenarioEnv): Promise<void> {
  const terminals = flags.terminals
  const pacedSeconds = flags.warmup + flags.sustain + flags.measure + 3 * terminals
  const outs: string[] = []
  const startPaced = async (i: number) => {
    const out = path.join(scratch, `paced-${i}.json`)
    rmSync(out, { force: true })
    outs.push(out)
    await driver.openTerminal()
    const cmd = probeCommand(path.join(scratch, `p${i}.sh`), {
      MODE: 'paced',
      RATE: flags['lines-per-sec'],
      SECONDS: pacedSeconds,
      SEED: seed + i,
      OUT: out
    })
    // Keystrokes typed before the terminal's WS session attaches are dropped
    // silently; retype until the probe's `started` marker says it ran.
    for (let attempt = 0; attempt < 4 && !existsSync(out); attempt++) {
      await driver.runInTerminal(cmd)
      for (let j = 0; j < 40 && !existsSync(out); j++) await sleep(300)
    }
    if (!existsSync(out)) throw new Error(`terminal ${i} never started the probe`)
  }
  await clock.phase('warmup', async () => {
    await startPaced(0)
    await sleep(flags.warmup * 1000)
  })
  await clock.phase('flood', async () => {
    for (let i = 1; i < terminals; i++) await startPaced(i)
    await sleep(flags.sustain * 1000)
  })
  await clock.phase('measure', () => sleep(flags.measure * 1000))
  const paced = await Promise.all(outs.map((o) => waitForFile(o, (pacedSeconds + 30) * 1000)))
  clock.note('paced', paced)

  const burstOut = path.join(scratch, 'burst.json')
  rmSync(burstOut, { force: true })
  await clock.phase('burst', async () => {
    const cmd = probeCommand(path.join(scratch, 'b.sh'), {
      MODE: 'burst',
      LINES: flags['burst-lines'],
      SEED: seed,
      OUT: burstOut
    })
    for (let attempt = 0; attempt < 4 && !existsSync(burstOut); attempt++) {
      await driver.runInTerminal(cmd)
      for (let j = 0; j < 40 && !existsSync(burstOut); j++) await sleep(300)
    }
    if (!existsSync(burstOut)) throw new Error('the burst probe never started')
    clock.note('burst', await waitForFile(burstOut, 180_000))
  })
}

/**
 * stream-storm load held for `minutes` with a chat switch every
 * `switch-every-sec` — the browser-lane shape of the desktop `soak`.
 */
async function soak({ driver, clock, flags }: ScenarioEnv): Promise<void> {
  const chats = Math.max(1, flags.agents * flags['sessions-per-agent'])
  await clock.phase('warmup', async () => {
    for (let i = 0; i < chats; i++) {
      await driver.startChat(`perf soak ${i}`).catch((err) => clock.error(`chat ${i}: ${err}`))
    }
    await sleep(flags.warmup * 1000)
  })
  await clock.phase('soak', async () => {
    const deadline = Date.now() + flags.minutes * 60 * 1000
    let nextSwitch = Date.now() + flags['switch-every-sec'] * 1000
    let switches = 0
    while (Date.now() < deadline) {
      await sleep(Math.min(1000, Math.max(0, deadline - Date.now())))
      if (Date.now() >= nextSwitch) {
        await driver.switchToChat(switches++).catch((err) => clock.error(`switch: ${err}`))
        nextSwitch = Date.now() + flags['switch-every-sec'] * 1000
      }
    }
    clock.note('switches', switches)
  })
  clock.note('chatsOpen', await driver.chatCount())
  await clock.phase('measure', () => sleep(flags.measure * 1000))
}

export const SCENARIOS: Record<ScenarioName, (env: ScenarioEnv) => Promise<void>> = {
  launch,
  idle,
  'stream-storm': streamStorm,
  'pty-flood': ptyFlood,
  soak
}
