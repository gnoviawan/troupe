import { useState } from 'react'
import { ZoomIn, ZoomOut } from '@/components/icons'
import { MENU_MOTION_CLASS, OVERLAY_PANEL_SURFACE_CLASS } from '@/components/ui/menu-styles'
import { FOCUS_RING_CLASS } from '@/components/ui/panel-styles'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { cn } from '@/lib/utils'
import { useBrowserSessionStore } from '@/stores/browser-session-store'
import { BrowserZoomStepper } from './BrowserZoomStepper'

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
  const setZoomFactor = useBrowserSessionStore((s) => s.setZoomFactor)
  const resetZoom = useBrowserSessionStore((s) => s.resetZoom)

  const [open, setOpen] = useState(false)

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
          className={cn(
            'inline-flex h-[22px] shrink-0 items-center gap-1 rounded-full bg-control-fill px-2 font-mono text-xs font-medium tabular-nums text-foreground shadow-control transition-colors duration-150 ease-out hover:bg-control-fill-hover',
            FOCUS_RING_CLASS
          )}
        >
          {zoomFactor > 1 ? (
            <ZoomIn size={11} className="text-secondary-foreground" aria-hidden="true" />
          ) : (
            <ZoomOut size={11} className="text-secondary-foreground" aria-hidden="true" />
          )}
          <span>{percent}%</span>
        </button>
      </PopoverTrigger>
      <PopoverContent
        align="end"
        sideOffset={6}
        className={cn(OVERLAY_PANEL_SURFACE_CLASS, MENU_MOTION_CLASS, 'w-64 border-0 p-3')}
      >
        <div className="flex flex-col gap-3">
          <div className="flex items-center justify-between">
            <span className="text-xs font-medium text-foreground">Page zoom</span>
            <button
              type="button"
              onClick={() => resetZoom(browserTabId)}
              className={cn(
                'rounded text-xs font-medium text-secondary-foreground underline underline-offset-2 hover:text-foreground',
                FOCUS_RING_CLASS
              )}
            >
              Reset
            </button>
          </div>

          <div className="flex items-center justify-center">
            <BrowserZoomStepper browserTabId={browserTabId} />
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
                    'flex h-[26px] flex-1 items-center justify-center rounded-md font-mono text-xs font-medium transition-colors duration-150 ease-out',
                    FOCUS_RING_CLASS,
                    active
                      ? 'bg-foreground/[0.1] text-foreground'
                      : 'text-muted-foreground hover:bg-foreground/[0.06] hover:text-foreground'
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
