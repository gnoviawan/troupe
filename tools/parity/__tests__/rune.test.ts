import { expect, test } from 'bun:test'
import { mkdtempSync, writeFileSync } from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { runeWebDist, serveStatic } from '../drivers/rune-apps'
import { runeKey } from '../drivers/rune-socket'
import { GALLERY_SURFACES } from '../gallery'
import { SURFACES } from '../surfaces'

test('runeKey maps a matrix stroke to the live driver key command', () => {
  expect(runeKey('ctrl-shift-k')).toEqual({ name: 'k', mods: ['ctrl', 'shift'] })
  expect(runeKey('escape')).toEqual({ name: 'Escape', mods: [] })
  expect(runeKey('alt-,')).toEqual({ name: ',', mods: ['alt'] })
  expect(runeKey('space')).toEqual({ name: 'Space', mods: [] })
})

test('the gallery profile has one surface per registry entry, apart from the matrix', () => {
  const ids = GALLERY_SURFACES.map((s) => s.id)
  expect(ids.length).toBe(65)
  expect(new Set(ids).size).toBe(65)
  expect(ids.slice(0, 2)).toEqual(['color.surface', 'color.text'])
  expect(GALLERY_SURFACES.every((s) => s.profile === 'gallery')).toBe(true)
  expect(SURFACES.filter((s) => s.profile === 'gallery')).toEqual([])
})

test('serveStatic serves a wasm build with the type streaming compile needs, and nothing outside it', async () => {
  const dir = mkdtempSync(path.join(os.tmpdir(), 'rune-static-'))
  writeFileSync(path.join(dir, 'index.html'), '<p>hi</p>')
  writeFileSync(path.join(dir, 'app_bg.wasm'), new Uint8Array([0, 97, 115, 109]))
  const server = await serveStatic(dir)
  try {
    const wasm = await fetch(`${server.url}/app_bg.wasm`)
    expect(wasm.headers.get('content-type')).toBe('application/wasm')
    expect([...new Uint8Array(await wasm.arrayBuffer())]).toEqual([0, 97, 115, 109])
    expect(await (await fetch(`${server.url}/`)).text()).toBe('<p>hi</p>')
    expect((await fetch(`${server.url}/..%2f..%2fetc%2fpasswd`)).status).toBe(404)
    expect(Number(new URL(server.url).port)).toBeWithin(8190, 8200)
  } finally {
    await server.close()
  }
})

test('runeWebDist names the variable to set when the wasm build is missing', () => {
  const previous = process.env.TROUPE_RUNE_HELLO_WEB
  process.env.TROUPE_RUNE_HELLO_WEB = path.join(os.tmpdir(), 'no-such-rune-build')
  try {
    expect(() => runeWebDist('hello')).toThrow('set TROUPE_RUNE_HELLO_WEB')
  } finally {
    if (previous === undefined) delete process.env.TROUPE_RUNE_HELLO_WEB
    else process.env.TROUPE_RUNE_HELLO_WEB = previous
  }
})
