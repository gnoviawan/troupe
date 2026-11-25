export interface DisplayUrlParts {
  scheme: string
  host: string
  path: string
  isHttps: boolean
}

export function normalizeBrowserInputUrl(raw: string): string | null {
  const trimmed = raw.trim()
  if (!trimmed) return null
  if (/^https?:\/\//i.test(trimmed) || /^about:/i.test(trimmed)) return trimmed
  if (/^(localhost|127\.0\.0\.1)(:\d+)?([/?#].*)?$/i.test(trimmed)) {
    return `http://${trimmed}`
  }
  if (/^:\d+([/?#].*)?$/.test(trimmed)) {
    return `http://localhost${trimmed}`
  }
  return `https://${trimmed}`
}

export function splitDisplayUrl(raw: string): DisplayUrlParts | null {
  if (!raw) return null
  try {
    const parsed = new URL(raw)
    const scheme = `${parsed.protocol}//`
    const host = parsed.host
    const path = `${parsed.pathname}${parsed.search}${parsed.hash}`
    return {
      scheme,
      host,
      path: path === '/' ? '' : path,
      isHttps: parsed.protocol === 'https:'
    }
  } catch {
    return null
  }
}

export function parseLocalhostPort(rawUrl: string): { host: string; port: number } | null {
  try {
    const parsed = new URL(rawUrl)
    if (parsed.hostname !== 'localhost' && parsed.hostname !== '127.0.0.1') return null
    const port = parsed.port ? Number(parsed.port) : parsed.protocol === 'https:' ? 443 : 80
    if (!Number.isInteger(port) || port <= 0) return null
    return { host: parsed.host, port }
  } catch {
    return null
  }
}

export function displayHost(rawUrl: string): string {
  try {
    return new URL(rawUrl).host
  } catch {
    return rawUrl
  }
}
