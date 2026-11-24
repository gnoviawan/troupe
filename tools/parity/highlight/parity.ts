/**
 * Colour parity of chat code blocks: the web client's Shiki output (the default `termul` syntax theme,
 * `lib/themes/syntax-shiki.ts`) against the Rune client's (`rune-highlights` through
 * `ui::markdown::syntax_colour`), per non-space character.
 *
 * bun tools/parity/highlight/parity.ts [rune.json]
 *
 * Without `rune.json` it runs `cargo run -p ui --example highlight_dump` itself.
 */

import { createHighlighter } from 'shiki'
import { getColorThemeDefinition } from '../../../src/renderer/lib/themes/bundled-themes'
import { syntaxThemeFor } from '../../../src/renderer/lib/themes/syntax-shiki'
import { SAMPLES } from './samples'

const RUNE_DIR = new URL('../../../apps/rune', import.meta.url).pathname

async function runeColours(
  file: string | undefined
): Promise<{ lang: string; colours: string[] }[]> {
  if (file) return JSON.parse(await Bun.file(file).text())
  const proc = Bun.spawn(['cargo', 'run', '-q', '-p', 'ui', '--example', 'highlight_dump'], {
    cwd: RUNE_DIR,
    stdin: new Blob([JSON.stringify(SAMPLES)]),
    stdout: 'pipe',
    stderr: 'inherit'
  })
  const out = await new Response(proc.stdout).text()
  if ((await proc.exited) !== 0) throw new Error('highlight_dump failed')
  return JSON.parse(out)
}

/** `#rrggbbaa` or `#RRGGBB` to `#rrggbb`. */
const norm = (hex: string) => hex.slice(0, 7).toLowerCase()

const theme = syntaxThemeFor(getColorThemeDefinition('termul'))
const highlighter = await createHighlighter({
  themes: [theme],
  langs: [...new Set(SAMPLES.map((s) => s.lang))]
})
const rune = await runeColours(process.argv[2])

let same = 0
let total = 0
const rows: string[] = []
SAMPLES.forEach((sample, i) => {
  const web: string[] = []
  for (const line of highlighter.codeToTokensBase(sample.code, {
    lang: sample.lang,
    theme
  })) {
    for (const token of line) for (const _ of token.content) web.push(norm(token.color ?? theme.fg))
    web.push(norm(theme.fg))
  }
  const chars = [...sample.code]
  const mismatches: string[] = []
  let s = 0
  let n = 0
  let run = ''
  let runKey = ''
  const flushRun = () => {
    if (run && mismatches.length < 6) mismatches.push(`\`${run}\` ${runKey}`)
    run = ''
    runKey = ''
  }
  chars.forEach((ch, k) => {
    if (/\s/.test(ch)) {
      flushRun()
      return
    }
    n += 1
    const r = norm(rune[i].colours[k])
    if (r === web[k]) {
      s += 1
      flushRun()
      return
    }
    const key = `web ${web[k]} rune ${r}`
    if (key !== runKey) flushRun()
    run += ch
    runKey = key
  })
  flushRun()
  same += s
  total += n
  rows.push(`| ${sample.lang} | ${n} | ${((100 * s) / n).toFixed(1)}% | ${mismatches.join('; ')} |`)
})
console.log('| Language | Characters | Same colour | First differences |')
console.log('| --- | ---: | ---: | --- |')
for (const row of rows) console.log(row)
console.log(`| all | ${total} | ${((100 * same) / total).toFixed(1)}% | |`)
