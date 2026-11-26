import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { describe, expect, it, vi } from 'vitest'
import { keySources, recordKeys } from './command-keys'
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
const KEYS = join(DIR, 'keys.json')

function compare(file: string, text: string): void {
  if (process.env.RECORD_VECTORS === '1') {
    mkdirSync(DIR, { recursive: true })
    writeFileSync(file, text)
    return
  }
  expect(existsSync(file), `${file} is missing`).toBe(true)
  expect(readFileSync(file, 'utf8') === text, `${file} is stale: rerun with RECORD_VECTORS=1`).toBe(
    true
  )
}

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
    compare(FILE, `${JSON.stringify(recorded)}\n`)
  })

  it('match what the command editor keymap does in CodeMirror today', () => {
    const recorded = {
      sources: Object.fromEntries(keySources.map((path) => [path, sourceHash(path)])),
      cases: recordKeys()
    }
    compare(KEYS, `${JSON.stringify(recorded, null, 1)}\n`)
  })
})
