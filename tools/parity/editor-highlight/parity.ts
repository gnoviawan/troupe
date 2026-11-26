/**
 * Colour parity of the code editor: the React editor's CodeMirror grammars and
 * `HighlightStyle` (`components/editor/codemirror-theme.ts`, default `termul` theme) against
 * the Rune editor's tags (`troupe-editor`'s `highlight` module), per non-space character.
 *
 * bun tools/parity/editor-highlight/parity.ts [rune.json]
 *
 * Without `rune.json` it runs `cargo run -p troupe-editor --example highlight_dump` itself.
 * Samples are `samples/<name>.<ext>.txt`, so Biome and tsc leave them alone.
 * Needs the repo's node_modules (the CodeMirror language packages).
 */

import { readdirSync } from 'node:fs'
import { css } from '@codemirror/lang-css'
import { html } from '@codemirror/lang-html'
import { javascript } from '@codemirror/lang-javascript'
import { json } from '@codemirror/lang-json'
import { markdown } from '@codemirror/lang-markdown'
import { python } from '@codemirror/lang-python'
import { rust } from '@codemirror/lang-rust'
import { yaml } from '@codemirror/lang-yaml'
import type { LanguageSupport } from '@codemirror/language'
import { highlightTree, tagHighlighter, tags } from '@lezer/highlight'
import { getColorThemeDefinition } from '../../../src/renderer/lib/themes/bundled-themes'
import { resolveSyntaxColors } from '../../../src/renderer/lib/themes/resolve-syntax'

const HERE = new URL('.', import.meta.url).pathname
const RUNE_DIR = new URL('../../../apps/rune', import.meta.url).pathname
const SAMPLE_DIR = `${HERE}samples`

/** `loadLanguage` in `use-codemirror.ts`, by the extension `detectLanguage` maps. */
const LANGUAGES: Record<string, () => LanguageSupport> = {
  ts: () => javascript({ typescript: true, jsx: true }),
  js: () => javascript({ typescript: false, jsx: true }),
  json: () => json(),
  css: () => css(),
  html: () => html(),
  md: () => markdown(),
  py: () => python(),
  rs: () => rust(),
  yaml: () => yaml()
}

/** `buildHighlightStyle`'s rules, each named by the style the Rune editor's `Tag::name` prints. */
const STYLE = tagHighlighter([
  { tag: tags.keyword, class: 'keyword' },
  { tag: [tags.comment, tags.lineComment, tags.blockComment], class: 'comment' },
  { tag: [tags.string, tags.special(tags.string)], class: 'string' },
  { tag: [tags.number, tags.integer, tags.float], class: 'number' },
  { tag: tags.bool, class: 'bool' },
  { tag: tags.null, class: 'bool' },
  { tag: tags.function(tags.definition(tags.variableName)), class: 'function' },
  { tag: tags.function(tags.variableName), class: 'function' },
  { tag: tags.special(tags.variableName), class: 'property' },
  { tag: tags.definition(tags.variableName), class: 'definition' },
  { tag: tags.variableName, class: 'variable' },
  { tag: [tags.typeName, tags.className], class: 'type' },
  { tag: tags.propertyName, class: 'property' },
  { tag: tags.operator, class: 'operator' },
  { tag: tags.punctuation, class: 'punctuation' },
  { tag: tags.meta, class: 'keyword' },
  { tag: tags.regexp, class: 'string' },
  { tag: tags.tagName, class: 'tag' },
  { tag: tags.attributeName, class: 'attributeName' },
  { tag: tags.attributeValue, class: 'attributeValue' },
  { tag: tags.heading, class: 'heading' },
  { tag: tags.link, class: 'link' },
  { tag: tags.emphasis, class: 'emphasis' },
  { tag: tags.strong, class: 'strong' }
])

const colours = resolveSyntaxColors(getColorThemeDefinition('termul'))
/** The colour a style draws in; emphasis and strong keep the text colour. */
function colourOf(style: string): string {
  const key = style.split(' ').find((s) => s in colours) as keyof typeof colours | undefined
  return key ? colours[key].toLowerCase() : 'text'
}

function codemirrorStyles(file: string, code: string): string[] {
  const name = file.replace(/\.txt$/, '')
  const ext = name.slice(name.lastIndexOf('.') + 1)
  const tree = LANGUAGES[ext]().language.parser.parse(code)
  const byUnit: string[] = new Array(code.length).fill('')
  highlightTree(tree, STYLE, (from, to, cls) => {
    for (let i = from; i < to; i++) byUnit[i] = cls
  })
  // UTF-16 units to chars, as the Rune dump counts.
  const out: string[] = []
  for (let i = 0; i < code.length; i++) {
    out.push(byUnit[i])
    const c = code.charCodeAt(i)
    if (c >= 0xd800 && c < 0xdc00) i++
  }
  return out
}

async function runeStyles(
  file: string | undefined,
  paths: string[]
): Promise<Record<string, string[]>> {
  if (file) return JSON.parse(await Bun.file(file).text())
  const proc = Bun.spawn(
    ['cargo', 'run', '-q', '-p', 'troupe-editor', '--example', 'highlight_dump', ...paths],
    {
      cwd: RUNE_DIR,
      stdout: 'pipe',
      stderr: 'inherit'
    }
  )
  const out = await new Response(proc.stdout).text()
  if ((await proc.exited) !== 0) throw new Error('highlight_dump failed')
  return JSON.parse(out)
}

const files = readdirSync(SAMPLE_DIR).sort()
const paths = files.map((f) => `${SAMPLE_DIR}/${f}`)
const rune = await runeStyles(process.argv[2], paths)

let same = 0
let total = 0
const rows: string[] = [
  '| sample | chars | same colour | top differences (CodeMirror -> Rune) |',
  '|---|---|---|---|'
]
for (const path of paths) {
  const code = await Bun.file(path).text()
  const chars = [...code]
  const cm = codemirrorStyles(path, code)
  const rn = rune[path]
  if (!rn || rn.length !== chars.length)
    throw new Error(`${path}: ${rn?.length} Rune chars, ${chars.length} expected`)
  let s = 0
  let n = 0
  const diffs = new Map<string, number>()
  let word = ''
  chars.forEach((c, i) => {
    if (/\s/.test(c)) return
    n++
    if (colourOf(cm[i]) === colourOf(rn[i])) s++
    else {
      const key = `${cm[i] || 'plain'} -> ${rn[i] || 'plain'}`
      diffs.set(key, (diffs.get(key) ?? 0) + 1)
      word += c
      if (process.env.DETAIL && colourOf(cm[i + 1] ?? '') === colourOf(rn[i + 1] ?? '')) {
        console.log(`${path.slice(path.lastIndexOf('.') + 1)} ${key}: ${word}`)
        word = ''
      }
    }
  })
  same += s
  total += n
  const top = [...diffs.entries()]
    .sort((a, b) => b[1] - a[1])
    .slice(0, 4)
    .map(([k, v]) => `${k} (${v})`)
    .join(', ')
  rows.push(
    `| ${path.slice(path.lastIndexOf('/') + 1).replace(/\.txt$/, '')} | ${n} | ${((100 * s) / n).toFixed(1)}% | ${top} |`
  )
}
rows.push(`| all | ${total} | ${((100 * same) / total).toFixed(1)}% | |`)
console.log(rows.join('\n'))
