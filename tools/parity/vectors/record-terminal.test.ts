import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { describe, expect, it, vi } from 'vitest'
import { OUT_DIR, sourceHash, sourcesOutsideWorkflow } from './record'
import { logged, recordReducer, recordScheduler, recordStore, terminalSources } from './terminal'
import { recordHistory, recordRoutes, recordWords } from './terminal-io'

vi.mock('../../../src/renderer/lib/log-api', async (importOriginal) => ({
  ...(await importOriginal<object>()),
  logFrontendError: (payload: { level?: string; source?: string; message: string }) => {
    logged.push({
      level: payload.level ?? 'error',
      source: payload.source ?? 'renderer',
      message: payload.message
    })
    return Promise.resolve()
  }
}))

// Rewrite `out/terminal/` with
//   RECORD_VECTORS=1 bun run test tools/parity/vectors

const DIR = join(OUT_DIR, 'terminal')
const FILE = join(DIR, 'terminal.json')

describe('terminal session vectors', () => {
  it('match what lib/command-lifecycle and the command editor transport do today', async () => {
    for (const file of ['parity-vectors.yml', 'rune.yml'] as const) {
      expect(sourcesOutsideWorkflow(terminalSources, file), `${file} paths`).toEqual([])
    }
    const recorded = {
      sources: Object.fromEntries(terminalSources.map((path) => [path, sourceHash(path)])),
      reducer: recordReducer(7, 100, 12),
      store: recordStore(11, 160, 18),
      scheduler: await recordScheduler(13, 80, 12),
      words: recordWords(),
      routes: await recordRoutes(),
      history: recordHistory()
    }
    const text = `${JSON.stringify(recorded)}\n`
    if (process.env.RECORD_VECTORS === '1') {
      mkdirSync(DIR, { recursive: true })
      writeFileSync(FILE, text)
      return
    }
    expect(existsSync(FILE), `${FILE} is missing`).toBe(true)
    expect(
      readFileSync(FILE, 'utf8') === text,
      `${FILE} is stale: rerun with RECORD_VECTORS=1`
    ).toBe(true)
  })
})
