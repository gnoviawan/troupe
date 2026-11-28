import { readFileSync } from 'node:fs'

const FIELDS = ['tag', 'repo', 'rev']

export function parseComuPin(text) {
  const pin = {}
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.trim()
    if (line === '') continue
    const eq = line.indexOf('=')
    if (eq <= 0) throw new Error(`troupe-comu.pin: malformed line "${line}"`)
    const key = line.slice(0, eq)
    if (!FIELDS.includes(key)) throw new Error(`troupe-comu.pin: unknown field "${key}"`)
    if (key in pin) throw new Error(`troupe-comu.pin: duplicate field "${key}"`)
    pin[key] = line.slice(eq + 1)
  }
  for (const key of FIELDS) {
    if (!pin[key]) throw new Error(`troupe-comu.pin: missing field "${key}"`)
  }
  if (!/^[0-9a-f]{40}$/.test(pin.rev)) {
    throw new Error('troupe-comu.pin: rev must be 40 lowercase hex characters')
  }
  if (!pin.repo.startsWith('https://')) {
    throw new Error('troupe-comu.pin: repo must be https')
  }
  return pin
}

export function readComuPin(path) {
  return parseComuPin(readFileSync(path, 'utf8'))
}

export function cargoComuEngineRef(cargoToml) {
  for (const line of cargoToml.split(/\r?\n/)) {
    if (!/^\s*troupe-comu-engine\s*=/.test(line)) continue
    const tag = /\btag\s*=\s*"([^"]+)"/.exec(line)
    if (tag) return { kind: 'tag', value: tag[1] }
    const rev = /\brev\s*=\s*"([^"]+)"/.exec(line)
    if (rev) return { kind: 'rev', value: rev[1] }
    throw new Error('Cargo.toml: troupe-comu-engine dependency has no tag or rev')
  }
  return null
}
