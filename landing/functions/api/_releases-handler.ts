import { fetchReleases, type ReleasesResponse } from './_releases';

const CACHE_SECONDS = 600;
const STALE_SECONDS = 3600;

function respond(body: ReleasesResponse, cacheHit: boolean): Response {
  return new Response(JSON.stringify(body), {
    headers: {
      'Content-Type': 'application/json; charset=utf-8',
      'Cache-Control': `public, max-age=60, s-maxage=${CACHE_SECONDS}, stale-while-revalidate=${STALE_SECONDS}`,
      'X-Releases-Cache': cacheHit ? 'hit' : 'miss',
    },
  });
}

/**
 * GET /api/releases: the newest stable and nightly builds, cached at the edge.
 * If GitHub fails, the last cached copy is served. With no cached copy, 503;
 * the page then keeps its build-time snapshot.
 */
export async function getReleases(request: Request, token?: string): Promise<Response> {
  const cache = (globalThis as unknown as { caches?: { default?: Cache } }).caches?.default;
  const cacheKey = new Request(new URL('/api/releases', request.url).toString());

  const cached = cache ? await cache.match(cacheKey) : undefined;
  if (cached) {
    const age = Number(cached.headers.get('X-Releases-Fetched') ?? 0);
    if (Date.now() - age < CACHE_SECONDS * 1000) {
      const body = (await cached.clone().json()) as ReleasesResponse;
      return respond(body, true);
    }
  }

  const fresh = await fetchReleases(fetch, token);
  if (!fresh.stable && !fresh.nightly) {
    if (cached) return respond((await cached.json()) as ReleasesResponse, true);
    return new Response(JSON.stringify({ error: 'Release data is not available' }), {
      status: 503,
      headers: { 'Content-Type': 'application/json; charset=utf-8', 'Cache-Control': 'no-store' },
    });
  }

  if (cache) {
    const stored = new Response(JSON.stringify(fresh), {
      headers: {
        'Content-Type': 'application/json; charset=utf-8',
        'Cache-Control': `public, max-age=${CACHE_SECONDS + STALE_SECONDS}`,
        'X-Releases-Fetched': String(Date.now()),
      },
    });
    await cache.put(cacheKey, stored);
  }
  return respond(fresh, false);
}
