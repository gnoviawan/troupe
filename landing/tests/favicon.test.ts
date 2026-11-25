import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, test } from 'bun:test';

const root = join(import.meta.dir, '..');

describe('landing favicon', () => {
  test('the page links the ico fallback and the svg icon', () => {
    const html = readFileSync(join(root, 'index.html'), 'utf8');
    expect(html).toContain('<link rel="icon" href="/favicon.ico" sizes="any" />');
    expect(html).toContain('<link rel="icon" type="image/svg+xml" href="/favicon.svg" />');
    expect(html).toContain('<link rel="apple-touch-icon" href="/apple-touch-icon.png" />');
  });

  test('favicon.ico is an icon file with 16 and 32 pixel images', () => {
    const ico = readFileSync(join(root, 'public/favicon.ico'));
    expect(ico.subarray(0, 4)).toEqual(Buffer.from([0, 0, 1, 0]));
    const count = ico.readUInt16LE(4);
    const sizes: string[] = [];
    for (let i = 0; i < count; i++) {
      const width = ico[6 + i * 16] || 256;
      const height = ico[7 + i * 16] || 256;
      sizes.push(`${width}x${height}`);
    }
    expect(sizes).toContain('16x16');
    expect(sizes).toContain('32x32');
  });

  test('favicon.svg is a self-contained dark tile with a white mark', () => {
    const svg = readFileSync(join(root, 'public/favicon.svg'), 'utf8');
    expect(svg.includes('<!DOCTYPE')).toBe(false);
    expect(svg.includes('www.w3.org/Graphics/SVG')).toBe(false);
    expect(svg).toContain('xmlns="http://www.w3.org/2000/svg"');
    expect(svg).toContain('fill="#0f172a"');
    expect(svg).toContain('fill="#ffffff"');
  });
});
