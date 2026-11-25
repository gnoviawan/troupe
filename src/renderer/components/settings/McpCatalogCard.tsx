import type { McpCatalogRow, McpCatalogState } from '@shared/types/mcp-catalog.types'
import { useVirtualizer } from '@tanstack/react-virtual'
import { useEffect, useId, useRef, useState } from 'react'
import { toast } from 'sonner'
import { Check, ExternalLink } from '@/components/icons'
import { McpCatalogIcon } from '@/components/settings/McpCatalogIcon'
import { SETTINGS_CARD_CLASS } from '@/components/settings/SettingsGroup'
import { type StatusTone, StatusWord } from '@/components/settings/StatusWord'
import { Button } from '@/components/ui/button'
import { openerApi } from '@/lib/api'
import {
  MCP_CATALOG_STATE_LABEL,
  MCP_INSTALL_BLOCK_TEXT,
  type McpInstallBlock,
  mcpCatalogReasonText
} from '@/lib/mcp-catalog-copy'
import { cn } from '@/lib/utils'

const STATE_TONE: Record<McpCatalogState, StatusTone> = {
  installable: 'success',
  'needs-runtime': 'warning',
  unsupported: 'muted',
  'source-only': 'muted'
}

/** A card with a two-line description, before measuring. */
const ESTIMATED_CARD_HEIGHT = 140
/** Space between cards, across and down. */
const CARD_GAP = 12
/** The grid goes to two columns from this width. */
const TWO_COLUMNS_FROM = 520
/** Fetch the next page once the last rendered card is this close to the end. */
const LOAD_AHEAD_CARDS = 8

export interface CatalogInstall {
  installed: ReadonlySet<string>
  block: { reason: McpInstallBlock; hintId: string } | null
  onInstall: (row: McpCatalogRow) => void
}

function openRepository(url: string): void {
  void openerApi.openUrlWithSystemBrowser(url).then((result) => {
    if (!result.success) toast.error("Couldn't open the repository link.")
  })
}

/**
 * One catalog server as a card: logo, name and the one action (Install, a
 * tick when added, or a Deprecated badge), a two-line description, the
 * registry name, then its state with the reason when it cannot install.
 */
function CatalogCard({
  row,
  install
}: {
  row: McpCatalogRow
  install: CatalogInstall
}): React.JSX.Element {
  const reasonId = useId()
  const displayName = row.title ?? row.name.slice(row.name.lastIndexOf('/') + 1)
  const installed = install.installed.has(row.name)
  const reason = !installed && row.reason ? mcpCatalogReasonText(row.reason) : null
  return (
    <div className={cn('flex h-full flex-col gap-2.5 p-3.5', SETTINGS_CARD_CLASS)}>
      <div className="flex min-w-0 items-center gap-2.5">
        <McpCatalogIcon name={row.name} displayName={displayName} hasIcon={row.hasIcon} />
        <div className="min-w-0 flex-1">
          <div className="flex min-w-0 items-center gap-1.5">
            <span className="min-w-0 truncate text-sm font-medium text-foreground">
              {displayName}
            </span>
            {row.status === 'deprecated' && (
              <span className="shrink-0 rounded-full bg-warning/10 px-1.5 text-3xs font-medium leading-5 text-warning">
                Deprecated
              </span>
            )}
          </div>
        </div>
        {installed ? (
          <Check size={14} className="shrink-0 text-success" aria-label="Added" />
        ) : (
          <span
            title={
              install.block ? MCP_INSTALL_BLOCK_TEXT[install.block.reason] : (reason ?? undefined)
            }
          >
            <Button
              type="button"
              size="xs"
              variant="secondary"
              disabled={install.block !== null || row.state !== 'installable'}
              aria-describedby={
                install.block ? install.block.hintId : reason ? reasonId : undefined
              }
              onClick={() => install.onInstall(row)}
            >
              Install
            </Button>
          </span>
        )}
      </div>
      {row.description && (
        <p className="line-clamp-2 min-h-8 break-words text-xs leading-4 text-secondary-foreground">
          {row.description}
        </p>
      )}
      <div className="mt-auto flex min-w-0 items-center gap-2">
        <div className="min-w-0 flex-1">
          <p
            className="fade-end font-mono text-xs leading-4 text-muted-foreground"
            title={row.name}
          >
            {row.name}
          </p>
          <div className="mt-1 flex flex-wrap items-baseline gap-x-2 gap-y-0.5">
            {installed ? (
              <StatusWord tone="success">Installed</StatusWord>
            ) : (
              <StatusWord tone={STATE_TONE[row.state]}>
                {MCP_CATALOG_STATE_LABEL[row.state]}
              </StatusWord>
            )}
            <span className="text-xs tabular-nums text-muted-foreground">{row.version}</span>
          </div>
          {reason && (
            <p id={reasonId} className="mt-1 text-xs leading-4 text-muted-foreground">
              {reason}
            </p>
          )}
        </div>
        {row.repositoryUrl && (
          <Button
            type="button"
            size="icon-sm"
            variant="ghost"
            className="size-7 shrink-0 self-end text-muted-foreground"
            aria-label={`Open the ${displayName} repository`}
            title="Repository"
            onClick={() => openRepository(row.repositoryUrl as string)}
          >
            <ExternalLink size={13} />
          </Button>
        )}
      </div>
    </div>
  )
}

/** Columns for a container `width`: one when narrow, else two. */
function useColumns(ref: React.RefObject<HTMLElement>): number {
  const [columns, setColumns] = useState(2)
  useEffect(() => {
    const element = ref.current
    if (!element || typeof ResizeObserver === 'undefined') return
    const observer = new ResizeObserver(([entry]) => {
      const width = entry?.contentRect.width ?? 0
      if (width > 0) setColumns(width >= TWO_COLUMNS_FROM ? 2 : 1)
    })
    observer.observe(element)
    return () => observer.disconnect()
  }, [ref])
  return columns
}

/**
 * The catalog as a virtualised grid of cards (two columns, one on a narrow
 * page). It asks for the next page when the last cards come into view.
 */
export function CatalogGrid({
  rows,
  install,
  onNearEnd
}: {
  rows: McpCatalogRow[]
  install: CatalogInstall
  /** Absent while a page is loading or the last one failed. */
  onNearEnd: (() => void) | undefined
}): React.JSX.Element {
  const scrollRef = useRef<HTMLDivElement>(null)
  const columns = useColumns(scrollRef)
  const virtualizer = useVirtualizer({
    count: rows.length,
    lanes: columns,
    gap: CARD_GAP,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ESTIMATED_CARD_HEIGHT,
    overscan: 6,
    getItemKey: (index) => rows[index]?.name ?? index
  })
  const items = virtualizer.getVirtualItems()
  const lastRendered = items.at(-1)?.index

  useEffect(() => {
    if (lastRendered !== undefined && lastRendered >= rows.length - LOAD_AHEAD_CARDS) onNearEnd?.()
  }, [lastRendered, rows.length, onNearEnd])

  const width = columns === 2 ? `calc(50% - ${CARD_GAP / 2}px)` : '100%'
  return (
    <div
      ref={scrollRef}
      data-testid="mcp-catalog-scroll"
      className="-m-1 max-h-[min(40rem,70dvh)] overflow-y-auto overscroll-contain p-1"
    >
      <div className="relative w-full" style={{ height: virtualizer.getTotalSize() }}>
        {items.map((item) => {
          const row = rows[item.index]
          if (!row) return null
          return (
            <div
              key={item.key}
              data-index={item.index}
              ref={virtualizer.measureElement}
              className="absolute top-0"
              style={{
                width,
                left: item.lane === 0 ? 0 : `calc(50% + ${CARD_GAP / 2}px)`,
                transform: `translateY(${item.start}px)`
              }}
            >
              <CatalogCard row={row} install={install} />
            </div>
          )
        })}
      </div>
    </div>
  )
}
