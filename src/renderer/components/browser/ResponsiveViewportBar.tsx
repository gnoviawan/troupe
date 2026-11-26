import { useEffect, useRef } from 'react'
import { Monitor, RotateCw, Smartphone, Tablet } from '@/components/icons'
import { FOCUS_RING_CLASS, SEGMENTED_TRACK_CLASS, segmentClass } from '@/components/ui/panel-styles'
import { cn } from '@/lib/utils'
import {
  resolveViewportDimensions,
  useBrowserSessionStore,
  type ViewportPreset
} from '@/stores/browser-session-store'
import { BrowserZoomStepper } from './BrowserZoomStepper'

interface ResponsiveViewportBarProps {
  browserTabId: string
  measuredWidth?: number
  measuredHeight?: number
}

const PRESETS: ReadonlyArray<{ id: ViewportPreset; label: string }> = [
  { id: 'desktop', label: 'Desktop' },
  { id: 'tablet', label: 'Tablet' },
  { id: 'mobile', label: 'Mobile' }
]

export function ResponsiveViewportBar({
  browserTabId,
  measuredWidth,
  measuredHeight
}: ResponsiveViewportBarProps): React.JSX.Element {
  const viewportPreset = useBrowserSessionStore(
    (s) => s.tabs.get(browserTabId)?.viewportPreset ?? 'desktop'
  )
  const orientation = useBrowserSessionStore(
    (s) => s.tabs.get(browserTabId)?.viewportOrientation ?? 'portrait'
  )
  const customWidth = useBrowserSessionStore((s) => s.tabs.get(browserTabId)?.customViewportWidth)
  const setViewportPreset = useBrowserSessionStore((s) => s.setViewportPreset)
  const toggleOrientation = useBrowserSessionStore((s) => s.toggleViewportOrientation)

  const open = viewportPreset !== 'desktop'
  const panelRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (panelRef.current) {
      panelRef.current.inert = !open
    }
  }, [open])

  const { presetSpec, targetWidth, targetHeight } = resolveViewportDimensions(
    viewportPreset,
    orientation,
    customWidth
  )
  const resolvedTargetWidth = targetWidth ?? 0
  const resolvedTargetHeight = targetHeight ?? 0
  const displayWidth =
    measuredWidth && measuredWidth > 0 ? Math.round(measuredWidth) : resolvedTargetWidth
  const displayHeight =
    measuredHeight && measuredHeight > 0 ? Math.round(measuredHeight) : resolvedTargetHeight
  const isClamped =
    Boolean(presetSpec) &&
    ((measuredWidth !== undefined &&
      measuredWidth > 0 &&
      Math.round(measuredWidth) < resolvedTargetWidth) ||
      (measuredHeight !== undefined &&
        measuredHeight > 0 &&
        Math.round(measuredHeight) < resolvedTargetHeight))

  return (
    <div
      ref={panelRef}
      className="t-acc-panel shrink-0"
      data-open={open ? 'true' : 'false'}
      aria-hidden={!open}
      data-testid="responsive-viewport-bar"
    >
      <div className="t-acc-panel-inner">
        <div className="@container flex h-[38px] min-w-0 items-center justify-between gap-2 border-b border-border/80 bg-card px-2.5">
          <div
            role="tablist"
            aria-label="Viewport preset"
            className={cn(SEGMENTED_TRACK_CLASS, 'h-[30px] shrink-0 bg-background')}
          >
            {PRESETS.map((preset) => {
              const selected = viewportPreset === preset.id
              return (
                <button
                  key={preset.id}
                  type="button"
                  role="tab"
                  aria-selected={selected}
                  aria-label={preset.label}
                  title={preset.label}
                  onClick={() => setViewportPreset(browserTabId, preset.id)}
                  className={cn(segmentClass(selected), 'h-[24px] gap-1.5 px-2 @[460px]:px-2.5')}
                >
                  {preset.id === 'desktop' ? (
                    <Monitor size={13} aria-hidden="true" />
                  ) : preset.id === 'tablet' ? (
                    <Tablet size={13} aria-hidden="true" />
                  ) : (
                    <Smartphone size={13} aria-hidden="true" />
                  )}
                  <span className="hidden @[460px]:inline">{preset.label}</span>
                </button>
              )
            })}
          </div>

          <div className="flex min-w-0 items-center gap-1.5 @[460px]:gap-2">
            <span
              data-testid="responsive-viewport-dimensions"
              title={
                isClamped
                  ? `Clamped to ${displayWidth} × ${displayHeight} (target ${resolvedTargetWidth} × ${resolvedTargetHeight})`
                  : `${displayWidth} × ${displayHeight}`
              }
              className="truncate font-mono text-xs font-medium tabular-nums text-secondary-foreground"
            >
              {displayWidth} × {displayHeight}
            </span>
            <button
              type="button"
              aria-label="Rotate viewport orientation"
              title="Rotate portrait / landscape"
              onClick={() => toggleOrientation(browserTabId)}
              className={cn(
                'flex size-6 shrink-0 items-center justify-center rounded-md bg-control-fill text-foreground shadow-control transition-colors duration-150 ease-out hover:bg-control-fill-hover',
                FOCUS_RING_CLASS
              )}
            >
              <RotateCw size={12} />
            </button>
          </div>

          <BrowserZoomStepper browserTabId={browserTabId} compact />
        </div>
      </div>
    </div>
  )
}
