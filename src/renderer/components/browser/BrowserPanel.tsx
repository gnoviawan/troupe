import { useCallback, useEffect, useRef } from 'react'
import { toast } from 'sonner'
import { useBrowserWebview } from '@/hooks/use-browser-webview'
import {
  browserCheckLocalPort,
  browserTabScreenshot,
  onAgentationActionRequested,
  onAgentationAnnotationCreated
} from '@/lib/browser-api'
import { cropBase64Png } from '@/lib/browser-screenshot'
import { openTerminalAtCwd } from '@/lib/terminal-spawn'
import { cn } from '@/lib/utils'
import { useBrowserSessionStore, VIEWPORT_PRESET_SIZES } from '@/stores/browser-session-store'
import { useProjectStore } from '@/stores/project-store'
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

function sameOriginAndPath(a: string | undefined, b: string): boolean {
  if (!a || !b) return true
  try {
    const ua = new URL(a)
    const ub = new URL(b)
    return ua.origin === ub.origin && ua.pathname === ub.pathname
  } catch {
    return a === b
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
  const annotateMode = tab?.annotateMode ?? false

  const updateUrl = useBrowserSessionStore((s) => s.updateUrl)
  const setLoadError = useBrowserSessionStore((s) => s.setLoadError)
  const setCustomViewportWidth = useBrowserSessionStore((s) => s.setCustomViewportWidth)
  const addPendingAnnotation = useBrowserSessionStore((s) => s.addPendingAnnotation)
  const setAnnotateTool = useBrowserSessionStore((s) => s.setAnnotateTool)

  const activeProject = useProjectStore((s) =>
    s.activeProjectId ? s.projects.find((p) => p.id === s.activeProjectId) : undefined
  )

  const stageRef = useRef<HTMLDivElement>(null)
  const layoutKey = `${viewportPreset}-${orientation}-${customWidth ?? 'preset'}`

  const { containerRef } = useBrowserWebview(browserTabId, isVisible, url, {
    hasError: Boolean(loadError),
    zoomFactor,
    layoutKey
  })

  // Bridge overlay annotations (comment, rectangle, ruler guide) into the host batch queue
  // and attach an auto-cropped PNG from the native webview screenshot.
  useEffect(() => {
    if (!isVisible || !annotateMode) return
    const subCreated = onAgentationAnnotationCreated((payload) => {
      if (!sameOriginAndPath(payload.url, url)) return
      const kind =
        payload.kind === 'rectangle' || payload.kind === 'guide' || payload.kind === 'measurement'
          ? payload.kind
          : 'feedback'
      void (async () => {
        let cropDataUrl: string | undefined
        let cropBase64: string | undefined
        const shot = await browserTabScreenshot(browserTabId)
        if (shot.success) {
          if (
            payload.boundingBox &&
            payload.boundingBox.width > 0 &&
            payload.boundingBox.height > 0
          ) {
            const cropped = await cropBase64Png(shot.data.base64, {
              ...payload.boundingBox,
              dpr: window.devicePixelRatio || 1
            })
            cropBase64 = cropped.base64
            cropDataUrl = cropped.dataUrl
          } else {
            cropBase64 = shot.data.base64
            cropDataUrl = `data:${shot.data.mimeType};base64,${shot.data.base64}`
          }
        }
        addPendingAnnotation(browserTabId, {
          id: payload.id,
          kind,
          element: payload.element,
          elementPath: payload.elementPath,
          comment: payload.comment,
          axis: payload.measurement?.axis,
          positionPx: payload.measurement?.positionPx,
          deltaPx: payload.measurement?.deltaPx,
          meta: payload.boundingBox
            ? `${payload.boundingBox.width}×${payload.boundingBox.height}px`
            : undefined,
          cropDataUrl,
          cropBase64
        })
      })()
    })

    const subAction = onAgentationActionRequested((payload) => {
      if (!payload.output) return
      try {
        const parsed = JSON.parse(payload.output) as {
          kind?: string
          rect?: { x: number; y: number; width: number; height: number }
          dpr?: number
        }
        if (parsed.kind !== 'screenshot_area' || !parsed.rect) return
        const rect = parsed.rect
        void (async () => {
          const shot = await browserTabScreenshot(browserTabId)
          if (!shot.success) {
            toast.error(shot.error || 'Could not capture area screenshot.')
            return
          }
          const cropped = await cropBase64Png(shot.data.base64, {
            ...rect,
            dpr: parsed.dpr ?? (window.devicePixelRatio || 1)
          })
          addPendingAnnotation(browserTabId, {
            id: `shot-${Date.now()}`,
            kind: 'rectangle',
            element: 'Area screenshot',
            elementPath: `${rect.width}×${rect.height}px area`,
            comment: `Captured ${rect.width}×${rect.height}px area screenshot.`,
            meta: `${rect.width}×${rect.height}px`,
            cropBase64: cropped.base64,
            cropDataUrl: cropped.dataUrl
          })
          setAnnotateTool(browserTabId, 'comment')
          toast.success('Captured area screenshot into batch.')
        })()
      } catch {
        // Non-JSON action output
      }
    })

    return () => {
      subCreated.unlisten()
      subAction.unlisten()
    }
  }, [addPendingAnnotation, annotateMode, browserTabId, isVisible, setAnnotateTool, url])

  // Auto-reload localhost offline view once the port begins listening
  useEffect(() => {
    if (!isVisible || !loadError?.port) return
    const port = loadError.port
    const timer = window.setInterval(() => {
      void browserCheckLocalPort(port).then((probe) => {
        if (probe.success && probe.data) {
          setLoadError(browserTabId, null)
          if (url) updateUrl(browserTabId, url)
        }
      })
    }, 2000)
    return () => window.clearInterval(timer)
  }, [browserTabId, isVisible, loadError?.port, setLoadError, updateUrl, url])

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

  const handleOpenTerminal = useCallback(() => {
    if (!activeProject?.path) return
    void openTerminalAtCwd(activeProject.id, activeProject.path)
  }, [activeProject])

  const isConstrained = viewportPreset !== 'desktop' && Boolean(url) && !loadError

  return (
    <div
      className={cn(
        'flex h-full w-full flex-col',
        isVisible ? 'visible' : 'invisible absolute inset-0'
      )}
    >
      {isVisible && <BrowserControls browserTabId={browserTabId} isVisible={isVisible} />}
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
          <BrowserOfflineView
            error={loadError}
            onRetry={handleRetryOffline}
            onOpenTerminal={activeProject?.path ? handleOpenTerminal : undefined}
          />
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
