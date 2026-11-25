import { useCallback, useEffect, useRef, useState } from 'react'
import { Minus, Plus, ZoomIn, ZoomOut } from '@/components/icons'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { cn } from '@/lib/utils'
import { useBrowserSessionStore } from '@/stores/browser-session-store'

const ZOOM_PRESETS = [0.75, 1, 1.25, 1.5] as const

interface BrowserZoomControlProps {
  browserTabId: string
}

export function BrowserZoomControl({
  browserTabId
}: BrowserZoomControlProps): React.JSX.Element | null {
  const zoomFactor = useBrowserSessionStore((s) => s.tabs.get(browserTabId)?.zoomFactor ?? 1)
  const viewportPreset = useBrowserSessionStore(
    (s) => s.tabs.get(browserTabId)?.viewportPreset ?? 'desktop'
  )
  const stepZoom = useBrowserSessionStore((s) => s.stepZoom)
  const setZoomFactor = useBrowserSessionStore((s) => s.setZoomFactor)
  const resetZoom = useBrowserSessionStore((s) => s.resetZoom)

  const [open, setOpen] = useState(false)
  const prevZoomRef = useRef(zoomFactor)
  const dir = zoomFactor >= prevZoomRef.current ? 1 : -1

  useEffect(() => {
    prevZoomRef.current = zoomFactor
  }, [zoomFactor])

  const handleZoomOut = useCallback(() => {
    stepZoom(browserTabId, 'out')
  }, [browserTabId, stepZoom])

  const handleZoomIn = useCallback(() => {
    stepZoom(browserTabId, 'in')
  }, [browserTabId, stepZoom])

  // Hide pill at default 100% or while ResponsiveViewportBar already shows inline zoom.
  if (Math.abs(zoomFactor - 1) < 0.001 || viewportPreset !== 'desktop') {
    return null
  }

  const percent = Math.round(zoomFactor * 100)

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          type="button"
          aria-label={`Page zoom ${percent}%`}
          title="Adjust page zoom"
          className="tactile-control inline-flex h-[22px] shrink-0 items-center gap-1 rounded-full bg-muted px-2 font-mono text-xs font-medium text-foreground transition-colors duration-150 hover:bg-secondary active:scale-[0.97]"
        >
          {zoomFactor > 1 ? (
            <ZoomIn size={11} className="text-secondary-foreground" aria-hidden="true" />
          ) : (
            <ZoomOut size={11} className="text-secondary-foreground" aria-hidden="true" />
          )}
          <span
            key={percent}
            className="t-subtle-number t-subtle-digit"
            style={{ '--subtle-dir': dir } as React.CSSProperties}
          >
            {percent}%
          </span>
        </button>
      </PopoverTrigger>
      <PopoverContent
        align="end"
        sideOffset={6}
        className="t-dropdown tactile-menu w-64 rounded-lg border-0 bg-popover p-3"
      >
        <div className="flex flex-col gap-3">
          <div className="flex items-center justify-between">
            <span className="text-[13px] font-medium text-foreground">Page zoom</span>
            <button
              type="button"
              onClick={() => resetZoom(browserTabId)}
              className="text-xs font-medium text-secondary-foreground underline underline-offset-2 hover:text-foreground"
            >
              Reset
            </button>
          </div>

          <div className="flex items-center justify-between gap-2">
            <button
              type="button"
              onClick={handleZoomOut}
              aria-label="Zoom out"
              disabled={zoomFactor <= 0.5}
              className="tactile-control flex size-8 items-center justify-center rounded-md bg-muted text-foreground transition-[transform,background-color] duration-150 hover:bg-secondary active:scale-[0.95] disabled:opacity-40"
            >
              <Minus size={14} />
            </button>
            <div className="flex h-8 flex-1 items-center justify-center rounded-md bg-card font-mono text-[13px] font-medium tabular-nums text-foreground">
              {percent}%
            </div>
            <button
              type="button"
              onClick={handleZoomIn}
              aria-label="Zoom in"
              disabled={zoomFactor >= 2}
              className="tactile-control flex size-8 items-center justify-center rounded-md bg-muted text-foreground transition-[transform,background-color] duration-150 hover:bg-secondary active:scale-[0.95] disabled:opacity-40"
            >
              <Plus size={14} />
            </button>
          </div>

          <div className="flex items-center gap-1">
            {ZOOM_PRESETS.map((preset) => {
              const active = Math.abs(zoomFactor - preset) < 0.01
              return (
                <button
                  key={preset}
                  type="button"
                  onClick={() => setZoomFactor(browserTabId, preset)}
                  className={cn(
                    'flex h-[26px] flex-1 items-center justify-center rounded-md font-mono text-xs font-medium transition-colors duration-150',
                    active
                      ? 'tactile-control bg-muted text-foreground'
                      : 'text-muted-foreground hover:bg-muted/60 hover:text-foreground'
                  )}
                >
                  {Math.round(preset * 100)}%
                </button>
              )
            })}
          </div>
        </div>
      </PopoverContent>
    </Popover>
  )
}
