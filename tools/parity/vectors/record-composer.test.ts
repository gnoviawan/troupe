import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { describe, expect, it } from 'vitest'
import {
  composerCases,
  composerSources,
  contextCases,
  recordComposer,
  recordContext
} from './composer'
import { OUT_DIR, sourceHash, sourcesOutsideWorkflow } from './record'

// Rewrite `out/composer/` with
//   RECORD_VECTORS=1 bun run test tools/parity/vectors

const DIR = join(OUT_DIR, 'composer')
const FILE = join(DIR, 'composer.json')

describe('composer test vectors', () => {
  it('match what lib/composer serializes and sends today', () => {
    for (const file of ['parity-vectors.yml', 'rune.yml'] as const) {
      expect(sourcesOutsideWorkflow(composerSources, file), `${file} paths`).toEqual([])
    }
    const recorded = {
      sources: Object.fromEntries(composerSources.map((path) => [path, sourceHash(path)])),
      documents: composerCases.map(recordComposer),
      contexts: contextCases.map(recordContext)
    }
    const text = `${JSON.stringify(recorded, null, 1)}\n`
    if (process.env.RECORD_VECTORS === '1') {
      mkdirSync(DIR, { recursive: true })
      writeFileSync(FILE, text)
      return
    }
    expect(existsSync(FILE), `${FILE} is missing`).toBe(true)
    expect(readFileSync(FILE, 'utf8'), `${FILE} is stale: rerun with RECORD_VECTORS=1`).toBe(text)
  })
})
