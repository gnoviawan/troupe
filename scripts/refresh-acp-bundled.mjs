#!/usr/bin/env bun
// Refreshes Troupe's offline ACP fallbacks from their live sources:
//   agents.json + manifest.json + <id>.svg  from the official ACP registry
//   troupe.json                             from Troupe's registry overlay
// `--check` writes nothing, prints what is stale, and exits 1 when anything is.
import { readFile, writeFile } from 'node:fs/promises'
import { join } from 'node:path'

const UPSTREAM_URL = 'https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json'
const OVERLAY_URL = 'https://gnoviawan.github.io/troupe-acp-registry/v1/troupe.json'
const ACP_DIR = join(import.meta.dir, '..', 'src', 'renderer', 'assets', 'agent-icons', 'acp')
const ALIASES = join(import.meta.dir, '..', 'src', 'shared', 'acp-agent-aliases.json')
const SAFE_ID = /^[A-Za-z0-9._-]{1,128}$/
const MAX_SVG_BYTES = 64 * 1024

const check = process.argv.includes('--check')

async function fetchOk(url) {
  const response = await fetch(url)
  if (!response.ok) throw new Error(`${url} returned ${response.status}`)
  return response
}

function biomeFormat(path, content) {
  const result = Bun.spawnSync(['bunx', 'biome', 'format', `--stdin-file-path=${path}`], {
    stdin: new TextEncoder().encode(content)
  })
  if (result.exitCode !== 0) throw new Error(`biome format ${path}: ${result.stderr.toString()}`)
  return result.stdout.toString()
}

const json = (value) => `${JSON.stringify(value, null, 2)}\n`

async function readText(path) {
  return readFile(path, 'utf8').catch(() => null)
}

const aliases = JSON.parse(await readFile(ALIASES, 'utf8'))
const upstream = (await (await fetchOk(UPSTREAM_URL)).json()).agents ?? []
const listed = upstream.filter(
  (agent) => SAFE_ID.test(agent.id) && !Object.hasOwn(aliases, agent.id)
)

const agents = listed.map(({ id, name, version, description, distribution }) => ({
  id,
  name,
  version,
  description,
  distribution
}))

const manifestPath = join(ACP_DIR, 'manifest.json')
const manifest = JSON.parse(await readFile(manifestPath, 'utf8'))
const known = new Set(manifest.map((entry) => entry.id))
const newIcons = []
for (const agent of listed) {
  if (known.has(agent.id) || typeof agent.icon !== 'string' || !agent.icon.startsWith('https://'))
    continue
  const svg = (await (await fetchOk(agent.icon)).text()).trim()
  if (
    Buffer.byteLength(svg) > MAX_SVG_BYTES ||
    !/^<svg\b[^>]*\bviewBox=/i.test(svg) ||
    /<script\b|\son[a-z]+\s*=/i.test(svg)
  ) {
    console.warn(`skipping icon for ${agent.id}: not a plain SVG with a viewBox`)
    continue
  }
  const file = `${agent.id}.svg`
  newIcons.push({ file, svg: `${svg}\n` })
  manifest.push({ id: agent.id, name: agent.name, file })
}
manifest.sort((a, b) => a.id.localeCompare(b.id))

const overlay = await (await fetchOk(OVERLAY_URL)).text()

const outputs = [
  [join(ACP_DIR, 'agents.json'), json(agents)],
  [manifestPath, json(manifest)],
  [join(ACP_DIR, 'troupe.json'), overlay.endsWith('\n') ? overlay : `${overlay}\n`],
  ...newIcons.map(({ file, svg }) => [join(ACP_DIR, file), svg])
]

const stale = []
for (const [path, raw] of outputs) {
  const content = path.endsWith('.json') ? biomeFormat(path, raw) : raw
  if ((await readText(path)) === content) continue
  stale.push(path.slice(ACP_DIR.length + 1))
  if (!check) await writeFile(path, content)
}

if (stale.length === 0) {
  console.log('bundled ACP snapshot is current')
  process.exit(0)
}
console.log(`${check ? 'stale' : 'updated'}: ${stale.join(', ')}`)
if (check) process.exit(1)
