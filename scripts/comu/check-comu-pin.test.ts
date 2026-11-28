import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'
// @ts-expect-error Plain ESM helper shared with the workflow scripts.
import { cargoComuEngineRef, parseComuPin, readComuPin } from './comu-pin.mjs'

describe('comu-pin', () => {
  it('parses comu/troupe-comu.pin and matches Cargo.toml', () => {
    const root = resolve(import.meta.dirname, '..', '..')
    const pin = readComuPin(resolve(root, 'comu', 'troupe-comu.pin'))
    const cargo = readFileSync(resolve(root, 'src-tauri', 'Cargo.toml'), 'utf-8')
    const ref = cargoComuEngineRef(cargo)
    expect(ref).toEqual({ kind: 'tag', value: pin.tag })
  })

  it('rejects malformed pin files', () => {
    expect(() => parseComuPin(`tag=v1\nrepo=http://insecure\nrev=${'a'.repeat(40)}`)).toThrow(
      /https/
    )
    expect(() => parseComuPin('tag=v1\nrepo=https://example.com\nrev=short')).toThrow(/40/)
  })
})
