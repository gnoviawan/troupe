import { useCallback, useRef } from 'react'
import { useBrowserWebview } from '@/hooks/use-browser-webview'
import { cn } from '@/lib/utils'
import { useBrowserSessionStore, VIEWPORT_PRESET_SIZES } from '@/stores/browser-session-store'
import { BrowserConsentStrip } from './BrowserConsentStrip'
import { BrowserControls } from './BrowserControls'
import { BrowserOfflineView } from './BrowserOfflineView'
import { BrowserStartPage } from './BrowserStartPage'
import { ResponsiveViewportBar } from './ResponsiveViewportBar'

interface BrowserPanelProps {
  browserTabId: string
  isVisible: boolean
}

function displayHost(rawUrl: string): string {
  try {
    return new URL(rawUrl).host
  } catch {
    return rawUrl
  }
}

export function BrowserPanel({ browserTabId, isVisible }: BrowserPanelProps): React.JSX.Element {
  const tab = useBrowserSessionStore((state) => state.tabs.get(browserTabId))
  const url = tab?.url ?? ''
  const loading = tab?.loading ?? false
  const loadError = tab?.loadError ?? null
  const zoomFactor = tab?.zoomFactor ?? 1
  const viewportPreset = tab?.viewportPreset ?? 'desktop'
  const orientation = tab?.viewportOrientation ?? 'portrait'
  const customWidth = tab?.customViewportWidth

  const updateUrl = useBrowserSessionStore((s) => s.updateUrl)
  const setLoadError = useBrowserSessionStore((s) => s.setLoadError)
  const setCustomViewportWidth = useBrowserSessionStore((s) => s.setCustomViewportWidth)

  const stageRef = useRef<HTMLDivElement>(null)
  const layoutKey = `${viewportPreset}-${orientation}-${customWidth ?? 'preset'}`

  // Visibility is owned solely by useBrowserWebview — it serializes
  // show/hide after create and inspects IpcResult.success.
  const { containerRef } = useBrowserWebview(browserTabId, isVisible, url, {
    hasError: Boolean(loadError),
    zoomFactor,
    layoutKey
  })

  const presetSpec = viewportPreset === 'desktop' ? null : VIEWPORT_PRESET_SIZES[viewportPreset]
  const targetWidth =
    customWidth ??
    (presetSpec ? (orientation === 'portrait' ? presetSpec.width : presetSpec.height) : undefined)

  const startGripDrag = useCallback(
    (side: 'left' | 'right', e: React.PointerEvent<HTMLButtonElement>) => {
      if (!targetWidth) return
      e.preventDefault()
      const startX = e.clientX
      const initialWidth = targetWidth
      const onMove = (ev: PointerEvent) => {
        const delta = (ev.clientX - startX) * (side === 'right' ? 2 : -2)
        const next = Math.max(320, Math.min(1280, Math.round(initialWidth + delta)))
        setCustomViewportWidth(browserTabId, next)
      }
      const onUp = () => {
        window.removeEventListener('pointermove', onMove)
        window.removeEventListener('pointerup', onUp)
      }
      window.addEventListener('pointermove', onMove)
      window.addEventListener('pointerup', onUp)
    },
    [browserTabId, setCustomViewportWidth, targetWidth]
  )

  const handleStartNavigate = useCallback(
    (nextUrl: string) => {
      updateUrl(browserTabId, nextUrl)
    },
    [browserTabId, updateUrl]
  )

  const handleRetryOffline = useCallback(() => {
    setLoadError(browserTabId, null)
    if (url) updateUrl(browserTabId, url)
  }, [browserTabId, setLoadError, updateUrl, url])

  const isConstrained = viewportPreset !== 'desktop' && Boolean(url) && !loadError

  return (
    <div
      className={cn(
        'flex h-full w-full flex-col',
        isVisible ? 'visible' : 'invisible absolute inset-0'
      )}
    >
      {isVisible && <BrowserControls browserTabId={browserTabId} />}
      {isVisible && <ResponsiveViewportBar browserTabId={browserTabId} />}
      {isVisible && <BrowserConsentStrip browserTabId={browserTabId} />}

      <div
        ref={stageRef}
        className={cn(
          'relative flex flex-1 overflow-hidden bg-background',
          isConstrained && 'items-center justify-center gap-3 p-5'
        )}
      >
        {!url ? (
          <BrowserStartPage onNavigate={handleStartNavigate} />
        ) : loadError ? (
          <BrowserOfflineView error={loadError} onRetry={handleRetryOffline} />
        ) : (
          <>
            {isConstrained && (
              <button
                type="button"
                aria-label="Resize responsive viewport width (left handle)"
                onPointerDown={(e) => startGripDrag('left', e)}
                className="h-12 w-1 shrink-0 cursor-ew-resize rounded-full bg-border transition-colors duration-150 hover:bg-muted-foreground"
              />
            )}

            <div
              ref={containerRef}
              style={
                isConstrained && targetWidth
                  ? { width: `${targetWidth}px`, maxWidth: '100%', height: '100%' }
                  : undefined
              }
              className={cn(
                'relative bg-background',
                isConstrained ? 'tactile-modal overflow-hidden rounded-[10px]' : 'h-full flex-1'
              )}
            >
              {/* Non-blocking status pill only — never dims the page with a dark scrim */}
              {loading && (
                <div
                  data-testid="browser-loading-pill"
                  className="tactile-control pointer-events-none absolute bottom-3 left-3 z-10 inline-flex h-[26px] items-center gap-2 rounded-full bg-popover px-2.5 font-mono text-xs text-muted-foreground"
                >
                  <span className="size-1.5 rounded-full bg-primary" />
                  <span>Waiting for {displayHost(url)}…</span>
                </div>
              )}
            </div>

            {isConstrained && (
              <button
                type="button"
                aria-label="Resize responsive viewport width (right handle)"
                onPointerDown={(e) => startGripDrag('right', e)}
                className="h-12 w-1 shrink-0 cursor-ew-resize rounded-full bg-border transition-colors duration-150 hover:bg-muted-foreground"
              />
            )}
          </>
        )}
      </div>
    </div>
  )
}
