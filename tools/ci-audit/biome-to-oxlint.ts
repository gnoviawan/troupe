// Maps every Biome lint rule this repo has on to a `bun lint` rule and proves the mapping on
// Biome's own documentation examples.
//   bun tools/ci-audit/biome-to-oxlint.ts map [--write]          mapped and unmapped rules; --write saves .oxlintrc.json
//   bun tools/ci-audit/biome-to-oxlint.ts seed                   which Biome examples bun lint also flags
//   bun tools/ci-audit/biome-to-oxlint.ts extras                 bun lint rules that are off and find nothing in this tree
//   bun tools/ci-audit/biome-to-oxlint.ts suppressions [--write] rewrites biome-ignore comments for mapped rules
// BUN_LINT names a bun binary that has `bun lint` (default: bun).
import { mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { basename, join, resolve } from 'node:path'

type Severity = 'error' | 'warn'
type Source = { kind: string; plugin: string; rule: string }
type Example = { lang: string; code: string }
type BiomeRule = { id: string; language: string; sources: Source[]; examples: Example[] }
type ActiveRule = BiomeRule & { severity: Severity }
type BunRule = {
  id: string
  scope: string
  value: string
  aliases: string[]
  modes: string[]
  type_aware: boolean
  deprecated: boolean
}
type Mapping = { biome: ActiveRule; bun: string | null; via: string; typeAware: boolean }
type Case = { rule: string; file: string; code: string }
type Hits = Map<string, Map<string, string>>
type LintConfig = {
  plugins: string[]
  categories: Record<string, string>
  ignorePatterns: string[]
  rules: Record<string, unknown>
  overrides: { files: string[]; rules: Record<string, unknown> }[]
}

const ROOT = resolve(import.meta.dir, '../..')
const BUN = process.env.BUN_LINT ?? 'bun'
const BIOME = join(ROOT, 'node_modules/.bin/biome')
const METADATA_URL = 'https://biomejs.dev/metadata/rules.json'

// Biome rules whose metadata names no source bun lint has, but whose check a bun lint rule
// makes. `seed` proves each one on Biome's examples.
const MANUAL: Record<string, string> = {
  'correctness/noUnusedImports': 'eslint/no-unused-vars',
  'correctness/noUnusedVariables': 'eslint/no-unused-vars',
  'correctness/noUnusedFunctionParameters': 'eslint/no-unused-vars',
  'suspicious/noEmptyInterface': 'typescript/no-empty-interface',
  'performance/noAccumulatingSpread': 'oxc/no-accumulating-spread',
  'suspicious/noConstEnum': 'oxc/no-const-enum',
  'suspicious/noMisrefactoredShorthandAssign': 'oxc/misrefactored-assign-op',
  'suspicious/noGlobalIsNan': 'unicorn/prefer-number-properties',
  'suspicious/noGlobalIsFinite': 'unicorn/prefer-number-properties',
  'suspicious/noRedundantUseStrict': 'eslint/strict',
  'complexity/noFlatMapIdentity': 'unicorn/prefer-array-flat',
  'suspicious/noUselessEscapeInString': 'eslint/no-useless-escape'
}

// Options that give a bun lint rule the scope of the Biome rule it replaces.
const OPTIONS: Record<string, unknown[]> = {
  // noDoubleEquals allows `== null` (ignoreNull defaults to true).
  'eslint/eqeqeq': ['always', { null: 'ignore' }],
  // noInvalidUseBeforeDeclaration reports only uses that throw or read undefined: not
  // hoisted functions, not types, not reads inside a closure that runs later.
  'eslint/no-use-before-define': [
    {
      functions: false,
      classes: false,
      variables: false,
      typedefs: false,
      enums: false,
      ignoreTypeReferences: true,
      allowNamedExports: true
    }
  ],
  // noUnusedVariables and noUnusedFunctionParameters skip names that start with `_`.
  'eslint/no-unused-vars': [
    { args: 'all', argsIgnorePattern: '^_', varsIgnorePattern: '^_', caughtErrors: 'none' }
  ],
  // noEmptyInterface allows `interface A extends B {}`.
  'typescript/no-empty-interface': [{ allowSingleExtends: true }]
}

const PREFIXES: Record<string, string[]> = {
  eslint: ['eslint'],
  eslintTypeScript: ['typescript'],
  eslintReact: ['react'],
  eslintReactX: ['react'],
  eslintReactHooks: ['react-hooks', 'react'],
  eslintJsxA11y: ['jsx-a11y'],
  eslintUnicorn: ['unicorn'],
  eslintImport: ['import'],
  eslintJest: ['jest'],
  eslintVitest: ['vitest'],
  eslintNext: ['nextjs'],
  eslintPromise: ['promise'],
  eslintN: ['node'],
  eslintJsDoc: ['jsdoc'],
  oxc: ['oxc']
}

const EXT: Record<string, string> = {
  js: 'js',
  jsx: 'jsx',
  ts: 'ts',
  tsx: 'tsx',
  css: 'css',
  json: 'json',
  jsonc: 'json'
}

// noRestrictedImports has no doc example for this repo's options, so these stand in.
const RESTRICTED_IMPORT_CASES: Case[] = [
  "import { invoke } from '@tauri-apps/api/core'\nexport { invoke }\n",
  "import type { Event } from '@tauri-apps/api/event'\nexport type { Event }\n",
  "export { open } from '@tauri-apps/plugin-dialog'\n",
  "export const load = () => import('@tauri-apps/api/core')\n",
  "export const core = require('@tauri-apps/api/core')\n"
].map((code, i) => ({
  rule: 'style/noRestrictedImports',
  file: `style__noRestrictedImports__${i}.ts`,
  code
}))

function sh(cmd: string[], cwd = ROOT): string {
  const out = Bun.spawnSync(cmd, { cwd, stdout: 'pipe', stderr: 'pipe' })
  return out.stdout.toString() + out.stderr.toString()
}

function json(output: string): any {
  return JSON.parse(output.slice(output.indexOf('{'), output.lastIndexOf('}') + 1))
}

// The name a rule has in .oxlintrc.json. `-f json` prints the same name as `plugin(rule)`.
function configName(rule: BunRule): string {
  if (rule.id.startsWith('react-hooks/')) return rule.id
  const scope = { jsx_a11y: 'jsx-a11y', react_perf: 'react-perf' }[rule.scope] ?? rule.scope
  return `${scope}/${rule.value}`
}

function diagnosticRule(code: string | undefined): string {
  if (!code) return 'parser'
  const match = /^([^(]+)\((.+)\)$/.exec(code)
  return match ? `${match[1]}/${match[2]}` : code
}

function allBunRules(): BunRule[] {
  return JSON.parse(sh([BUN, 'lint', '--no-script', '--rules', '--format', 'json']))
}

function bunRuleIndex(): Map<string, BunRule> {
  const byName = new Map<string, BunRule>()
  for (const rule of allBunRules()) {
    for (const name of [configName(rule), rule.id, ...rule.aliases]) byName.set(name, rule)
  }
  return byName
}

function examples(docs: string): Example[] {
  const invalid = docs.split(/^ *### Valid/m)[0]
  return [...invalid.matchAll(/```(\w+),expect_diagnostic[^\n]*\n([\s\S]*?)```/g)].map((m) => ({
    lang: m[1],
    code: m[2].replace(/^ /gm, '')
  }))
}

async function biomeCatalog(): Promise<Map<string, BiomeRule>> {
  const cache = join(tmpdir(), 'biome-rules-metadata.json')
  const file = Bun.file(cache)
  if (!(await file.exists())) writeFileSync(cache, await (await fetch(METADATA_URL)).text())
  const languages: Record<string, Record<string, Record<string, any>>> = (await file.json()).lints
    .languages
  const catalog = new Map<string, BiomeRule>()
  for (const [language, groups] of Object.entries(languages)) {
    for (const [group, rules] of Object.entries(groups)) {
      for (const [name, meta] of Object.entries(rules)) {
        const id = `${group}/${name}`
        const rule = catalog.get(id) ?? { id, language: '', sources: [], examples: [] }
        rule.language = rule.language ? `${rule.language}+${language}` : language
        for (const s of meta.sources ?? []) {
          for (const [plugin, name] of Object.entries(s.source)) {
            rule.sources.push({ kind: s.kind, plugin, rule: String(name) })
          }
        }
        rule.examples.push(...examples(meta.docs ?? ''))
        catalog.set(id, rule)
      }
    }
  }
  return catalog
}

function seedCases(rules: Iterable<BiomeRule>): Case[] {
  const cases = [...rules].flatMap((rule) =>
    rule.examples
      .filter((e) => EXT[e.lang])
      .map((e, i) => ({
        rule: rule.id,
        file: `${rule.id.replace('/', '__')}__${i}.${EXT[e.lang]}`,
        code: e.code
      }))
  )
  return [...cases, ...RESTRICTED_IMPORT_CASES]
}

function writeSeeds(dir: string, cases: Case[], biomeConfig: any) {
  rmSync(dir, { recursive: true, force: true })
  mkdirSync(dir, { recursive: true })
  const { vcs: _vcs, files: _files, overrides: _overrides, ...rest } = biomeConfig
  writeFileSync(join(dir, 'biome.json'), JSON.stringify(rest))
  // Biome turns on its React and test rules when package.json lists react or vitest.
  writeFileSync(join(dir, 'package.json'), readFileSync(join(ROOT, 'package.json')))
  for (const c of cases) writeFileSync(join(dir, c.file), c.code)
}

// file name -> rule -> severity
function biomeHits(dir: string): Hits {
  const hits: Hits = new Map()
  const out = json(sh([BIOME, 'lint', '--reporter=json', '--max-diagnostics=none', '.'], dir))
  for (const d of out.diagnostics) {
    const file = basename(d.location?.path ?? '')
    const rule = String(d.category).replace(/^lint\//, '')
    const byRule = hits.get(file) ?? new Map()
    if (byRule.get(rule) !== 'error') byRule.set(rule, d.severity)
    hits.set(file, byRule)
  }
  return hits
}

function bunHits(dir: string, configPath: string, args: string[] = []): Hits {
  const hits: Hits = new Map()
  const out = json(
    sh([BUN, 'lint', '--no-script', '-c', configPath, '-f', 'json', ...args, '.'], dir)
  )
  for (const d of out.diagnostics) {
    const file = basename(String(d.filename))
    const byRule = hits.get(file) ?? new Map()
    byRule.set(diagnosticRule(d.code), d.severity)
    hits.set(file, byRule)
  }
  return hits
}

function configuredSeverity(value: unknown): string | undefined {
  if (typeof value === 'string') return value
  if (value && typeof value === 'object' && 'level' in value) return String(value.level)
}

// Active = what `biome rage` lists, plus every rule Biome flags in its own examples under
// this repo's config. The second set catches the React and test rules that `rage` omits.
function activeRules(
  catalog: Map<string, BiomeRule>,
  biomeConfig: any,
  seedHits: Hits
): ActiveRule[] {
  const listed = sh([BIOME, 'rage', '--linter'])
    .split('Enabled rules:')[1]
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => /^[a-zA-Z0-9]+\/[a-zA-Z0-9]+$/.test(line))
  const observed = new Map<string, string>()
  for (const byRule of seedHits.values()) {
    for (const [rule, severity] of byRule) {
      if (catalog.has(rule) && observed.get(rule) !== 'error') observed.set(rule, severity)
    }
  }
  const explicit: Record<string, Record<string, unknown>> = biomeConfig.linter.rules
  const ids = [...new Set([...listed, ...observed.keys()])].sort()
  return ids.map((id) => {
    const rule = catalog.get(id)
    if (!rule) throw new Error(`${id} is on but missing from ${METADATA_URL}`)
    const [group, name] = id.split('/')
    const explained = /Default severity: (\w+)/.exec(sh([BIOME, 'explain', name]))?.[1]
    const severity = configuredSeverity(explicit[group]?.[name]) ?? observed.get(id) ?? explained
    return { ...rule, severity: severity === 'error' ? 'error' : 'warn' }
  })
}

function mapRule(rule: ActiveRule, bun: Map<string, BunRule>): Mapping {
  const usable = (name: string, via: string): Mapping | null => {
    const found = bun.get(name)
    if (!found?.modes.includes('oxlint')) return null
    return { biome: rule, bun: configName(found), via, typeAware: found.type_aware }
  }
  if (MANUAL[rule.id]) {
    const manual = usable(MANUAL[rule.id], 'manual')
    if (!manual)
      throw new Error(`MANUAL maps ${rule.id} to ${MANUAL[rule.id]}, which bun lint does not have`)
    return manual
  }
  for (const kind of ['sameLogic', 'inspired']) {
    for (const source of rule.sources.filter((s) => s.kind === kind)) {
      for (const prefix of PREFIXES[source.plugin] ?? []) {
        const found = usable(`${prefix}/${source.rule}`, kind)
        if (found) return found
      }
    }
  }
  const sources = rule.sources.map((s) => `${s.plugin}:${s.rule}`).join(' ') || 'none'
  return { biome: rule, bun: null, via: `sources: ${sources}`, typeAware: false }
}

function buildConfig(biomeConfig: any, mappings: Mapping[]): LintConfig {
  const severities: Record<string, Severity> = {}
  const byBiome = new Map<string, string>()
  for (const { biome, bun } of mappings) {
    if (!bun) continue
    byBiome.set(biome.id, bun)
    if (severities[bun] !== 'error') severities[bun] = biome.severity
  }
  const rules: Record<string, unknown> = {}
  for (const [name, severity] of Object.entries(severities).sort(([a], [b]) =>
    a.localeCompare(b)
  )) {
    rules[name] = OPTIONS[name] ? [severity, ...OPTIONS[name]] : severity
  }
  const restricted = biomeConfig.linter.rules.style.noRestrictedImports
  const restrictedRule = byBiome.get('style/noRestrictedImports')
  if (!restrictedRule) throw new Error('style/noRestrictedImports has no bun lint rule')
  rules[restrictedRule] = [restricted.level, { patterns: restricted.options.patterns }]
  const overrides = biomeConfig.overrides.map((override: any) => {
    const off: Record<string, unknown> = {}
    for (const [group, groupRules] of Object.entries(override.linter.rules)) {
      for (const [name, value] of Object.entries(groupRules as Record<string, unknown>)) {
        const target = byBiome.get(`${group}/${name}`)
        if (!target)
          throw new Error(`override turns off ${group}/${name}, which has no bun lint rule`)
        off[target] = value
      }
    }
    return { files: override.includes, rules: off }
  })
  return {
    plugins: [
      'typescript',
      'unicorn',
      'oxc',
      'react',
      'react-perf',
      'jsx-a11y',
      'import',
      'promise',
      'node',
      'nextjs',
      'jest',
      'vitest'
    ],
    categories: { correctness: 'off' },
    ignorePatterns: biomeConfig.files.includes
      .filter((glob: string) => glob.startsWith('!'))
      .map((glob: string) => glob.slice(1)),
    rules,
    overrides
  }
}

async function analyze() {
  const biomeConfig = JSON.parse(readFileSync(join(ROOT, 'biome.json'), 'utf8'))
  const catalog = await biomeCatalog()
  const dir = join(tmpdir(), 'biome-to-oxlint-seed')
  const cases = seedCases(catalog.values())
  writeSeeds(dir, cases, biomeConfig)
  const seedBiome = biomeHits(dir)
  const index = bunRuleIndex()
  const mappings = activeRules(catalog, biomeConfig, seedBiome).map((rule) => mapRule(rule, index))
  const config = buildConfig(biomeConfig, mappings)
  return { mappings, config, cases, dir, seedBiome }
}

async function map(write: boolean) {
  const { mappings, config } = await analyze()
  const mapped = mappings.filter((m) => m.bun)
  const unmapped = mappings.filter((m) => !m.bun)
  console.log(`| Biome rule | language | severity | bun lint rule | via |\n|---|---|---|---|---|`)
  for (const m of [...mapped, ...unmapped]) {
    const bunRule = m.bun ? `${m.bun}${m.typeAware ? ' (type-aware)' : ''}` : '**none**'
    console.log(
      `| ${m.biome.id} | ${m.biome.language} | ${m.biome.severity} | ${bunRule} | ${m.via} |`
    )
  }
  console.log(
    `\n${mappings.length} active Biome rules: ${mapped.length} mapped, ${unmapped.length} unmapped`
  )
  if (write) writeFileSync(join(ROOT, '.oxlintrc.json'), `${JSON.stringify(config, null, 2)}\n`)
}

async function seed() {
  const { mappings, config, cases, dir, seedBiome } = await analyze()
  const configPath = join(dir, '.oxlintrc.json')
  writeFileSync(configPath, JSON.stringify({ ...config, ignorePatterns: [] }))
  const seedBun = bunHits(dir, configPath, ['--type-aware'])
  const byRule = new Map(mappings.map((m) => [m.biome.id, m]))
  const rows = new Map<string, { flagged: number; caught: number; missed: string[] }>()
  for (const c of cases) {
    const m = byRule.get(c.rule)
    if (!m || !seedBiome.get(c.file)?.has(c.rule)) continue
    const row = rows.get(c.rule) ?? { flagged: 0, caught: 0, missed: [] }
    row.flagged++
    const bun = seedBun.get(c.file)
    if ((m.bun && bun?.has(m.bun)) || bun?.has('parser')) row.caught++
    else row.missed.push(c.file)
    rows.set(c.rule, row)
  }
  console.log(
    `| Biome rule | severity | bun lint rule | Biome flags | bun lint flags | missed examples |\n|---|---|---|---|---|---|`
  )
  for (const [id, row] of [...rows].sort(([a], [b]) => a.localeCompare(b))) {
    const m = byRule.get(id) as Mapping
    const missed = row.missed.map(
      (file) =>
        `\`${cases
          .find((c) => c.file === file)
          ?.code.trim()
          .split('\n')
          .join(' ⏎ ')
          .slice(0, 80)}\``
    )
    console.log(
      `| ${id} | ${m.biome.severity} | ${m.bun ?? 'none'} | ${row.flagged} | ${row.caught} | ${missed.join('<br>')} |`
    )
  }
  const mapped = [...rows].filter(([id]) => byRule.get(id)?.bun)
  const full = mapped.filter(([, r]) => r.caught === r.flagged)
  console.log(
    `\nmapped rules with a flagged example: ${mapped.length}; every example caught: ${full.length}`
  )
  const noExample = mappings.filter((m) => m.bun && !rows.has(m.biome.id)).map((m) => m.biome.id)
  console.log(`mapped rules with no example Biome flags: ${noExample.join(', ') || 'none'}`)
}

function extras() {
  const config = JSON.parse(readFileSync(join(ROOT, '.oxlintrc.json'), 'utf8')) as LintConfig
  const candidates = allBunRules().filter(
    (r) =>
      r.modes.includes('oxlint') &&
      !r.type_aware &&
      !r.deprecated &&
      !(configName(r) in config.rules)
  )
  const probe = join(tmpdir(), 'oxlintrc-extras.json')
  writeFileSync(
    probe,
    JSON.stringify({
      ...config,
      plugins: [...config.plugins, 'jest', 'jsdoc', 'vue'],
      rules: Object.fromEntries(candidates.map((r) => [configName(r), 'warn'])),
      overrides: []
    })
  )
  const counts = new Map<string, number>()
  for (const byRule of bunHits(ROOT, probe).values()) {
    for (const rule of byRule.keys()) counts.set(rule, (counts.get(rule) ?? 0) + 1)
  }
  const clean = candidates.map(configName).filter((name) => !counts.has(name))
  const failing = Object.fromEntries([...counts].sort((a, b) => a[1] - b[1]))
  console.log(
    JSON.stringify({ candidates: candidates.length, clean, filesFailing: failing }, null, 2)
  )
}

// `// biome-ignore lint/group/rule: reason` becomes `// eslint-disable-next-line bun-rule -- reason`.
// A Biome suppression covers the whole next node, but an eslint-disable-next-line covers one
// line. Where the bun lint rule reports further down the node, `bun lint` still fails after this.
async function suppressions(write: boolean) {
  const { mappings } = await analyze()
  const target = new Map(mappings.filter((m) => m.bun).map((m) => [m.biome.id, m.bun as string]))
  const files = sh(['git', 'grep', '-l', 'biome-ignore lint/']).trim().split('\n').filter(Boolean)
  const pattern =
    /(\/\/|\/\*|\{\/\*) biome-ignore(-all)? lint\/([a-zA-Z0-9]+\/[a-zA-Z0-9]+)(?::\s*(.*?))?( \*\/\}?| \*\/)?$/
  let converted = 0
  const kept: string[] = []
  for (const file of files) {
    const lines = readFileSync(join(ROOT, file), 'utf8').split('\n')
    let changed = false
    for (let i = 0; i < lines.length; i++) {
      const match = pattern.exec(lines[i])
      if (!match) continue
      const [whole, open, all, rule, reason = '', close = ''] = match
      const bun = target.get(rule)
      if (!bun) {
        kept.push(`${file}:${i + 1} ${rule}`)
        continue
      }
      const directive = all ? `eslint-disable ${bun}` : `eslint-disable-next-line ${bun}`
      const text = reason ? `${directive} -- ${reason}` : directive
      const comment = open === '//' ? `// ${text}` : `${open} ${text}${close || ' */'}`
      lines[i] = lines[i].replace(whole, comment)
      changed = true
      converted++
    }
    if (changed && write) writeFileSync(join(ROOT, file), lines.join('\n'))
  }
  console.log(
    `converted ${converted} suppressions; kept ${kept.length} for rules bun lint does not have:`
  )
  for (const line of kept) console.log(`  ${line}`)
}

const [command, arg] = process.argv.slice(2)
if (command === 'map') await map(arg === '--write')
else if (command === 'seed') await seed()
else if (command === 'extras') extras()
else if (command === 'suppressions') await suppressions(arg === '--write')
else {
  console.error('usage: biome-to-oxlint.ts map [--write] | seed | extras | suppressions [--write]')
  process.exit(2)
}
