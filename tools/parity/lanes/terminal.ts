/**
 * The R7a terminal lane: each client opens a terminal from the tool strip and runs the colour,
 * attribute and box-drawing screen, `ls --color`, `vim`, `htop` and `seq 200000`, then switches
 * to beta and back and checks the shell's process id is unchanged.
 *
 * A drain is timed the same way in both clients, from the page's clock at Enter to the last
 * animation frame before the page goes quiet for 250 ms after the marker that follows the last
 * number reached the page's `/terminal/ws` socket. The Rune web mirror carries no terminal
 * rows at the pin, so the lane checks what the socket delivered; React's checks also read the
 * xterm buffer.
 *
 *   bun tools/parity/lanes/terminal.ts --port 8197 --root /tmp/rune-r7a/lane \
 *     --server-bin <troupe-server> --dist <dist-web> [--only react|rune] [--runs 3]
 */

import path from 'node:path'
import { parseArgs } from 'node:util'
import { type Browser, chromium } from 'playwright'
import { REFERENCE_ARGS } from '../drivers/chromium'
import { runeWebArgs } from '../drivers/rune-web'
import { REPO_DIR } from '../server'
import { createWorld } from '../world'
import { type Client, inReact, inRune, type LaneOptions, shot, until, writePairs } from './kit'

const { values } = parseArgs({
  args: Bun.argv.slice(2),
  options: {
    port: { type: 'string', default: '8197' },
    root: { type: 'string', default: '/tmp/troupe-parity/terminal' },
    'server-bin': { type: 'string' },
    dist: { type: 'string', default: path.join(REPO_DIR, 'dist-web') },
    only: { type: 'string' },
    runs: { type: 'string', default: '3' }
  }
})
const options: LaneOptions = {
  root: path.resolve(values.root as string),
  phone: false,
  dist: path.resolve(values.dist as string),
  device: 'terminal'
}
const SCREEN = path.join(REPO_DIR, 'tools/parity/depth/term-screen.sh')
const QUIET_MS = 250

interface Probe {
  out: string
  marks: Record<string, number>
  frames: number[]
}

/** Records what `/terminal/ws` delivers (decoded output, its tail kept) and every frame's time. */
function instrument(): void {
  const probe: Probe = { out: '', marks: {}, frames: [] }
  ;(window as unknown as { __lane: Probe }).__lane = probe
  const decoder = new TextDecoder()
  const take = (text: string) => {
    probe.out = (probe.out + text).slice(-200_000)
    for (const m of probe.out.matchAll(/[A-Z]+-\d+-[A-Z]+/g))
      probe.marks[m[0]] ??= performance.now()
  }
  const chunks = (frame: unknown): number[][] => {
    const f = frame as { data?: unknown; chunks?: Array<{ data?: unknown }> }
    if (Array.isArray(f.data)) return [f.data as number[]]
    return (f.chunks ?? []).flatMap((c) => (Array.isArray(c.data) ? [c.data as number[]] : []))
  }
  const Native = window.WebSocket
  window.WebSocket = class extends Native {
    constructor(url: string | URL, protocols?: string | string[]) {
      super(url, protocols)
      if (!String(url).includes('/terminal/ws')) return
      this.addEventListener('message', (event: MessageEvent) => {
        if (typeof event.data !== 'string') return
        try {
          for (const bytes of chunks(JSON.parse(event.data)))
            take(decoder.decode(new Uint8Array(bytes)))
        } catch {}
      })
    }
  } as typeof WebSocket
  const raf = window.requestAnimationFrame.bind(window)
  window.requestAnimationFrame = (cb) =>
    raf((t) => {
      probe.frames.push(performance.now())
      if (probe.frames.length > 20_000) probe.frames.splice(0, 10_000)
      cb(t)
    })
}

/** React's visible rows, read from the xterm instance a component under `.xterm` holds. */
function xtermRows(): string {
  type Buf = {
    viewportY: number
    getLine(y: number): { translateToString(trim: boolean): string } | undefined
  }
  type Xterm = { rows: number; buffer: { active: Buf } }
  type Hook = { memoizedState: unknown; next: unknown }
  type Fiber = { return: Fiber | null; memoizedState: Hook | null }
  const isXterm = (v: unknown): v is Xterm =>
    typeof (v as Xterm | null)?.buffer?.active?.getLine === 'function'
  const fiberKey = (el: Element) => Object.keys(el).find((k) => k.startsWith('__reactFiber$'))
  let host = document.querySelector('.xterm')
  while (host && !fiberKey(host)) host = host.parentElement
  const key = host && fiberKey(host)
  let found: Xterm | undefined
  for (
    let f = key ? (host as unknown as Record<string, Fiber>)[key] : null;
    f && !found;
    f = f.return
  ) {
    for (let h = f.memoizedState; h && typeof h === 'object' && !found; h = h.next as Hook | null) {
      const current = (h.memoizedState as { current?: unknown } | null)?.current
      if (isXterm(current)) found = current
    }
  }
  if (!found) return ''
  const b = found.buffer.active
  const rows: string[] = []
  for (let y = b.viewportY; y < b.viewportY + found.rows; y++)
    rows.push(b.getLine(y)?.translateToString(true) ?? '')
  return rows.join('\n')
}

/** What the terminal shows in React; what the socket delivered in Rune. */
function screenText(client: Client): Promise<string> {
  if (client.name === 'react') return client.page.evaluate(xtermRows)
  return client.page.evaluate(() => (window as unknown as { __lane: Probe }).__lane.out)
}

const shows = (client: Client, want: RegExp | string, ms = 20_000) =>
  until(async () => {
    const text = await screenText(client)
    return typeof want === 'string' ? text.includes(want) : want.test(text)
  }, ms)

async function focus(client: Client): Promise<void> {
  if (client.name === 'react') await client.page.locator('.xterm-screen').first().click()
  else await client.page.mouse.click(560, 300)
}

async function line(client: Client, command: string): Promise<void> {
  await client.page.keyboard.type(command, { delay: 5 })
  await client.page.keyboard.press('Enter')
}

/**
 * Ms from `t0` to the last frame before the page went quiet after `mark` arrived, ms to the
 * mark's arrival, and the frames drawn in between, or null.
 */
async function drained(
  client: Client,
  mark: string,
  t0: number
): Promise<[number, number, number] | null> {
  let result: [number, number, number] | null = null
  await until(async () => {
    result = await client.page.evaluate(
      ({ mark, t0, quiet }) => {
        const p = (window as unknown as { __lane: Probe }).__lane
        const at = p.marks[mark]
        if (at === undefined) return null
        const after = p.frames.filter((f) => f >= at)
        for (let i = 0; i < after.length; i++) {
          const next = after[i + 1] ?? performance.now()
          if (next - after[i] >= quiet) {
            const frames = p.frames.filter((f) => f >= t0 && f <= after[i]).length
            return [after[i] - t0, at - t0, frames]
          }
        }
        return null
      },
      { mark, t0, quiet: QUIET_MS }
    )
    return result !== null
  }, 90_000)
  return result
}

interface Result {
  ok: boolean
  drains: number[]
  notes: string[]
}

async function run(client: Client): Promise<Result> {
  const notes: string[] = []
  const check = (ok: boolean, what: string) => {
    notes.push(`${ok ? 'ok' : 'FAIL'} ${what}`)
    console.log(`${client.name}: ${ok ? 'ok' : 'FAIL'} ${what}`)
    return ok
  }
  await client.page.addInitScript(instrument)
  await client.page.reload()
  await client.sees('What should we do in alpha?', 60_000)
  await client.page.waitForTimeout(1500)
  await client.click('New terminal')
  let ok = check(await client.sees('Terminal 1', 30_000), 'tab "Terminal 1" opened')
  await client.page.waitForTimeout(1500)
  await focus(client)
  await line(client, `clear; bash ${SCREEN}; echo SCREEN-1-DONE`)
  ok = check(await shows(client, 'ascii again'), 'colour screen printed') && ok
  await shot(client, options.root, '1-colours')
  await line(client, 'clear; ls --color=always -F / ; echo LS-$((1+1))-DONE')
  ok = check(await shows(client, 'LS-2-DONE'), 'ls --color printed') && ok
  await shot(client, options.root, '2-ls')
  await line(client, 'vim -u NONE -c "syntax on" -c "set hlsearch" src/app.ts')
  ok = check(await shows(client, 'greet'), 'vim shows src/app.ts') && ok
  await client.page.keyboard.type('/name', { delay: 20 })
  await client.page.keyboard.press('Enter')
  await shot(client, options.root, '3-vim')
  await client.page.keyboard.type(':q!', { delay: 20 })
  await client.page.keyboard.press('Enter')
  await line(client, 'clear; htop -d 50')
  ok = check(await shows(client, /F10\s*Quit|Quit\W/), 'htop drew its key bar') && ok
  await client.page.waitForTimeout(3000)
  await shot(client, options.root, '4-htop')
  await client.page.keyboard.press('q')
  await client.page.waitForTimeout(800)
  const drains: number[] = []
  const runs = Number(values.runs)
  for (let i = 0; i < runs; i++) {
    await line(client, `clear; echo READY-$((${i}+200))-GO`)
    ok = check(await shows(client, `READY-${i + 200}-GO`), `ready for run ${i + 1}`) && ok
    await client.page.waitForTimeout(800)
    await client.page.keyboard.type(`seq 200000; echo SEQ-$((${i}+100))-DONE`, { delay: 5 })
    await client.page.waitForTimeout(300)
    const t0 = await client.page.evaluate(() => performance.now())
    await client.page.keyboard.press('Enter')
    const timed = await drained(client, `SEQ-${i + 100}-DONE`, t0)
    const [ms, arrived, frames] = (timed ?? [-1, -1, 0]).map(Math.round)
    if (timed) drains.push(ms)
    ok =
      check(
        timed !== null,
        `seq 200000 run ${i + 1} drained in ${ms} ms (all output in at ${arrived} ms, ${frames} frames)`
      ) && ok
  }
  ok = check(await shows(client, '199999'), 'the last screen ends in 199999, 200000') && ok
  await shot(client, options.root, '5-seq')
  await line(client, 'clear; echo PID-$$-SHELL')
  ok = check(await shows(client, /PID-\d+-SHELL/), 'shell printed its pid') && ok
  const pid = /PID-(\d+)-SHELL/.exec(await screenText(client))?.[1] ?? '?'
  await client.click('Project: beta')
  await client.page.waitForTimeout(2500)
  await shot(client, options.root, '6-beta')
  await client.click('Project: alpha')
  ok =
    check(await client.sees('Terminal 1', 20_000), 'the terminal tab is back after the switch') &&
    ok
  await client.page.waitForTimeout(1500)
  await focus(client)
  await line(client, 'echo AGAIN-$$-SHELL')
  ok =
    check(await shows(client, `AGAIN-${pid}-SHELL`), `same shell pid ${pid} after beta and back`) &&
    ok
  await shot(client, options.root, '7-preserved')
  return { ok, drains, notes }
}

const world = await createWorld({
  port: Number(values.port),
  root: options.root,
  binary: values['server-bin']
})
let browser: Browser | undefined
try {
  browser = await chromium.launch({ args: runeWebArgs(REFERENCE_ARGS) })
  const results: Array<[string, Result]> = []
  if (values.only !== 'rune') results.push(['react', await inReact(browser, world, options, run)])
  if (values.only !== 'react') results.push(['rune', await inRune(browser, world, options, run)])
  for (const [name, r] of results)
    console.log(`${name}: ${r.ok ? 'PASS' : 'FAIL'} drains ${JSON.stringify(r.drains)} ms`)
  await Bun.write(
    path.join(options.root, `results-${values.only ?? 'both'}.json`),
    JSON.stringify(results, null, 2)
  )
  process.exitCode = results.every(([, r]) => r.ok) ? 0 : 1
} finally {
  try {
    writePairs(options.root)
  } finally {
    try {
      await browser?.close()
    } finally {
      try {
        world.ws.close()
      } finally {
        await world.server.stop()
      }
    }
  }
}
