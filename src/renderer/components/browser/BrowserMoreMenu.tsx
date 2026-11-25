import { useState } from 'react'
import { Bug, ExternalLink, Minus, MoreHorizontal, Plus, RotateCcw } from '@/components/icons'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { cn } from '@/lib/utils'
import { useBrowserSessionStore } from '@/stores/browser-session-store'

interface BrowserMoreMenuProps {
  browserTabId: string
  hasUrl: boolean
  onCopyAddress: () => void
  onOpenExternal: () => void
  onHardReload: () => void
  onOpenDevtools?: () => void
}

export function BrowserMoreMenu({
  browserTabId,
  hasUrl,
  onCopyAddress,
  onOpenExternal,
  onHardReload,
  onOpenDevtools
}: BrowserMoreMenuProps): React.JSX.Element {
  const [open, setOpen] = useState(false)
  const zoomFactor = useBrowserSessionStore((s) => s.tabs.get(browserTabId)?.zoomFactor ?? 1)
  const stepZoom = useBrowserSessionStore((s) => s.stepZoom)
  const resetZoom = useBrowserSessionStore((s) => s.resetZoom)

  const percent = Math.round(zoomFactor * 100)

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          type="button"
          aria-label="More browser actions"
          title="More"
          className={cn(
            'flex size-7 shrink-0 items-center justify-center rounded-md transition-colors duration-150 active:scale-[0.95]',
            open
              ? 'tactile-control bg-muted text-foreground'
              : 'text-muted-foreground hover:bg-muted/60 hover:text-foreground'
          )}
        >
          <MoreHorizontal size={14} />
        </button>
      </PopoverTrigger>
      <PopoverContent
        align="end"
        sideOffset={6}
        className="t-dropdown tactile-menu w-60 rounded-lg border-0 bg-popover px-0 py-1"
      >
        <div className="flex h-[34px] items-center justify-between px-3">
          <span className="text-[13px] font-medium text-foreground">Zoom</span>
          <div className="flex items-center gap-1">
            <button
              type="button"
              aria-label="Zoom out"
              disabled={!hasUrl || zoomFactor <= 0.5}
              onClick={() => stepZoom(browserTabId, 'out')}
              className="tactile-control flex size-6 items-center justify-center rounded-md bg-muted text-foreground transition-colors duration-150 hover:bg-secondary disabled:opacity-40"
            >
              <Minus size={12} />
            </button>
            <button
              type="button"
              aria-label="Reset zoom"
              title="Reset zoom to 100%"
              disabled={!hasUrl || percent === 100}
              onClick={() => resetZoom(browserTabId)}
              className="w-11 text-center font-mono text-xs font-medium tabular-nums text-foreground hover:underline disabled:no-underline disabled:opacity-75"
            >
              {percent}%
            </button>
            <button
              type="button"
              aria-label="Zoom in"
              disabled={!hasUrl || zoomFactor >= 2}
              onClick={() => stepZoom(browserTabId, 'in')}
              className="tactile-control flex size-6 items-center justify-center rounded-md bg-muted text-foreground transition-colors duration-150 hover:bg-secondary disabled:opacity-40"
            >
              <Plus size={12} />
            </button>
          </div>
        </div>

        <div className="my-1 h-[0.5px] w-full bg-border" />

        <div className="flex flex-col gap-0.5 px-1">
          <button
            type="button"
            disabled={!hasUrl}
            onClick={() => {
              onCopyAddress()
              setOpen(false)
            }}
            className="flex h-[30px] items-center justify-between rounded-md px-2 text-[13px] font-medium text-secondary-foreground transition-colors duration-150 hover:bg-muted hover:text-foreground disabled:opacity-40"
          >
            <span>Copy address</span>
            <span className="font-mono text-xs text-muted-foreground">⌘L</span>
          </button>

          <button
            type="button"
            disabled={!hasUrl}
            onClick={() => {
              onOpenExternal()
              setOpen(false)
            }}
            className="flex h-[30px] items-center justify-between rounded-md px-2 text-[13px] font-medium text-secondary-foreground transition-colors duration-150 hover:bg-muted hover:text-foreground disabled:opacity-40"
          >
            <span>Open in default browser</span>
            <ExternalLink size={12} className="text-muted-foreground" aria-hidden="true" />
          </button>
        </div>

        <div className="my-1 h-[0.5px] w-full bg-border" />

        <div className="flex flex-col gap-0.5 px-1">
          <button
            type="button"
            disabled={!hasUrl}
            onClick={() => {
              onHardReload()
              setOpen(false)
            }}
            className="flex h-[30px] items-center justify-between rounded-md px-2 text-[13px] font-medium text-secondary-foreground transition-colors duration-150 hover:bg-muted hover:text-foreground disabled:opacity-40"
          >
            <span className="inline-flex items-center gap-1.5">
              <RotateCcw size={12} className="text-muted-foreground" aria-hidden="true" />
              Hard reload
            </span>
            <span className="font-mono text-xs text-muted-foreground">⇧⌘R</span>
          </button>

          {onOpenDevtools && (
            <button
              type="button"
              disabled={!hasUrl}
              aria-label="Open debug console"
              onClick={() => {
                onOpenDevtools()
                setOpen(false)
              }}
              className="flex h-[30px] items-center justify-between rounded-md px-2 text-[13px] font-medium text-muted-foreground transition-colors duration-150 hover:bg-muted hover:text-foreground disabled:opacity-40"
            >
              <span className="inline-flex items-center gap-1.5">
                <Bug size={12} aria-hidden="true" />
                Debug console (dev)
              </span>
            </button>
          )}
        </div>
      </PopoverContent>
    </Popover>
  )
}
