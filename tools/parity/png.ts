/**
 * PNG read and write through the pngjs copy that Playwright bundles. The repo
 * has no direct pngjs dependency, and `playwright-core/lib/utilsBundle` is a
 * public subpath in Playwright's package exports.
 */

import { readFileSync, writeFileSync } from 'node:fs'
import { PNG as BundledPng } from 'playwright-core/lib/utilsBundle'

export interface Image {
  width: number
  height: number
  /** RGBA, 4 bytes per pixel, row-major. */
  data: Uint8Array
}

interface PngModule {
  new (opts: { width: number; height: number }): Image
  sync: { read(buffer: Buffer): Image; write(png: Image): Buffer }
}

const PNG = BundledPng as unknown as PngModule

export function readPng(file: string): Image {
  return PNG.sync.read(readFileSync(file))
}

export function writePng(file: string, image: Image): void {
  const png = new PNG({ width: image.width, height: image.height })
  png.data.set(image.data)
  writeFileSync(file, PNG.sync.write(png))
}

export function blankImage(width: number, height: number, fill = 0): Image {
  return { width, height, data: new Uint8Array(width * height * 4).fill(fill) }
}

/** Copy the `width`x`height` block at `sx`,`sy` of `src` to `dx`,`dy` of `dst`. */
export function blit(
  src: Image,
  dst: Image,
  sx: number,
  sy: number,
  width: number,
  height: number,
  dx: number,
  dy: number
): void {
  for (let row = 0; row < height; row++) {
    const from = ((sy + row) * src.width + sx) * 4
    dst.data.set(src.data.subarray(from, from + width * 4), ((dy + row) * dst.width + dx) * 4)
  }
}
