#!/usr/bin/env node
// Verifies that comu/troupe-comu.pin and the troupe-comu-engine Cargo dependency
// name the same release tag.
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { cargoComuEngineRef, readComuPin } from './comu-pin.mjs'

const root = resolve(import.meta.dirname, '..', '..')
const pinPath = process.argv[2] ?? resolve(root, 'comu', 'troupe-comu.pin')
const cargoPath = process.argv[3] ?? resolve(root, 'src-tauri', 'Cargo.toml')

const pin = readComuPin(pinPath)
const ref = cargoComuEngineRef(readFileSync(cargoPath, 'utf8'))

if (ref === null) {
  console.log(
    `comu pin check skipped: ${cargoPath} has no troupe-comu-engine dependency yet (pin tag ${pin.tag})`
  )
} else if (ref.kind === 'rev') {
  if (ref.value !== pin.rev) {
    console.error(
      `comu pin mismatch: comu/troupe-comu.pin rev is ${pin.rev}, Cargo.toml troupe-comu-engine rev is ${ref.value}`
    )
    process.exit(1)
  }
  console.log(`comu pin matches Cargo.toml troupe-comu-engine rev ${pin.rev}`)
} else if (ref.value !== pin.tag) {
  console.error(
    `comu pin mismatch: comu/troupe-comu.pin tag is ${pin.tag}, Cargo.toml troupe-comu-engine tag is ${ref.value}`
  )
  process.exit(1)
} else {
  console.log(`comu pin matches Cargo.toml troupe-comu-engine tag ${pin.tag}`)
}
