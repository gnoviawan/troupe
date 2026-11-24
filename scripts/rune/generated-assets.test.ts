import { spawnSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import { describe, expect, test } from 'vitest'

const ROOT = fileURLToPath(new URL('../..', import.meta.url))

// A child process, so the generator runs exactly as a contributor runs it.
function check(script: string): { code: number | null; output: string } {
  const run = spawnSync(process.execPath, [script, '--check'], { cwd: ROOT, encoding: 'utf8' })
  return { code: run.status, output: `${run.stdout}${run.stderr}` }
}

describe('Rune generated assets', () => {
  test('the theme files match the theme engine', () => {
    const { code, output } = check('scripts/rune/gen-themes.ts')
    expect(output).not.toContain('stale:')
    expect(code).toBe(0)
  })

  test('the icon catalog matches the icons the web client renders', () => {
    const { code, output } = check('scripts/rune/gen-icons.ts')
    expect(output).not.toContain('stale:')
    expect(code).toBe(0)
  })

  test('the Rune fonts come from the fonts the web client bundles', () => {
    const { code, output } = check('scripts/rune/gen-fonts.ts')
    expect(output).not.toContain('stale:')
    expect(code).toBe(0)
  })
})
