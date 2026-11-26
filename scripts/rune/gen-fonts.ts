// Converts the web client's bundled fonts to the TTF files Rune loads
// (AppSpec::fonts takes TTF or OTF, not woff2) and records each source's
// SHA-256, so a font change in the web client fails the check until rerun.
//   bun scripts/rune/gen-fonts.ts          convert (downloads wawoff2 through bunx)
//   bun scripts/rune/gen-fonts.ts --check  exit 1 when a source font changed
import { createHash } from 'node:crypto'
import { existsSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

const ROOT = join(import.meta.dir, '..', '..')
const FONTS_DIR = join(ROOT, 'apps/rune/crates/theme/assets/fonts')
const SOURCES_JSON = join(FONTS_DIR, 'sources.json')
const WAWOFF2 = 'wawoff2@2.0.1'

/** Output file and the woff2 the web client loads for it. */
const FONTS: Record<string, string> = {
  'InterVariable.ttf':
    'node_modules/@fontsource-variable/inter/files/inter-latin-wght-normal.woff2',
  'InterVariable-Italic.ttf':
    'node_modules/@fontsource-variable/inter/files/inter-latin-wght-italic.woff2',
  'PaperMono.ttf': 'src/renderer/assets/fonts/paper-mono/PaperMono[wght].woff2',
  'TermulSymbols.ttf': 'src/renderer/assets/fonts/termul-symbols/TermulSymbols.woff2'
}

function sha256(path: string): string {
  return createHash('sha256').update(readFileSync(path)).digest('hex')
}

function sourcesJson(): string {
  const sources = Object.fromEntries(
    Object.entries(FONTS).map(([out, src]) => [
      out,
      { source: src, sha256: sha256(join(ROOT, src)) }
    ])
  )
  return `${JSON.stringify(sources, null, 2)}\n`
}

if (import.meta.main) {
  if (process.argv.includes('--check')) {
    if (!existsSync(SOURCES_JSON) || readFileSync(SOURCES_JSON, 'utf8') !== sourcesJson()) {
      console.error(`stale: ${SOURCES_JSON}`)
      console.error('Run: bun scripts/rune/gen-fonts.ts')
      process.exit(1)
    }
  } else {
    // bunx writes a lockfile into its working directory; keep it out of the repo.
    const cwd = mkdtempSync(join(tmpdir(), 'gen-fonts-'))
    for (const [out, src] of Object.entries(FONTS)) {
      const run = Bun.spawnSync(
        ['bun', 'x', '-p', WAWOFF2, 'woff2_decompress.js', join(ROOT, src), join(FONTS_DIR, out)],
        { cwd, stdout: 'inherit', stderr: 'inherit' }
      )
      if (run.exitCode !== 0) throw new Error(`woff2_decompress failed for ${src}`)
    }
    writeFileSync(SOURCES_JSON, sourcesJson())
    console.log(`wrote ${Object.keys(FONTS).length} fonts`)
  }
}
