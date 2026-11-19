import { describe, expect, test } from 'bun:test';

import {
  fetchReleases,
  parseNightlyVersion,
  pickAssets,
  toNightlyChannel,
  toStableChannel,
  type GitHubRelease,
} from '../functions/api/_releases';

const asset = (name: string) => ({ name, browser_download_url: `https://example.test/${name}` });

const stableRelease: GitHubRelease = {
  tag_name: 'v0.4.60',
  name: 'Termul Manager v0.4.60',
  published_at: '2026-11-07T05:44:20Z',
  html_url: 'https://github.com/gnoviawan/troupe/releases/tag/v0.4.60',
  assets: [
    asset('Termul.Manager_0.4.60_aarch64.dmg'),
    asset('Termul.Manager_0.4.60_x64.dmg'),
    asset('Termul.Manager_0.4.60_x64-setup.exe'),
    asset('Termul.Manager_0.4.60_x64-setup.exe.sig'),
    asset('Termul.Manager_0.4.60_amd64.AppImage'),
    asset('Termul.Manager_0.4.60_amd64.AppImage.sig'),
    asset('Termul.Manager_aarch64.app.tar.gz'),
  ],
};

const nightlyRelease: GitHubRelease = {
  tag_name: 'nightly',
  name: 'Troupe Nightly (0.5.0-nightly.20261119.108.3e31072)',
  published_at: '2026-08-09T17:21:09Z',
  html_url: 'https://github.com/gnoviawan/troupe/releases/tag/nightly',
  assets: [
    asset('Troupe_0.5.0-nightly.20261118.107.104308a_aarch64.dmg'),
    asset('Troupe_0.5.0-nightly.20261118.107.104308a_x64.dmg'),
    asset('Troupe_0.5.0-nightly.20261119.108.3e31072_aarch64.dmg'),
    asset('Troupe_0.5.0-nightly.20261119.108.3e31072_amd64.AppImage'),
    asset('latest-nightly.json'),
  ],
};

describe('pickAssets', () => {
  test('picks files by ending, not by product name', () => {
    const picked = pickAssets(stableRelease.assets);
    expect(picked.macArm).toEndWith('Termul.Manager_0.4.60_aarch64.dmg');
    expect(picked.macIntel).toEndWith('Termul.Manager_0.4.60_x64.dmg');
    expect(picked.windows).toEndWith('_x64-setup.exe');
    expect(picked.linuxAppImage).toEndWith('_amd64.AppImage');
  });

  test('ignores signature files', () => {
    expect(pickAssets([asset('a_x64-setup.exe.sig')]).windows).toBeUndefined();
  });
});

describe('parseNightlyVersion', () => {
  test('reads base, date, build and commit', () => {
    expect(parseNightlyVersion('Troupe_0.5.0-nightly.20261119.108.3e31072_aarch64.dmg')).toEqual({
      base: '0.5.0',
      date: '20261119',
      build: 108,
      commit: '3e31072',
      full: '0.5.0-nightly.20261119.108.3e31072',
    });
  });

  test('returns null for other names', () => {
    expect(parseNightlyVersion('latest-nightly.json')).toBeNull();
  });
});

describe('channels', () => {
  test('stable channel', () => {
    const channel = toStableChannel(stableRelease);
    expect(channel.version).toBe('0.4.60');
    expect(channel.tag).toBe('v0.4.60');
    expect(channel.publishedAt).toBe('2026-11-07T05:44:20Z');
  });

  test('nightly channel uses only the newest build', () => {
    const channel = toNightlyChannel(nightlyRelease);
    expect(channel).not.toBeNull();
    expect(channel?.version).toBe('0.5.0-nightly');
    expect(channel?.build).toBe(108);
    expect(channel?.commit).toBe('3e31072');
    expect(channel?.publishedAt).toBe('2026-11-19T00:00:00.000Z');
    expect(channel?.assets.macArm).toContain('20261119.108');
    // Build 108 has no Intel file in this fixture; build 107's must not leak in.
    expect(channel?.assets.macIntel).toBeUndefined();
  });

  test('nightly without versioned files falls back to the release name', () => {
    const channel = toNightlyChannel({ ...nightlyRelease, assets: [] });
    expect(channel?.build).toBe(108);
    expect(channel?.assets).toEqual({});
  });

  test('nightly with no version anywhere is null', () => {
    expect(toNightlyChannel({ ...nightlyRelease, name: 'Nightly', assets: [] })).toBeNull();
  });
});

describe('fetchReleases', () => {
  test('one failing channel does not hide the other', async () => {
    const fakeFetch = async (url: string) => {
      if (url.endsWith('/latest')) return new Response(JSON.stringify(stableRelease), { status: 200 });
      return new Response('rate limited', { status: 403 });
    };
    const result = await fetchReleases(fakeFetch);
    expect(result.stable?.version).toBe('0.4.60');
    expect(result.nightly).toBeNull();
  });

  test('network errors give null channels', async () => {
    const result = await fetchReleases(async () => {
      throw new Error('offline');
    });
    expect(result.stable).toBeNull();
    expect(result.nightly).toBeNull();
  });
});
