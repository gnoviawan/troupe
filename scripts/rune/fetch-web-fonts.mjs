#!/usr/bin/env node
// Downloads the fonts the wasm gallery loads after its first frame (a CJK face and an emoji
// face: Rune's web target has no system fonts, so those glyphs draw as boxes without them)
// into a web build's folder, checking each file's SHA-256 against the pin below. Exits 1 on
// a failed download or a hash that does not match.
//   node scripts/rune/fetch-web-fonts.mjs --out apps/rune/target/web-gallery/fonts

import { createHash } from 'node:crypto'
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

// The file names are the URLs the gallery asks for (`fonts/<name>`, crates/gallery/src/lib.rs).
export const WEB_FONTS = [
  {
    name: 'NotoSansJP.ttf',
    url: 'https://raw.githubusercontent.com/google/fonts/295d98a7a0c17c68f1341eaeea354e7960ea70d3/ofl/notosansjp/NotoSansJP%5Bwght%5D.ttf',
    sha256: 'c2f3b4d463500a2ddcd3849cded1fceeb9fd6d1c32e6cbecd568453ba50fc68f'
  },
  {
    name: 'NotoColorEmoji.ttf',
    url: 'https://raw.githubusercontent.com/googlefonts/noto-emoji/v2.047/fonts/NotoColorEmoji.ttf',
    sha256: '39ee3c587e10e89669b9ff32703261d10d5f9c4dd5ad147b6b5a1c5200591817'
  }
]

export const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex')

// Writes every font into `out`, reusing a file already there when its hash matches. Returns
// the names that failed.
export async function fetchFonts(out, fonts = WEB_FONTS, get = fetch) {
  mkdirSync(out, { recursive: true })
  const failed = []
  for (const font of fonts) {
    const path = join(out, font.name)
    if (existsSync(path) && sha256(readFileSync(path)) === font.sha256) {
      console.log(`fetch-web-fonts: ${font.name} already there`)
      continue
    }
    try {
      const res = await get(font.url)
      if (!res.ok) throw new Error(`HTTP ${res.status}`)
      const bytes = Buffer.from(await res.arrayBuffer())
      const got = sha256(bytes)
      if (got !== font.sha256) throw new Error(`sha256 ${got}, pinned ${font.sha256}`)
      writeFileSync(path, bytes)
      console.log(`fetch-web-fonts: ${font.name} ${bytes.length} bytes`)
    } catch (error) {
      console.error(`fetch-web-fonts: ${font.name} from ${font.url}: ${error.message}`)
      failed.push(font.name)
    }
  }
  return failed
}

if (process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
  const i = process.argv.indexOf('--out')
  if (i === -1) {
    console.error('usage: fetch-web-fonts.mjs --out DIR')
    process.exit(2)
  }
  const failed = await fetchFonts(process.argv[i + 1])
  process.exit(failed.length === 0 ? 0 : 1)
}
