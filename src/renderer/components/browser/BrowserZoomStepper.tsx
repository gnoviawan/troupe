import { Minus, Plus } from '@/components/icons'
import { FOCUS_RING_CLASS } from '@/components/ui/panel-styles'
import { cn } from '@/lib/utils'
import {
  MAX_BROWSER_ZOOM,
  MIN_BROWSER_ZOOM,
  useBrowserSessionStore
} from '@/stores/browser-session-store'

interface BrowserZoomStepperProps {
  browserTabId: string
  disabled?: boolean
  compact?: boolean
  disableResetAt100?: boolean
}

export function BrowserZoomStepper({
  browserTabId,
  disabled = false,
  compact = false,
  disableResetAt100 = false
}: BrowserZoomStepperProps): React.JSX.Element {
  const zoomFactor = useBrowserSessionStore((s) => s.tabs.get(browserTabId)?.zoomFactor ?? 1)
  const stepZoom = useBrowserSessionStore((s) => s.stepZoom)
  const resetZoom = useBrowserSessionStore((s) => s.resetZoom)

  const percent = Math.round(zoomFactor * 100)
  const canZoomOut = !disabled && zoomFactor > MIN_BROWSER_ZOOM + 0.001
  const canZoomIn = !disabled && zoomFactor < MAX_BROWSER_ZOOM - 0.001

  return (
    <div className={cn('flex shrink-0 items-center', compact ? 'gap-1 @[460px]:gap-1.5' : 'gap-1')}>
      <button
        type="button"
        aria-label="Zoom out"
        disabled={!canZoomOut}
        onClick={() => stepZoom(browserTabId, 'out')}
        className={cn(
          'flex size-6 items-center justify-center rounded-md bg-control-fill text-foreground shadow-control transition-colors duration-150 ease-out hover:bg-control-fill-hover disabled:opacity-40',
          FOCUS_RING_CLASS
        )}
      >
        <Minus size={12} />
      </button>
      <button
        type="button"
        aria-label="Reset zoom"
        title="Reset zoom to 100%"
        disabled={disabled || (disableResetAt100 && percent === 100)}
        onClick={() => resetZoom(browserTabId)}
        className={cn(
          'rounded text-center font-mono text-xs font-medium tabular-nums text-foreground hover:underline disabled:no-underline disabled:opacity-75',
          compact ? 'min-w-8 @[460px]:min-w-10' : 'w-11',
          FOCUS_RING_CLASS
        )}
      >
        {percent}%
      </button>
      <button
        type="button"
        aria-label="Zoom in"
        disabled={!canZoomIn}
        onClick={() => stepZoom(browserTabId, 'in')}
        className={cn(
          'flex size-6 items-center justify-center rounded-md bg-control-fill text-foreground shadow-control transition-colors duration-150 ease-out hover:bg-control-fill-hover disabled:opacity-40',
          FOCUS_RING_CLASS
        )}
      >
        <Plus size={12} />
      </button>
    </div>
  )
}
