import { useEffect, useState } from 'react'
import { Globe, Lock } from '@/components/icons'
import { FOCUS_RING_CLASS, INNER_FIELD_FOCUS_CLASS } from '@/components/ui/panel-styles'
import { browserListLocalServers, type LocalDevServer } from '@/lib/browser-api'
import { normalizeBrowserInputUrl } from '@/lib/browser-url'
import { isMac } from '@/lib/platform'
import { cn } from '@/lib/utils'
import { useBrowserSessionStore } from '@/stores/browser-session-store'

interface BrowserStartPageProps {
  onNavigate: (url: string) => void
}

function formatRelativeAge(timestamp: number): string {
  const diffMin = Math.max(1, Math.round((Date.now() - timestamp) / 60_000))
  if (diffMin < 60) return `${diffMin}m ago`
  const diffHr = Math.round(diffMin / 60)
  if (diffHr < 24) return `${diffHr}h ago`
  return `${Math.round(diffHr / 24)}d ago`
}

export function BrowserStartPage({ onNavigate }: BrowserStartPageProps): React.JSX.Element {
  const [query, setQuery] = useState('')
  const [servers, setServers] = useState<LocalDevServer[]>([])
  const recentUrls = useBrowserSessionStore((s) => s.recentUrls)
  const clearRecentUrls = useBrowserSessionStore((s) => s.clearRecentUrls)

  useEffect(() => {
    let active = true
    void browserListLocalServers().then((res) => {
      if (active && res.success) {
        setServers(res.data)
      }
    })
    return () => {
      active = false
    }
  }, [])

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault()
    const normalized = normalizeBrowserInputUrl(query)
    if (normalized) onNavigate(normalized)
  }

  return (
    <div
      data-testid="browser-start-page"
      className="flex h-full w-full flex-col items-center justify-between overflow-y-auto bg-background px-6 pt-20 pb-7"
    >
      <div className="flex w-full max-w-[600px] flex-col gap-9">
        <div className="flex flex-col gap-4">
          <h1 className="text-xl font-medium tracking-[-0.02em] text-foreground">
            Open a local server or URL
          </h1>

          <form onSubmit={handleSubmit}>
            <div className="flex h-10 items-center gap-2.5 rounded-lg bg-muted px-3.5 ring-2 ring-focus-ring">
              <Globe size={15} className="shrink-0 text-secondary-foreground" aria-hidden="true" />
              <input
                type="text"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="Enter a URL or localhost port…"
                aria-label="Open URL or localhost port"
                className={cn(
                  'min-w-0 flex-1 bg-transparent text-sm pointer-coarse:text-base font-medium text-foreground placeholder:text-muted-foreground',
                  INNER_FIELD_FOCUS_CLASS
                )}
              />
            </div>
          </form>
        </div>

        {servers.length > 0 && (
          <section className="flex flex-col gap-3">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <h2 className="text-sm font-medium text-foreground">Detected local servers</h2>
                <span className="inline-flex h-5 items-center gap-1.5 rounded-full bg-success/15 px-2 text-xs font-medium text-success">
                  <span className="size-1.5 rounded-full bg-success" />
                  {servers.length} running
                </span>
              </div>
              <span className="text-xs font-medium text-muted-foreground">Local dev servers</span>
            </div>

            <div className="divide-y divide-border/70 overflow-hidden rounded-lg border border-border bg-card shadow-sm">
              {servers.map((server) => (
                <button
                  key={server.port}
                  type="button"
                  onClick={() => onNavigate(server.url)}
                  className={cn(
                    'flex h-[42px] w-full items-center justify-between px-3.5 text-left transition-colors duration-150 ease-out hover:bg-foreground/[0.04]',
                    FOCUS_RING_CLASS
                  )}
                >
                  <span className="flex items-center gap-2.5">
                    <span className="size-[7px] shrink-0 rounded-full bg-success" />
                    <span className="font-mono text-xs font-medium text-foreground">
                      {server.host}
                    </span>
                  </span>
                  <span className="text-xs font-medium text-muted-foreground">{server.label}</span>
                </button>
              ))}
            </div>
          </section>
        )}

        {recentUrls.length > 0 && (
          <section className="flex flex-col gap-2">
            <div className="flex items-center justify-between px-2">
              <h2 className="text-sm font-medium text-foreground">Recent pages</h2>
              <button
                type="button"
                onClick={clearRecentUrls}
                className={cn(
                  'rounded text-xs font-medium text-muted-foreground hover:text-foreground',
                  FOCUS_RING_CLASS
                )}
              >
                Clear
              </button>
            </div>

            <div className="flex flex-col gap-0.5">
              {recentUrls.map((entry) => {
                const isHttps = entry.url.startsWith('https://')
                return (
                  <button
                    key={entry.url}
                    type="button"
                    onClick={() => onNavigate(entry.url)}
                    className={cn(
                      'flex h-9 w-full items-center justify-between gap-3 rounded-md px-2 text-left transition-colors duration-150 ease-out hover:bg-foreground/[0.04]',
                      FOCUS_RING_CLASS
                    )}
                  >
                    <span className="flex min-w-0 items-center gap-2">
                      {isHttps ? (
                        <Lock
                          size={13}
                          className="shrink-0 text-muted-foreground"
                          aria-hidden="true"
                        />
                      ) : (
                        <Globe
                          size={13}
                          className="shrink-0 text-muted-foreground"
                          aria-hidden="true"
                        />
                      )}
                      <span className="truncate text-xs font-medium text-foreground">
                        {entry.host}
                      </span>
                      {entry.path && (
                        <span className="truncate text-xs font-medium text-muted-foreground">
                          {entry.path}
                        </span>
                      )}
                    </span>
                    <span className="shrink-0 font-mono text-xs text-muted-foreground">
                      {formatRelativeAge(entry.visitedAt)}
                    </span>
                  </button>
                )
              })}
            </div>
          </section>
        )}
      </div>

      <div className="flex items-center gap-4 pt-6 font-mono text-xs text-muted-foreground">
        <span>{isMac ? '⌘L' : 'Ctrl+L'} Focus URL</span>
        <span>{isMac ? '⇧⌘R' : 'Ctrl+Shift+R'} Reload</span>
        <span>{isMac ? '⌘W' : 'Ctrl+W'} Close tab</span>
      </div>
    </div>
  )
}
