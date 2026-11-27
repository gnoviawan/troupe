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
/** The grid goes to three columns from this width. */
const THREE_COLUMNS_FROM = 860

/** Columns for a catalog grid container width: 1 narrow, 2 medium, 3 wide. */
export function catalogColumnsForWidth(width: number): 1 | 2 | 3 {
  if (width >= THREE_COLUMNS_FROM) return 3
  if (width >= TWO_COLUMNS_FROM) return 2
  return 1
}
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

/** Columns for a container `width`: one when narrow, two medium, three wide. */
function useColumns(ref: React.RefObject<HTMLElement>): 1 | 2 | 3 {
  const [columns, setColumns] = useState<1 | 2 | 3>(2)
  useEffect(() => {
    const element = ref.current
    if (!element || typeof ResizeObserver === 'undefined') return
    const observer = new ResizeObserver(([entry]) => {
      const width = entry?.contentRect.width ?? 0
      if (width > 0) setColumns(catalogColumnsForWidth(width))
    })
    observer.observe(element)
    return () => observer.disconnect()
  }, [ref])
  return columns
}

/**
 * The catalog as a virtualised grid of cards (one, two, or three columns by
 * available width). It virtualises rows, not cards, so the cards in a row
 * share one height and the rows line up. It asks for the next page when the
 * last cards come into view.
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
    count: Math.ceil(rows.length / columns),
    gap: CARD_GAP,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ESTIMATED_CARD_HEIGHT,
    overscan: 3,
    getItemKey: (index) => rows[index * columns]?.name ?? index
  })
  const items = virtualizer.getVirtualItems()
  const lastRenderedRow = items.at(-1)?.index
  const lastRenderedCard =
    lastRenderedRow === undefined ? undefined : (lastRenderedRow + 1) * columns - 1

  useEffect(() => {
    if (lastRenderedCard !== undefined && lastRenderedCard >= rows.length - LOAD_AHEAD_CARDS)
      onNearEnd?.()
  }, [lastRenderedCard, rows.length, onNearEnd])

  const cardWidth =
    columns === 1 ? '100%' : `calc((100% - ${(columns - 1) * CARD_GAP}px) / ${columns})`
  return (
    <div
      ref={scrollRef}
      data-testid="mcp-catalog-scroll"
      className="-m-1 max-h-[min(40rem,70dvh)] overflow-y-auto overscroll-contain p-1"
    >
      <div className="relative w-full" style={{ height: virtualizer.getTotalSize() }}>
        {items.map((item) => {
          const first = item.index * columns
          const cards = rows.slice(first, first + columns)
          if (cards.length === 0) return null
          return (
            <div
              key={item.key}
              data-index={item.index}
              ref={virtualizer.measureElement}
              className="absolute left-0 top-0 flex w-full items-stretch"
              style={{ gap: CARD_GAP, transform: `translateY(${item.start}px)` }}
            >
              {cards.map((row, offset) => (
                <div
                  key={row.name}
                  data-card-index={first + offset}
                  className="min-w-0 shrink-0"
                  style={{ width: cardWidth }}
                >
                  <CatalogCard row={row} install={install} />
                </div>
              ))}
            </div>
          )
        })}
      </div>
    </div>
  )
}
