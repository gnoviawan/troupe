import { useCallback, useEffect, useRef, useState } from 'react'
import { toast } from 'sonner'
import { useBrowserWebview } from '@/hooks/use-browser-webview'
import {
  browserAnnotationsSync,
  browserCheckLocalPort,
  browserSaveAnnotationCrop,
  browserTabAnnotationCmd,
  browserTabInjectAgentation,
  browserTabPollAnnotationOutbox,
  browserTabScreenshot,
  onAgentationActionRequested,
  onAgentationAnnotationCreated,
  onAgentationAnnotationUpdated
} from '@/lib/browser-api'
import { copyBase64PngToClipboard, cropBase64Png } from '@/lib/browser-screenshot'
import { displayHost } from '@/lib/browser-url'
import { logFrontendError } from '@/lib/log-api'
import { isTauriContext } from '@/lib/tauri-runtime'
import { openTerminalAtCwd } from '@/lib/terminal-spawn'
import { cn } from '@/lib/utils'
import { resolveViewportDimensions, useBrowserSessionStore } from '@/stores/browser-session-store'
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
  const epoch = tab?.epoch ?? 0

  const updateUrl = useBrowserSessionStore((s) => s.updateUrl)
  const setLoadError = useBrowserSessionStore((s) => s.setLoadError)
  const setCustomViewportWidth = useBrowserSessionStore((s) => s.setCustomViewportWidth)
  const addPendingAnnotation = useBrowserSessionStore((s) => s.addPendingAnnotation)
  const removePendingAnnotation = useBrowserSessionStore((s) => s.removePendingAnnotation)
  const removePendingAnnotations = useBrowserSessionStore((s) => s.removePendingAnnotations)
  const setAnnotateMode = useBrowserSessionStore((s) => s.setAnnotateMode)
  const setAnnotateTool = useBrowserSessionStore((s) => s.setAnnotateTool)
  const setStandaloneScreenshotArea = useBrowserSessionStore((s) => s.setStandaloneScreenshotArea)
  const markScreenshotCopied = useBrowserSessionStore((s) => s.markScreenshotCopied)

  const activeProject = useProjectStore((s) =>
    s.activeProjectId ? s.projects.find((p) => p.id === s.activeProjectId) : undefined
  )

  const stageRef = useRef<HTMLDivElement>(null)
  const gripCleanupRef = useRef<(() => void) | null>(null)
  const removedAnnotationIdsRef = useRef<Set<string>>(new Set())
  const [reloadNonce, setReloadNonce] = useState(0)
  const [measuredViewport, setMeasuredViewport] = useState<{
    width: number
    height: number
  } | null>(null)
  const offlineErrorKeyRef = useRef('')
  const stopGeneration = tab?.stopGeneration ?? 0
  const layoutKey = `${viewportPreset}-${orientation}-${customWidth ?? 'preset'}`

  const { containerRef, occlusionPreviewUrl } = useBrowserWebview(browserTabId, isVisible, url, {
    hasError: Boolean(loadError),
    zoomFactor,
    layoutKey,
    reloadNonce,
    stopGeneration,
    stageRef
  })

  // Clean up any active grip drag listeners on unmount
  useEffect(() => {
    return () => {
      gripCleanupRef.current?.()
      gripCleanupRef.current = null
    }
  }, [])

  // Re-sync overlay state after a navigation/reload completes while annotateMode is active.
  // Keep prevEpochRef unchanged while `loading` is true so the post-load rerun detects the new epoch.
  const prevEpochRef = useRef(epoch)
  useEffect(() => {
    if (epoch === prevEpochRef.current) return
    if (loading || !url) return
    prevEpochRef.current = epoch
    if (!annotateMode) return
    const current = useBrowserSessionStore.getState().getTab(browserTabId)
    if (!current?.annotateMode) return
    void browserTabInjectAgentation(browserTabId).then((res) => {
      if (!res.success && res.code !== 'NOT_TAURI') return
      const latest = useBrowserSessionStore.getState().getTab(browserTabId)
      if (!latest?.annotateMode) return
      void browserTabAnnotationCmd(browserTabId, { type: 'set_active', active: true })
      void browserTabAnnotationCmd(browserTabId, {
        type: 'set_tool',
        tool: latest.annotateTool,
        axis: latest.annotateTool === 'ruler' ? latest.rulerAxis : undefined
      })
      if (latest.animationsPaused) {
        void browserTabAnnotationCmd(browserTabId, { type: 'set_pause', paused: true })
      }
      if (latest.pinsHidden) {
        void browserTabAnnotationCmd(browserTabId, { type: 'set_pins_hidden', hidden: true })
      }
    })
  }, [annotateMode, browserTabId, epoch, loading, url])

  // Drop resolved/dismissed annotations from this tab's pending queue when updated via MCP/HTTP
  useEffect(() => {
    const subUpdated = onAgentationAnnotationUpdated((payload) => {
      if (payload.tabId && payload.tabId !== browserTabId) return
      if (payload.status !== 'resolved' && payload.status !== 'dismissed') return
      removedAnnotationIdsRef.current.add(payload.id)
      const hasItem = useBrowserSessionStore
        .getState()
        .getTab(browserTabId)
        ?.pendingAnnotations.some((item) => item.id === payload.id)
      if (!hasItem) return
      removePendingAnnotation(browserTabId, payload.id)
      void browserTabAnnotationCmd(browserTabId, { type: 'clear', ids: [payload.id] })
    })
    return () => {
      subUpdated.unlisten()
      removedAnnotationIdsRef.current.clear()
    }
  }, [browserTabId, removePendingAnnotation])

  // Drain overlay annotations/actions via the CSP- and mixed-content-proof host
  // bridge while annotation mode is active on desktop.
  useEffect(() => {
    if (!isVisible || !annotateMode || !isTauriContext()) return
    let active = true
    let inFlight = false
    const poll = () => {
      if (!active || inFlight) return
      inFlight = true
      void browserTabPollAnnotationOutbox(browserTabId)
        .catch((err) => {
          void logFrontendError({
            level: 'warn',
            source: 'BrowserPanel',
            message: `browserTabPollAnnotationOutbox failed: ${
              err instanceof Error ? err.message : String(err)
            }`
          })
        })
        .finally(() => {
          inFlight = false
        })
    }
    const timer = window.setInterval(poll, 120)
    return () => {
      active = false
      window.clearInterval(timer)
    }
  }, [annotateMode, browserTabId, isVisible])

  // Bridge overlay annotations (comment, rectangle, ruler guide) into the host batch queue,
  // auto-crop via native screenshot, and persist the crop PNG to disk + SQLite/MCP.
  useEffect(() => {
    if (!isVisible || !annotateMode) return
    const subCreated = onAgentationAnnotationCreated((payload) => {
      if (payload.tabId && payload.tabId !== browserTabId) return
      if (!sameOriginAndPath(payload.url, url)) return
      const expectedPageUrl = payload.url || url
      const kind =
        payload.kind === 'rectangle' || payload.kind === 'guide' || payload.kind === 'measurement'
          ? payload.kind
          : 'feedback'
      void (async () => {
        let cropDataUrl: string | undefined
        let cropBase64: string | undefined
        let cropPath: string | undefined
        const shot = await browserTabScreenshot(browserTabId)
        if (shot.success) {
          if (
            payload.boundingBox &&
            payload.boundingBox.width > 0 &&
            payload.boundingBox.height > 0
          ) {
            const tabZoom = useBrowserSessionStore.getState().getTab(browserTabId)?.zoomFactor ?? 1
            const cropped = await cropBase64Png(shot.data.base64, {
              ...payload.boundingBox,
              viewportWidth: payload.viewportWidth,
              dpr: (window.devicePixelRatio || 1) * tabZoom
            })
            if (cropped) {
              cropBase64 = cropped.base64
              cropDataUrl = cropped.dataUrl
            }
          } else {
            cropBase64 = shot.data.base64
            cropDataUrl = `data:${shot.data.mimeType};base64,${shot.data.base64}`
          }
          if (cropBase64) {
            const saved = await browserSaveAnnotationCrop(payload.id, cropBase64)
            if (saved.success) {
              cropPath = saved.data
            }
          }
        }
        if (removedAnnotationIdsRef.current.has(payload.id)) {
          removedAnnotationIdsRef.current.delete(payload.id)
          return
        }
        const liveUrl = useBrowserSessionStore.getState().getTab(browserTabId)?.url ?? ''
        if (!sameOriginAndPath(expectedPageUrl, liveUrl)) {
          return
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
          gapPx: payload.measurement?.gapPx,
          meta: payload.boundingBox
            ? `${payload.boundingBox.width}×${payload.boundingBox.height}px`
            : payload.measurement?.gapPx !== undefined
              ? `${payload.measurement.gapPx}px spacing`
              : undefined,
          cropDataUrl,
          cropBase64,
          cropPath
        })
      })()
    })

    const subAction = onAgentationActionRequested((payload) => {
      if (!payload.output) return
      if (payload.tabId && payload.tabId !== browserTabId) return
      try {
        const parsed = JSON.parse(payload.output) as {
          kind?: string
          tabId?: string
          ids?: unknown[]
          rect?: { x: number; y: number; width: number; height: number }
          dpr?: number
          viewportWidth?: number
        }
        if (parsed.tabId && parsed.tabId !== browserTabId) return

        if (parsed.kind === 'annotations_removed' && Array.isArray(parsed.ids)) {
          const ids = parsed.ids.filter(
            (id): id is string => typeof id === 'string' && id.length > 0
          )
          if (ids.length > 0) {
            for (const id of ids) {
              removedAnnotationIdsRef.current.add(id)
            }
            removePendingAnnotations(browserTabId, ids)
            void browserAnnotationsSync('delete', ids)
          }
          return
        }

        const captureToken =
          useBrowserSessionStore.getState().getTab(browserTabId)?.screenshotAreaToken ?? 0

        const finishAreaCapture = (expectedToken: number) => {
          const latestTab = useBrowserSessionStore.getState().getTab(browserTabId)
          if ((latestTab?.screenshotAreaToken ?? 0) !== expectedToken) return
          if (latestTab?.standaloneScreenshotArea) {
            setStandaloneScreenshotArea(browserTabId, false)
            setAnnotateMode(browserTabId, false)
            void browserTabAnnotationCmd(browserTabId, { type: 'set_active', active: false })
          } else if (latestTab?.annotateMode && latestTab.annotateTool === 'screenshot-area') {
            setAnnotateTool(browserTabId, 'comment')
            void browserTabAnnotationCmd(browserTabId, { type: 'set_tool', tool: 'comment' })
          }
        }

        if (parsed.kind === 'screenshot_area_cancelled') {
          finishAreaCapture(captureToken)
          return
        }
        if (parsed.kind !== 'screenshot_area' || !parsed.rect) return
        const rect = parsed.rect
        void (async () => {
          const hideOverlay =
            useBrowserSessionStore.getState().getTab(browserTabId)?.annotateMode === true
          if (hideOverlay) {
            await browserTabAnnotationCmd(browserTabId, {
              type: 'set_capture_hidden',
              hidden: true
            })
            await new Promise<void>((resolve) => {
              requestAnimationFrame(() => {
                requestAnimationFrame(() => resolve())
              })
            })
          }
          try {
            const shot = await browserTabScreenshot(browserTabId)
            if (!shot.success) {
              toast.error(shot.error || 'Could not capture area screenshot.')
              void logFrontendError({
                level: 'warn',
                source: 'BrowserPanel',
                message: `screenshot_area capture failed: ${shot.error ?? shot.code ?? 'unknown'}`
              })
              return
            }
            const tabZoom = useBrowserSessionStore.getState().getTab(browserTabId)?.zoomFactor ?? 1
            const cropped = await cropBase64Png(shot.data.base64, {
              ...rect,
              viewportWidth: parsed.viewportWidth,
              dpr: parsed.dpr ?? (window.devicePixelRatio || 1) * tabZoom
            })
            if (!cropped) {
              toast.error('Could not crop selected area.')
              void logFrontendError({
                level: 'warn',
                source: 'BrowserPanel',
                message: 'screenshot_area crop failed'
              })
              return
            }
            const copied = await copyBase64PngToClipboard(cropped.base64)
            if (copied.ok) {
              markScreenshotCopied(browserTabId)
              toast.success('Screenshot copied to clipboard')
            } else {
              toast.error(copied.error)
            }
          } finally {
            if (hideOverlay) {
              void browserTabAnnotationCmd(browserTabId, {
                type: 'set_capture_hidden',
                hidden: false
              })
            }
            finishAreaCapture(captureToken)
          }
        })()
      } catch {
        // Non-JSON action output
      }
    })

    return () => {
      subCreated.unlisten()
      subAction.unlisten()
    }
  }, [
    addPendingAnnotation,
    annotateMode,
    browserTabId,
    isVisible,
    markScreenshotCopied,
    removePendingAnnotations,
    setAnnotateMode,
    setAnnotateTool,
    setStandaloneScreenshotArea,
    url
  ])

  // Track current tab/URL/error identity so late probes never clear a newer error
  const errorKey = loadError
    ? `${browserTabId}|${url}|${loadError.code}|${loadError.port ?? ''}`
    : ''
  offlineErrorKeyRef.current = errorKey

  const attemptOfflineRecovery = useCallback(
    async (port: number, expectedKey: string, notifyIfStillOffline: boolean) => {
      const probe = await browserCheckLocalPort(port)
      if (expectedKey !== offlineErrorKeyRef.current) return
      if (probe.success && probe.data) {
        setLoadError(browserTabId, null)
        setReloadNonce((n) => n + 1)
      } else if (notifyIfStillOffline && probe.success && !probe.data) {
        toast.error(`Port ${port} is still not responding.`)
      }
    },
    [browserTabId, setLoadError]
  )

  // Auto-reload localhost offline view once the port begins listening
  useEffect(() => {
    if (!isVisible || !loadError?.port || !errorKey) return
    const port = loadError.port
    const timer = window.setInterval(() => {
      void attemptOfflineRecovery(port, errorKey, false)
    }, 2000)
    return () => window.clearInterval(timer)
  }, [attemptOfflineRecovery, errorKey, isVisible, loadError?.port])

  const { targetWidth, targetHeight } = resolveViewportDimensions(
    viewportPreset,
    orientation,
    customWidth
  )

  const startGripDrag = useCallback(
    (side: 'left' | 'right', e: React.PointerEvent<HTMLButtonElement>) => {
      if (!targetWidth) return
      e.preventDefault()
      gripCleanupRef.current?.()
      const startX = e.clientX
      const initialWidth = targetWidth
      const onMove = (ev: PointerEvent) => {
        const delta = (ev.clientX - startX) * (side === 'right' ? 2 : -2)
        const next = Math.max(320, Math.min(1280, Math.round(initialWidth + delta)))
        setCustomViewportWidth(browserTabId, next)
      }
      const cleanup = () => {
        window.removeEventListener('pointermove', onMove)
        window.removeEventListener('pointerup', cleanup)
        window.removeEventListener('pointercancel', cleanup)
        if (gripCleanupRef.current === cleanup) {
          gripCleanupRef.current = null
        }
      }
      gripCleanupRef.current = cleanup
      window.addEventListener('pointermove', onMove)
      window.addEventListener('pointerup', cleanup)
      window.addEventListener('pointercancel', cleanup)
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
    if (loadError?.port && errorKey) {
      void attemptOfflineRecovery(loadError.port, errorKey, true)
      return
    }
    setLoadError(browserTabId, null)
    setReloadNonce((n) => n + 1)
  }, [attemptOfflineRecovery, browserTabId, errorKey, loadError?.port, setLoadError])

  const handleOpenTerminal = useCallback(() => {
    if (!activeProject?.path) return
    void openTerminalAtCwd(activeProject.id, activeProject.path)
  }, [activeProject])

  const isConstrained = viewportPreset !== 'desktop' && Boolean(url) && !loadError

  // Track actual rendered dimensions of the constrained viewport slot so
  // ResponsiveViewportBar displays clamped dimensions on smaller panes.
  useEffect(() => {
    if (!isConstrained) {
      setMeasuredViewport(null)
      return
    }
    const el = containerRef.current
    if (!el) return
    const syncSize = () => {
      const rect = el.getBoundingClientRect()
      const w = Math.round(rect.width)
      const h = Math.round(rect.height)
      if (w <= 0 || h <= 0) return
      setMeasuredViewport((prev) =>
        prev && prev.width === w && prev.height === h ? prev : { width: w, height: h }
      )
    }
    syncSize()
    if (typeof ResizeObserver === 'undefined') return
    const ro = new ResizeObserver(syncSize)
    ro.observe(el)
    return () => ro.disconnect()
  }, [containerRef, isConstrained])

  return (
    <div
      className={cn(
        'flex h-full w-full flex-col',
        isVisible ? 'visible' : 'invisible absolute inset-0'
      )}
    >
      {isVisible && <BrowserControls browserTabId={browserTabId} isVisible={isVisible} />}
      {isVisible && (
        <ResponsiveViewportBar
          browserTabId={browserTabId}
          measuredWidth={measuredViewport?.width}
          measuredHeight={measuredViewport?.height}
        />
      )}
      {isVisible && <BrowserConsentStrip browserTabId={browserTabId} />}

      <div
        ref={stageRef}
        className={cn(
          'relative flex flex-1 overflow-hidden',
          isConstrained ? 'items-center justify-center gap-3 bg-muted/30 p-5' : 'bg-background'
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
                className="h-12 w-1 shrink-0 cursor-ew-resize rounded-full bg-border transition-colors duration-150 ease-out hover:bg-muted-foreground"
              />
            )}

            <div
              ref={containerRef}
              style={
                isConstrained && targetWidth
                  ? {
                      width: `${targetWidth}px`,
                      maxWidth: '100%',
                      height: targetHeight ? `${targetHeight}px` : '100%',
                      maxHeight: '100%'
                    }
                  : undefined
              }
              className={cn(
                'relative bg-background',
                isConstrained
                  ? 'overflow-hidden rounded-[10px] shadow-md ring-1 ring-border'
                  : 'h-full flex-1'
              )}
            >
              {occlusionPreviewUrl && (
                <img
                  data-testid="browser-occlusion-preview"
                  src={occlusionPreviewUrl}
                  alt=""
                  aria-hidden="true"
                  className="pointer-events-none absolute inset-0 h-full w-full object-fill select-none"
                />
              )}
              {loading && (
                <div
                  data-testid="browser-loading-pill"
                  className="pointer-events-none absolute bottom-3 left-3 z-10 inline-flex h-[26px] items-center gap-2 rounded-full bg-popover px-2.5 font-mono text-xs text-muted-foreground shadow-control"
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
                className="h-12 w-1 shrink-0 cursor-ew-resize rounded-full bg-border transition-colors duration-150 ease-out hover:bg-muted-foreground"
              />
            )}
          </>
        )}
      </div>
    </div>
  )
}
