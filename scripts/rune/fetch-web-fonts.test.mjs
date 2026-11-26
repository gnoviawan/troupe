import assert from 'node:assert/strict'
import { mkdtempSync, readFileSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import test from 'node:test'
import { fetchFonts, sha256 } from './fetch-web-fonts.mjs'

const body = Buffer.from('a font')
const font = { name: 'A.ttf', url: 'https://fonts.test/A.ttf', sha256: sha256(body) }
const serve =
  (bytes, status = 200) =>
  async () =>
    new Response(bytes, { status })

test('writes a font whose hash matches the pin', async () => {
  const out = mkdtempSync(join(tmpdir(), 'web-fonts-'))
  assert.deepEqual(await fetchFonts(out, [font], serve(body)), [])
  assert.equal(readFileSync(join(out, 'A.ttf'), 'utf8'), 'a font')
})

test('refuses a font whose hash does not match and leaves no file', async () => {
  const out = mkdtempSync(join(tmpdir(), 'web-fonts-'))
  assert.deepEqual(await fetchFonts(out, [font], serve(Buffer.from('tampered'))), ['A.ttf'])
  assert.throws(() => readFileSync(join(out, 'A.ttf')))
})

test('reports a failed download', async () => {
  const out = mkdtempSync(join(tmpdir(), 'web-fonts-'))
  assert.deepEqual(await fetchFonts(out, [font], serve('', 404)), ['A.ttf'])
})

test('keeps a matching file without downloading it again', async () => {
  const out = mkdtempSync(join(tmpdir(), 'web-fonts-'))
  writeFileSync(join(out, 'A.ttf'), body)
  const never = async () => {
    throw new Error('fetched')
  }
  assert.deepEqual(await fetchFonts(out, [font], never), [])
})
