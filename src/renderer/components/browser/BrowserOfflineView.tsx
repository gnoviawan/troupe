import { Plug, RefreshCw, Terminal } from '@/components/icons'
import { Button } from '@/components/ui/button'
import type { BrowserLoadError } from '@/stores/browser-session-store'

interface BrowserOfflineViewProps {
  error: BrowserLoadError
  onRetry: () => void
  onOpenTerminal?: () => void
}

export function BrowserOfflineView({
  error,
  onRetry,
  onOpenTerminal
}: BrowserOfflineViewProps): React.JSX.Element {
  const isLocal = error.host.startsWith('localhost') || error.host.startsWith('127.0.0.1')

  return (
    <div
      data-testid="browser-offline-view"
      className="flex h-full w-full items-center justify-center bg-card p-11"
    >
      <div className="flex w-full max-w-[380px] flex-col gap-4">
        <div className="flex flex-col gap-2.5">
          <Plug size={20} className="text-muted-foreground" aria-hidden="true" />
          <div className="flex flex-col gap-1">
            <h2 className="text-base font-medium leading-6 text-foreground">
              Cannot connect to {error.host}
            </h2>
            <p className="text-sm leading-5 text-muted-foreground">
              {error.message ||
                (error.port
                  ? `No local server is listening on port ${error.port}. Start your dev server or check the port number.`
                  : 'Check that the server is running and reachable.')}
            </p>
          </div>
          <span className="font-mono text-xs text-muted-foreground">
            {error.code}
            {error.port ? ` · 127.0.0.1:${error.port}` : ''}
          </span>
        </div>

        <div className="flex items-center gap-2">
          {onOpenTerminal && isLocal && (
            <Button type="button" variant="outline" size="sm" onClick={onOpenTerminal}>
              <Terminal size={14} className="text-secondary-foreground" aria-hidden="true" />
              <span>Open terminal</span>
            </Button>
          )}
          <Button type="button" variant="default" size="sm" onClick={onRetry}>
            <RefreshCw size={14} aria-hidden="true" />
            <span>Retry</span>
          </Button>
        </div>

        {isLocal && error.port && (
          <div className="flex items-center gap-2 text-xs font-medium text-muted-foreground">
            <span className="size-1.5 rounded-full bg-muted-foreground" />
            <span>Waiting for port {error.port}. Reloads automatically when ready.</span>
          </div>
        )}
      </div>
    </div>
  )
}
