import { expect, test } from 'bun:test'
import { mkdtempSync, writeFileSync } from 'node:fs'
import { createServer } from 'node:http'
import type { AddressInfo } from 'node:net'
import os from 'node:os'
import path from 'node:path'
import {
  RUNE_SHELL_SURFACES,
  runePending,
  runeWebDist,
  serveRuneClient,
  serveStatic
} from '../drivers/rune-apps'
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

test('the Rune shell surfaces exist and run; every other surface waits on a named unit', () => {
  const ids = new Set(SURFACES.map((s) => s.id))
  for (const id of RUNE_SHELL_SURFACES) expect(ids.has(id)).toBe(true)
  const dashboard = SURFACES.find((s) => s.id === 'shell.dashboard')
  expect(dashboard && runePending(dashboard)).toBeUndefined()
  const transcript = SURFACES.find((s) => s.id === 'chat.transcript')
  expect(transcript && runePending(transcript)).toBeUndefined()
  const chat = SURFACES.find((s) => s.area === 'chat' && !RUNE_SHELL_SURFACES.has(s.id))
  expect(chat && runePending(chat)?.unit).toBe('R6')
  const drawer = SURFACES.find((s) => s.id === 'phone.drawer')
  expect(drawer && runePending(drawer)).toBeUndefined()
  const files = SURFACES.find((s) => s.id === 'phone.files')
  expect(files && runePending(files)?.unit).toBe('R8')
})

test('serveRuneClient serves the build and forwards the rest, upgrades included, to the server', async () => {
  const dir = mkdtempSync(path.join(os.tmpdir(), 'rune-proxy-'))
  writeFileSync(path.join(dir, 'index.html'), '<p>rune</p>')
  const upstream = createServer((req, res) => {
    res.writeHead(200, { 'content-type': 'application/json' })
    res.end(JSON.stringify({ path: req.url, host: req.headers.host }))
  })
  upstream.on('upgrade', (req, socket) => {
    socket.end(`HTTP/1.1 101 Switching Protocols\r\nX-Path: ${req.url}\r\n\r\n`)
  })
  await new Promise<void>((resolve) => upstream.listen(0, '127.0.0.1', resolve))
  const { port } = upstream.address() as AddressInfo
  const proxy = await serveRuneClient(dir, `http://127.0.0.1:${port}`)
  try {
    expect(await (await fetch(`${proxy.url}/`)).text()).toBe('<p>rune</p>')
    const forwarded = (await (await fetch(`${proxy.url}/projects?x=1`)).json()) as {
      path: string
      host: string
    }
    expect(forwarded).toEqual({ path: '/projects?x=1', host: proxy.url.replace('http://', '') })
    const net = await import('node:net')
    const reply = await new Promise<string>((resolve, reject) => {
      const url = new URL(proxy.url)
      const socket = net.connect(Number(url.port), url.hostname, () => {
        socket.write(
          'GET /ws HTTP/1.1\r\nHost: x\r\nConnection: Upgrade\r\nUpgrade: websocket\r\n\r\n'
        )
      })
      let text = ''
      socket.on('data', (chunk) => {
        text += chunk.toString()
      })
      socket.on('end', () => resolve(text))
      socket.on('error', reject)
    })
    expect(reply).toContain('101 Switching Protocols')
    expect(reply).toContain('X-Path: /ws')
  } finally {
    await proxy.close()
    await new Promise((resolve) => upstream.close(resolve))
  }
})
