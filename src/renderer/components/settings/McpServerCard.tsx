import { RefreshCw } from '@/components/icons'
import { McpCatalogIcon } from '@/components/settings/McpCatalogIcon'
import { SETTINGS_CARD_CLASS } from '@/components/settings/SettingsGroup'
import { StatusWord } from '@/components/settings/StatusWord'
import { Button } from '@/components/ui/button'
import { FOCUS_RING_CLASS } from '@/components/ui/panel-styles'
import { Switch } from '@/components/ui/switch'
import type { StoredMcpServer } from '@/lib/acp-mcp-persistence'
import { cn } from '@/lib/utils'
import { type McpServerStatus, mcpServerTarget } from './mcp-server-view'

interface McpServerCardProps {
  server: StoredMcpServer
  status: McpServerStatus
  /** "41 tools", once the tools are known. */
  toolCount: string | null
  /** Show Connect (the server needs a sign-in). */
  canConnect: boolean
  connecting: boolean
  onConnect: () => void
  /** Show Try again (the server is unreachable). */
  canRetry: boolean
  onRetry: () => void
  onToggle: (enabled: boolean) => void
  onOpen: () => void
}

/**
 * An added MCP server as a card: logo, name and its on/off switch, then the
 * command or URL it runs, then its state with the one action a problem needs
 * (Connect or Try again). The name opens the server's detail page; a click
 * anywhere else on the card that is not a control does the same.
 */
export function McpServerCard({
  server,
  status,
  toolCount,
  canConnect,
  connecting,
  onConnect,
  canRetry,
  onRetry,
  onToggle,
  onOpen
}: McpServerCardProps): React.JSX.Element {
  const enabled = server.enabled !== false
  const target = mcpServerTarget(server)
  return (
    // biome-ignore lint/a11y/useKeyWithClickEvents lint/a11y/noStaticElementInteractions: the name button is the keyboard path; the card click is a pointer shortcut
    <div
      data-testid={`mcp-server-card-${server.id}`}
      onClick={(event) => {
        if ((event.target as HTMLElement).closest('button, [role="switch"], a, input')) return
        onOpen()
      }}
      className={cn(
        'flex cursor-pointer flex-col gap-2.5 p-3.5 transition-colors duration-150 ease-out hover:bg-secondary',
        SETTINGS_CARD_CLASS
      )}
    >
      <div className="flex min-w-0 items-center gap-2.5">
        <McpCatalogIcon
          name={server.catalog?.name ?? null}
          displayName={server.name}
          hasIcon={server.catalog !== undefined}
        />
        <button
          type="button"
          onClick={onOpen}
          className={cn(
            'min-w-0 flex-1 truncate rounded-sm text-left text-sm font-medium',
            enabled ? 'text-foreground' : 'text-muted-foreground',
            FOCUS_RING_CLASS
          )}
        >
          {server.name}
        </button>
        <Switch
          checked={enabled}
          aria-label={`${enabled ? 'Disable' : 'Enable'} ${server.name}`}
          onCheckedChange={onToggle}
        />
      </div>
      <p
        className="line-clamp-2 min-h-8 break-all font-mono text-xs leading-4 text-secondary-foreground"
        title={target}
      >
        {target}
      </p>
      <div className="flex h-6 min-w-0 items-center gap-2">
        <StatusWord tone={status.tone} pending={status.pending}>
          {status.label}
        </StatusWord>
        {status.tone === 'success' && toolCount && (
          <span className="truncate text-xs tabular-nums text-muted-foreground">· {toolCount}</span>
        )}
        <span className="flex-1" />
        {canConnect && (
          <Button
            type="button"
            size="xs"
            variant="secondary"
            className="h-6"
            disabled={connecting}
            onClick={onConnect}
          >
            {connecting ? 'Connecting…' : 'Connect'}
          </Button>
        )}
        {canRetry && (
          <Button type="button" size="xs" variant="secondary" className="h-6" onClick={onRetry}>
            <RefreshCw size={12} />
            Try again
          </Button>
        )}
      </div>
    </div>
  )
}
