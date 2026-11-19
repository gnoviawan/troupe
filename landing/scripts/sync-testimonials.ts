#!/usr/bin/env bun

/**
 * Build-time testimonial snapshot.
 *
 * 1. Reads the approved testimonials from the live database
 *    (GET https://usetroupe.dev/api/testimonials).
 * 2. Saves each avatar as a small 96×96 WebP in public/media/testimonials/
 *    (the page shows them at 32px; uploads can be 1–2 MB photos).
 * 3. Writes src/data/testimonials.snapshot.json with the local avatar paths.
 *
 * The prerendered homepage shows this snapshot; after hydration the page asks
 * /api/testimonials again for newer approvals. A network or API error keeps
 * the existing snapshot and avatar files, so the build never fails because
 * the API is down.
 */

import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import sharp from 'sharp';

import type { PublicTestimonial } from '../src/types/testimonials';

const __dirname = dirname(fileURLToPath(import.meta.url));
const SNAPSHOT_FILE = resolve(__dirname, '../src/data/testimonials.snapshot.json');
const AVATAR_DIR = resolve(__dirname, '../public/media/testimonials');
const AVATAR_PUBLIC_PATH = '/media/testimonials';
const SITE = process.env.TESTIMONIALS_SITE ?? 'https://usetroupe.dev';
const AVATAR_PX = 96; // 3× the 32px avatar
const HEADERS = { 'User-Agent': 'usetroupe.dev build' };

function isTestimonial(value: unknown): value is PublicTestimonial {
  if (!value || typeof value !== 'object') return false;
  const item = value as Record<string, unknown>;
  return ['id', 'quote', 'name', 'role', 'avatarUrl'].every((key) => typeof item[key] === 'string');
}

/** Safe file name from the testimonial id (a UUID). */
function avatarFileName(id: string): string {
  return `${id.replace(/[^a-zA-Z0-9-]/g, '')}.webp`;
}

/** Downloads and shrinks one avatar. Returns the local public path, or null. */
async function saveSmallAvatar(item: PublicTestimonial): Promise<string | null> {
  if (!item.avatarUrl) return null;
  const file = resolve(AVATAR_DIR, avatarFileName(item.id));
  const publicPath = `${AVATAR_PUBLIC_PATH}/${avatarFileName(item.id)}`;
  try {
    const url = new URL(item.avatarUrl, SITE);
    const response = await fetch(url, { headers: HEADERS });
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    const input = Buffer.from(await response.arrayBuffer());
    const output = await sharp(input, { animated: false })
      .rotate()
      .resize(AVATAR_PX, AVATAR_PX, { fit: 'cover', position: 'attention' })
      .webp({ quality: 80 })
      .toBuffer();
    writeFileSync(file, output);
    return publicPath;
  } catch (error) {
    console.warn(`[sync-testimonials] Avatar for ${item.id} failed:`, error);
    // Keep an earlier copy if there is one.
    return existsSync(file) ? publicPath : null;
  }
}

async function main(): Promise<void> {
  const previous: PublicTestimonial[] = existsSync(SNAPSHOT_FILE)
    ? (JSON.parse(readFileSync(SNAPSHOT_FILE, 'utf8')) as PublicTestimonial[])
    : [];

  let list: PublicTestimonial[] | null = null;
  try {
    const response = await fetch(`${SITE}/api/testimonials`, { headers: { ...HEADERS, Accept: 'application/json' } });
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    const payload = (await response.json()) as { testimonials?: unknown };
    const approved = Array.isArray(payload.testimonials) ? payload.testimonials.filter(isTestimonial) : [];
    if (approved.length > 0) list = approved;
    else console.warn('[sync-testimonials] The API returned no approved testimonials. Keeping the snapshot.');
  } catch (error) {
    console.warn('[sync-testimonials] Request failed. Keeping the snapshot:', error);
  }

  if (!list) {
    console.log(`[sync-testimonials] ${previous.length} testimonials (unchanged)`);
    return;
  }

  mkdirSync(AVATAR_DIR, { recursive: true });
  const next: PublicTestimonial[] = [];
  for (const item of list) {
    const local = await saveSmallAvatar(item);
    // No local copy: keep the API URL, so the avatar still shows.
    next.push({ ...item, avatarUrl: local ?? item.avatarUrl });
  }

  // Remove avatars of testimonials that are no longer approved.
  const keep = new Set(next.map((item) => avatarFileName(item.id)));
  for (const name of readdirSync(AVATAR_DIR)) {
    if (name.endsWith('.webp') && !keep.has(name)) rmSync(resolve(AVATAR_DIR, name));
  }

  writeFileSync(SNAPSHOT_FILE, `${JSON.stringify(next, null, 2)}\n`);
  const local = next.filter((item) => item.avatarUrl.startsWith(AVATAR_PUBLIC_PATH)).length;
  console.log(`[sync-testimonials] ${next.length} approved testimonials, ${local} small avatars`);
}

await main();
