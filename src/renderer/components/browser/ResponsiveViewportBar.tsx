import { useEffect, useLayoutEffect, useRef } from 'react'
import { Minus, Monitor, Plus, RotateCw, Smartphone, Tablet } from '@/components/icons'
import { cn } from '@/lib/utils'
import {
  useBrowserSessionStore,
  VIEWPORT_PRESET_SIZES,
  type ViewportPreset
} from '@/stores/browser-session-store'

interface ResponsiveViewportBarProps {
  browserTabId: string
  measuredHeight?: number
}

const PRESETS: ReadonlyArray<{ id: ViewportPreset; label: string }> = [
  { id: 'desktop', label: 'Desktop' },
  { id: 'tablet', label: 'Tablet' },
  { id: 'mobile', label: 'Mobile' }
]

export function ResponsiveViewportBar({
  browserTabId,
  measuredHeight
}: ResponsiveViewportBarProps): React.JSX.Element {
  const viewportPreset = useBrowserSessionStore(
    (s) => s.tabs.get(browserTabId)?.viewportPreset ?? 'desktop'
  )
  const orientation = useBrowserSessionStore(
    (s) => s.tabs.get(browserTabId)?.viewportOrientation ?? 'portrait'
  )
  const customWidth = useBrowserSessionStore((s) => s.tabs.get(browserTabId)?.customViewportWidth)
  const zoomFactor = useBrowserSessionStore((s) => s.tabs.get(browserTabId)?.zoomFactor ?? 1)
  const setViewportPreset = useBrowserSessionStore((s) => s.setViewportPreset)
  const toggleOrientation = useBrowserSessionStore((s) => s.toggleViewportOrientation)
  const stepZoom = useBrowserSessionStore((s) => s.stepZoom)
  const resetZoom = useBrowserSessionStore((s) => s.resetZoom)

  const open = viewportPreset !== 'desktop'
  const panelRef = useRef<HTMLDivElement>(null)
  const pillRef = useRef<HTMLSpanElement>(null)

  useEffect(() => {
    if (panelRef.current) {
      panelRef.current.inert = !open
    }
  }, [open])
  const tabRefs = useRef<Record<ViewportPreset, HTMLButtonElement | null>>({
    desktop: null,
    tablet: null,
    mobile: null
  })
  const mountedRef = useRef(false)

  const syncPill = (animate: boolean): void => {
    const pill = pillRef.current
    const activeEl = tabRefs.current[viewportPreset]
    if (!pill || !activeEl) return
    if (!animate) {
      const prev = pill.style.transition
      pill.style.transition = 'none'
      pill.style.transform = `translateX(${activeEl.offsetLeft}px)`
      pill.style.width = `${activeEl.offsetWidth}px`
      void pill.offsetWidth
      pill.style.transition = prev
    } else {
      pill.style.transform = `translateX(${activeEl.offsetLeft}px)`
      pill.style.width = `${activeEl.offsetWidth}px`
    }
  }

  useLayoutEffect(() => {
    syncPill(mountedRef.current)
    mountedRef.current = true
  })

  useEffect(() => {
    const onResize = () => syncPill(false)
    window.addEventListener('resize', onResize)
    return () => window.removeEventListener('resize', onResize)
  })

  const presetSpec = viewportPreset === 'desktop' ? null : VIEWPORT_PRESET_SIZES[viewportPreset]
  const displayWidth =
    customWidth ??
    (presetSpec ? (orientation === 'portrait' ? presetSpec.width : presetSpec.height) : 0)
  const displayHeight =
    measuredHeight ??
    (presetSpec ? (orientation === 'portrait' ? presetSpec.height : presetSpec.width) : 0)
  const percent = Math.round(zoomFactor * 100)

  return (
    <div
      ref={panelRef}
      className="t-acc-panel shrink-0"
      data-open={open ? 'true' : 'false'}
      aria-hidden={!open}
      data-testid="responsive-viewport-bar"
    >
      <div className="t-acc-panel-inner">
        <div className="flex h-[38px] items-center justify-between border-b border-border/80 bg-card px-3">
          <div
            role="tablist"
            aria-label="Viewport preset"
            className="t-tabs h-[30px] gap-0.5 rounded-full bg-background p-0.5"
          >
            <span
              ref={pillRef}
              aria-hidden="true"
              className="t-tabs-pill tactile-control top-0.5 h-[26px] rounded-full bg-muted"
            />
            {PRESETS.map((preset) => {
              const selected = viewportPreset === preset.id
              return (
                <button
                  key={preset.id}
                  ref={(el) => {
                    tabRefs.current[preset.id] = el
                  }}
                  type="button"
                  role="tab"
                  aria-selected={selected}
                  onClick={() => setViewportPreset(browserTabId, preset.id)}
                  className={cn(
                    't-tab inline-flex h-[26px] items-center gap-1.5 rounded-full px-2.5 text-xs font-medium',
                    selected ? 'text-foreground' : 'text-muted-foreground hover:text-foreground'
                  )}
                >
                  {preset.id === 'desktop' ? (
                    <Monitor size={13} aria-hidden="true" />
                  ) : preset.id === 'tablet' ? (
                    <Tablet size={13} aria-hidden="true" />
                  ) : (
                    <Smartphone size={13} aria-hidden="true" />
                  )}
                  <span>{preset.label}</span>
                </button>
              )
            })}
          </div>

          <div className="flex items-center gap-2">
            <span className="font-mono text-xs font-medium tabular-nums text-secondary-foreground">
              {displayWidth} × {displayHeight}
            </span>
            <button
              type="button"
              aria-label="Rotate viewport orientation"
              title="Rotate portrait / landscape"
              onClick={() => toggleOrientation(browserTabId)}
              className="tactile-control flex size-6 items-center justify-center rounded-md bg-muted text-foreground transition-[transform,background-color] duration-150 hover:bg-secondary active:scale-[0.95]"
            >
              <RotateCw size={12} />
            </button>
          </div>

          <div className="flex items-center gap-1.5">
            <button
              type="button"
              aria-label="Zoom out"
              disabled={zoomFactor <= 0.5}
              onClick={() => stepZoom(browserTabId, 'out')}
              className="tactile-control flex size-6 items-center justify-center rounded-md bg-muted text-foreground transition-colors duration-150 hover:bg-secondary disabled:opacity-40"
            >
              <Minus size={12} />
            </button>
            <button
              type="button"
              aria-label="Reset zoom"
              title="Reset zoom to 100%"
              onClick={() => resetZoom(browserTabId)}
              className="min-w-10 text-center font-mono text-xs font-medium tabular-nums text-foreground hover:underline"
            >
              {percent}%
            </button>
            <button
              type="button"
              aria-label="Zoom in"
              disabled={zoomFactor >= 2}
              onClick={() => stepZoom(browserTabId, 'in')}
              className="tactile-control flex size-6 items-center justify-center rounded-md bg-muted text-foreground transition-colors duration-150 hover:bg-secondary disabled:opacity-40"
            >
              <Plus size={12} />
            </button>
          </div>
        </div>
      </div>
    </div>
  )
}
