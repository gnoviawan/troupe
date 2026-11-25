import { Plug, RotateCcw, Terminal } from '@/components/icons'
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
            <p className="text-[13px] font-medium leading-5 text-muted-foreground">
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
            <button
              type="button"
              onClick={onOpenTerminal}
              className="tactile-control inline-flex h-8 items-center gap-1.5 rounded-md bg-muted px-3 text-[13px] font-medium tracking-[-0.01em] text-foreground transition-[transform,background-color] duration-150 hover:bg-secondary active:scale-[0.97]"
            >
              <Terminal size={14} className="text-secondary-foreground" aria-hidden="true" />
              <span>Open terminal</span>
            </button>
          )}
          <button
            type="button"
            onClick={onRetry}
            className="inline-flex h-8 items-center gap-1.5 rounded-md bg-primary-fill px-3 text-[13px] font-medium tracking-[-0.01em] text-primary-foreground shadow-[0_0_0_0.5px_var(--color-primary-fill),0_1px_2px_rgba(0,0,0,0.5)] transition-[transform,background-color] duration-150 hover:bg-primary-fill-hover active:scale-[0.97]"
          >
            <RotateCcw size={14} aria-hidden="true" />
            <span>Retry</span>
          </button>
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
