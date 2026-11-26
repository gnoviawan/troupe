// Writes every bundled Troupe theme as data for the Rune client: runs the real
// theme engine (applyColorTheme) per theme, layers its CSS variables over the
// `:root` blocks of src/renderer/index.css, resolves each one as Chromium does
// and writes apps/rune/crates/theme/assets/themes/<id>.json plus the typed
// Rust accessors in apps/rune/crates/theme/src/generated.rs.
//   bun scripts/rune/gen-themes.ts          rewrite the files
//   bun scripts/rune/gen-themes.ts --check  exit 1 when a committed file is stale
import { existsSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { isColor, parseColor, splitTopLevel, toHex8 } from './css-color'

const ROOT = join(import.meta.dir, '..', '..')
const THEMES_DIR = join(ROOT, 'apps/rune/crates/theme/assets/themes')
const GENERATED_RS = join(ROOT, 'apps/rune/crates/theme/src/generated.rs')
const INDEX_CSS = join(ROOT, 'src/renderer/index.css')
const REM_PX = 16

type Easing = [number, number, number, number]

interface ThemeData {
  id: string
  name: string
  family: string
  appearance: 'dark' | 'light'
  colors: Record<string, string>
  shadows: Record<string, string>
  numbers: Record<string, number>
  easings: Record<string, Easing>
  terminal: Record<string, string>
  syntax: Record<string, string>
}

const KEYWORD_EASINGS: Record<string, Easing> = {
  linear: [0, 0, 1, 1],
  ease: [0.25, 0.1, 0.25, 1],
  'ease-in': [0.42, 0, 1, 1],
  'ease-out': [0, 0, 0.58, 1],
  'ease-in-out': [0.42, 0, 0.58, 1]
}

function stripComments(css: string): string {
  return css.replace(/\/\*[\s\S]*?\*\//g, '')
}

/** Custom properties declared directly in the first `<selector> {` block. */
function declarations(css: string, selector: string): Map<string, string> {
  const start = css.indexOf(`${selector} {`)
  if (start === -1) throw new Error(`index.css has no "${selector}" block`)
  let depth = 0
  let end = start
  for (let i = css.indexOf('{', start); i < css.length; i++) {
    if (css[i] === '{') depth++
    else if (css[i] === '}' && --depth === 0) {
      end = i
      break
    }
  }
  const body = css.slice(css.indexOf('{', start) + 1, end)
  const vars = new Map<string, string>()
  for (const decl of splitTopLevel(body, ';')) {
    const match = /^(--[\w-]+)\s*:\s*([\s\S]+)$/.exec(decl)
    if (match) vars.set(match[1], match[2].replace(/\s+/g, ' ').trim())
  }
  return vars
}

interface StyleStub {
  props: Map<string, string>
  colorScheme: string
  readonly length: number
  item(i: number): string
  setProperty(name: string, value: string): void
  getPropertyValue(name: string): string
}

/** The little of `document` and `window` applyColorTheme touches, for runs outside a DOM. */
function installDomStub(): void {
  const props = new Map<string, string>()
  const style: StyleStub = {
    props,
    colorScheme: '',
    get length() {
      return props.size
    },
    item: (i) => [...props.keys()][i] ?? '',
    setProperty: (name, value) => props.set(name, value),
    getPropertyValue: (name) => props.get(name) ?? ''
  }
  const element = () => ({ id: '', append() {}, remove() {}, getContext: () => null })
  const g = globalThis as Record<string, unknown>
  g.document = {
    documentElement: {
      style,
      classList: { add() {}, remove() {} },
      removeAttribute: () => props.clear()
    },
    head: { append() {} },
    body: { offsetHeight: 0 },
    getElementById: () => null,
    createElement: element,
    createTextNode: () => ({}),
    querySelector: () => null
  }
  g.window = { dispatchEvent: () => true }
  g.requestAnimationFrame = () => 0
  g.getComputedStyle = () => ({ backgroundColor: '' })
}

async function engineVars(themeId: string): Promise<Map<string, string>> {
  if (typeof document === 'undefined') installDomStub()
  const { applyColorTheme } = await import('../../src/renderer/lib/themes/apply-color-theme')
  const root = document.documentElement
  root.removeAttribute('style')
  applyColorTheme(themeId)
  const vars = new Map<string, string>()
  for (let i = 0; i < root.style.length; i++) {
    const name = root.style.item(i)
    if (name.startsWith('--')) vars.set(name, root.style.getPropertyValue(name).trim())
  }
  return vars
}

/** Substitutes `var(--x[, fallback])` until none is left. */
function substitute(value: string, vars: Map<string, string>, seen: string[] = []): string {
  return value.replace(
    /var\(\s*(--[\w-]+)\s*(?:,\s*([^()]*))?\)/g,
    (_m, name: string, fallback?: string) => {
      if (seen.includes(name)) throw new Error(`var cycle: ${[...seen, name].join(' -> ')}`)
      const raw = vars.get(name) ?? fallback
      if (raw === undefined) throw new Error(`undefined var ${name}`)
      return substitute(raw, vars, [...seen, name])
    }
  )
}

const LENGTH = /^-?[\d.]+(px)?$/
const COMPONENTS = /^-?[\d.]+ -?[\d.]+ -?[\d.]+$/

/** `#rrggbbaa Xpx Ypx Bpx Spx [inset]` per layer, CSS order, or undefined when not a shadow list. */
function shadowList(value: string): string | undefined {
  if (value === 'none') return 'none'
  const layers = splitTopLevel(value, ',').map((layer) => {
    const tokens = splitTopLevel(layer, ' ')
    const inset = tokens.includes('inset')
    const rest = tokens.filter((t) => t !== 'inset')
    const lengths = rest.filter((t) => LENGTH.test(t))
    const colors = rest.filter((t) => !LENGTH.test(t))
    if (lengths.length < 2 || lengths.length > 4 || colors.length !== 1) return undefined
    if (!isColor(colors[0])) return undefined
    const px = [0, 1, 2, 3].map((i) => `${Number.parseFloat(lengths[i] ?? '0')}px`)
    return [toHex8(parseColor(colors[0])), ...px, ...(inset ? ['inset'] : [])].join(' ')
  })
  return layers.every((layer) => layer !== undefined) ? layers.join(', ') : undefined
}

function scalar(value: string): number | undefined {
  const match = /^(-?[\d.]+)(px|rem|ms|s|%)?$/.exec(value)
  if (!match) return undefined
  const n = Number.parseFloat(match[1])
  switch (match[2]) {
    case 'rem':
      return n * REM_PX
    case 's':
      return n * 1000
    case '%':
      return n / 100
    default:
      return n
  }
}

function easing(value: string): Easing | undefined {
  if (value in KEYWORD_EASINGS) return KEYWORD_EASINGS[value]
  const match = /^cubic-bezier\(([^)]*)\)$/.exec(value)
  if (!match) return undefined
  const parts = match[1].split(',').map((p) => Number.parseFloat(p))
  return parts.length === 4 ? (parts as Easing) : undefined
}

function sorted<T>(record: Record<string, T>): Record<string, T> {
  return Object.fromEntries(Object.entries(record).sort(([a], [b]) => a.localeCompare(b)))
}

function hexOf(value: string): string {
  return toHex8(parseColor(value))
}

async function resolveThemes(): Promise<ThemeData[]> {
  const { BUNDLED_COLOR_THEMES } = await import('../../src/renderer/lib/themes/bundled-themes')
  const { paletteToXtermTheme } = await import('../../src/renderer/lib/themes/apply-color-theme')
  const { resolveSyntaxColors } = await import('../../src/renderer/lib/themes/resolve-syntax')
  const css = stripComments(readFileSync(INDEX_CSS, 'utf8'))
  const base = declarations(css, ':root')
  const lightBase = declarations(css, ':root:not(.dark)')

  const themes: ThemeData[] = []
  for (const theme of Object.values(BUNDLED_COLOR_THEMES)) {
    const vars = new Map(base)
    if (theme.appearance === 'light') for (const [k, v] of lightBase) vars.set(k, v)
    for (const [k, v] of await engineVars(theme.id)) vars.set(k, v)

    const data: ThemeData = {
      id: theme.id,
      name: theme.name,
      family: theme.familyId,
      appearance: theme.appearance,
      colors: {},
      shadows: {},
      numbers: {},
      easings: {},
      terminal: {},
      syntax: {}
    }
    for (const [name, raw] of vars) {
      const key = name.slice(2)
      const value = substitute(raw, vars)
      const asNumber = scalar(value)
      const asEasing = easing(value)
      if (COMPONENTS.test(value)) data.colors[key] = hexOf(`oklch(${value})`)
      else if (asNumber !== undefined) data.numbers[key] = asNumber
      else if (asEasing) data.easings[key] = asEasing
      else if (isColor(value)) data.colors[key] = hexOf(value)
      else {
        const shadow = shadowList(value)
        if (shadow === undefined) throw new Error(`${theme.id}: cannot resolve ${name}: ${value}`)
        data.shadows[key] = shadow
      }
    }
    const xterm = paletteToXtermTheme(theme.dark.palette, theme.appearance, theme.dark.chrome)
    for (const [k, v] of Object.entries(xterm))
      if (typeof v === 'string') data.terminal[k] = hexOf(v)
    for (const [k, v] of Object.entries(resolveSyntaxColors(theme))) data.syntax[k] = hexOf(v)
    themes.push(data)
  }
  return themes
}

/** The theme file Rune's `Theme::from_json` reads: colours under the appearance's key. */
function themeJson(t: ThemeData): string {
  const doc = {
    id: t.id,
    name: t.name,
    family: t.family,
    appearance: t.appearance,
    blend: { mode: 'srgb', over: 'background' },
    [t.appearance]: sorted(t.colors),
    shadows: sorted(t.shadows),
    numbers: sorted(t.numbers),
    easings: sorted(t.easings),
    terminal: sorted(t.terminal),
    syntax: sorted(t.syntax)
  }
  // Number arrays on one line, as Biome formats them.
  const json = JSON.stringify(doc, null, 2).replace(
    /\[\s+([-\d.,\s]+?)\s+\]/g,
    (_m, items: string) => `[${items.split(/,\s*/).join(', ')}]`
  )
  return `${json}\n`
}

const RUST_KEYWORDS = new Set(['type', 'fn', 'match', 'move', 'ref', 'self', 'use', 'where'])

function snake(name: string): string {
  const ident = name
    .replace(/([a-z0-9])([A-Z])/g, '$1_$2')
    .replace(/-/g, '_')
    .toLowerCase()
  return RUST_KEYWORDS.has(ident) ? `${ident}_` : ident
}

function sameKeys(themes: ThemeData[], section: keyof ThemeData): string[] {
  const keys = Object.keys(themes[0][section] as object).sort()
  for (const t of themes) {
    const own = Object.keys(t[section] as object).sort()
    if (own.join() !== keys.join())
      throw new Error(`${t.id}: ${section} keys differ from ${themes[0].id}`)
  }
  return keys
}

function struct(name: string, doc: string, ty: string, keys: string[]): string {
  const fields = keys.map((k) => `    pub ${snake(k)}: ${ty},`).join('\n')
  const names = keys.map((k) => `        "${k}",`).join('\n')
  const get = keys.map((k) => `            "${k}" => Some(&self.${snake(k)}),`).join('\n')
  return `/// ${doc}
#[derive(Clone, Debug, PartialEq)]
pub struct ${name} {
${fields}
}

impl ${name} {
    /// Every token name, as in the theme file.
    pub const NAMES: &[&str] = &[
${names}
    ];

    /// The token named as in the theme file (\`"card"\`), or None.
    pub fn get(&self, name: &str) -> Option<&${ty}> {
        match name {
${get}
            _ => None,
        }
    }
}
`
}

function reader(name: string, keys: string[], read: (k: string) => string): string {
  const fields = keys.map((k) => `            ${snake(k)}: ${read(k)},`).join('\n')
  return `impl ${name} {
    pub(crate) fn read(src: &Source) -> Self {
        Self {
${fields}
        }
    }
}
`
}

function generatedRust(themes: ThemeData[]): string {
  const colors = sameKeys(themes, 'colors')
  const shadows = sameKeys(themes, 'shadows')
  const numbers = sameKeys(themes, 'numbers')
  const easings = sameKeys(themes, 'easings')
  const terminal = sameKeys(themes, 'terminal')
  const syntax = sameKeys(themes, 'syntax')
  const list = themes
    .map((t) => `    ("${t.id}", include_str!("../assets/themes/${t.id}.json")),`)
    .join('\n')
  return `// Generated by scripts/rune/gen-themes.ts. Do not edit.
use rune::{Color, Shadow};

use crate::Source;

/// Every bundled theme: its id and its theme file.
pub const THEMES: &[(&str, &str)] = &[
${list}
];

${struct('Colors', 'Every colour token (`--<name>` in index.css), as Chromium paints it.', 'Color', colors)}
${reader('Colors', colors, (k) => `src.color("${k}")`)}
${struct('Shadows', 'Every `box-shadow` token, layers in CSS order.', 'Vec<Shadow>', shadows)}
${reader('Shadows', shadows, (k) => `src.shadow("${k}")`)}
${struct('Numbers', 'Lengths in px, durations in ms, percentages as fractions, plain numbers.', 'f32', numbers)}
${reader('Numbers', numbers, (k) => `src.number("${k}")`)}
${struct('Easings', 'Cubic bezier control points `[x1, y1, x2, y2]`.', '[f32; 4]', easings)}
${reader('Easings', easings, (k) => `src.easing("${k}")`)}
${struct('Terminal', 'The xterm theme the terminal draws with.', 'Color', terminal)}
${reader('Terminal', terminal, (k) => `src.section_color("terminal", "${k}")`)}
${struct('Syntax', 'The editor and code-block syntax colours.', 'Color', syntax)}
${reader('Syntax', syntax, (k) => `src.section_color("syntax", "${k}")`)}`
}

/** Every generated file and its content. */
async function generateThemeFiles(): Promise<Map<string, string>> {
  const themes = await resolveThemes()
  const files = new Map<string, string>()
  for (const t of themes) files.set(join(THEMES_DIR, `${t.id}.json`), themeJson(t))
  files.set(GENERATED_RS, generatedRust(themes))
  return files
}

/** Paths whose committed content differs from a fresh run, plus stale theme files. */
async function staleThemeFiles(): Promise<string[]> {
  const files = await generateThemeFiles()
  const stale = [...files].filter(([path, content]) => {
    return !existsSync(path) || readFileSync(path, 'utf8') !== content
  })
  const extra = existsSync(THEMES_DIR)
    ? readdirSync(THEMES_DIR)
        .map((f) => join(THEMES_DIR, f))
        .filter((p) => !files.has(p))
    : []
  return [...stale.map(([path]) => path), ...extra]
}

if (import.meta.main) {
  if (process.argv.includes('--check')) {
    const stale = await staleThemeFiles()
    for (const path of stale) console.error(`stale: ${path}`)
    if (stale.length > 0) {
      console.error('Run: bun scripts/rune/gen-themes.ts')
      process.exit(1)
    }
  } else {
    const files = await generateThemeFiles()
    if (existsSync(THEMES_DIR)) {
      for (const f of readdirSync(THEMES_DIR)) {
        const p = join(THEMES_DIR, f)
        if (!files.has(p)) rmSync(p)
      }
    }
    for (const [path, content] of files) writeFileSync(path, content)
    console.log(`wrote ${files.size} files`)
  }
}
