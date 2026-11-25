import { useCallback, useEffect, useRef, useState } from 'react'
import { useNativeWebviewOccluded } from '@/hooks/use-native-webview-occlusion'
import { getAppliedUiZoom } from '@/hooks/use-ui-zoom'
import {
  browserCheckLocalPort,
  browserTabCreate,
  browserTabDestroy,
  browserTabHide,
  browserTabNavigate,
  browserTabResize,
  browserTabSetZoom,
  browserTabShow,
  onBrowserTabUpdated
} from '@/lib/browser-api'
import { logFrontendError } from '@/lib/log-api'
import { useBrowserSessionStore } from '@/stores/browser-session-store'
import { useWorkspaceStore, browserTabId as workspaceBrowserTabId } from '@/stores/workspace-store'

interface BrowserBounds {
  x: number
  y: number
  width: number
  height: number
}

function measureLogicalBounds(el: HTMLElement): BrowserBounds {
  const rect = el.getBoundingClientRect()
  const zoom = getAppliedUiZoom()
  return {
    x: rect.x * zoom,
    y: rect.y * zoom,
    width: rect.width * zoom,
    height: rect.height * zoom
  }
}

const ZOOM_EPSILON = 0.001

function parseLocalhostPort(rawUrl: string): { host: string; port: number } | null {
  try {
    const parsed = new URL(rawUrl)
    if (parsed.hostname !== 'localhost' && parsed.hostname !== '127.0.0.1') return null
    const port = parsed.port ? Number(parsed.port) : parsed.protocol === 'https:' ? 443 : 80
    if (!Number.isInteger(port) || port <= 0) return null
    return { host: parsed.host, port }
  } catch {
    return null
  }
}

export function useBrowserWebview(
  browserTabId: string,
  paneVisible: boolean,
  url: string,
  opts?: { hasError?: boolean; zoomFactor?: number; layoutKey?: string }
) {
  // An occluded webview (a shell popover is open), blank Start Page, or offline
  // error view hides the native child webview so React UI renders unobstructed.
  const occluded = useNativeWebviewOccluded()
  const hasUrl = url.trim().length > 0
  const hasDocument = hasUrl && !opts?.hasError
  const isVisible = paneVisible && !occluded && hasDocument
  const zoomFactor = opts?.zoomFactor ?? 1
  const layoutKey = opts?.layoutKey ?? 'desktop'
  const containerRef = useRef<HTMLDivElement>(null)
  const createdRef = useRef(false)
  const [createdTick, setCreatedTick] = useState(0)
  const mountedRef = useRef(true)
  const mountTokenRef = useRef(0)
  const urlRef = useRef(url)
  const latestUrlRef = useRef(url)
  latestUrlRef.current = url
  const zoomAppliedRef = useRef(1)
  const visibilityRef = useRef(isVisible)

  const updateBounds = useCallback((): Promise<void> => {
    const el = containerRef.current
    if (!el || !createdRef.current) return Promise.resolve()
    const bounds = measureLogicalBounds(el)
    return browserTabResize(browserTabId, bounds)
      .then((result) => {
        if (!result.success) {
          void logFrontendError({
            message: `browserTabResize failed for tab ${browserTabId}: ${result.error}`,
            source: 'useBrowserWebview'
          })
        }
      })
      .catch((err) => {
        void logFrontendError({
          message: `browserTabResize rejected for tab ${browserTabId}: ${
            err instanceof Error ? err.message : String(err)
          }`,
          source: 'useBrowserWebview'
        })
      })
  }, [browserTabId])
  // Create / destroy webview lifecycle
  useEffect(() => {
    mountedRef.current = true
    mountTokenRef.current += 1
    const mountToken = mountTokenRef.current
    const el = containerRef.current
    if (!el || !hasUrl) return
    urlRef.current = latestUrlRef.current

    useBrowserSessionStore.getState().setLoading(browserTabId, true)

    const bounds = measureLogicalBounds(el)
    browserTabCreate(browserTabId, urlRef.current, bounds)
      .then((result) => {
        if (!mountedRef.current || mountToken !== mountTokenRef.current) {
          browserTabDestroy(browserTabId).catch((e) => {
            void logFrontendError({
              message: `browserTabDestroy after stale mount for tab ${browserTabId}: ${
                e instanceof Error ? e.message : String(e)
              }`,
              source: 'useBrowserWebview'
            })
          })
          return
        }
        if (result.success) {
          createdRef.current = true
          zoomAppliedRef.current = 1
          setCreatedTick((n) => n + 1)
          useBrowserSessionStore
            .getState()
            .setAgentControlled(browserTabId, result.data.agentControlled)
          // Resync bounds after creation. The container was measured once at
          // call time (before browserTabCreate above), but the pane layout can
          // still be mid-transition — pane wrappers use `transition-all
          // duration-150` (PaneContent) — or otherwise unsettled when the
          // async create resolves. Without a re-measure the native webview
          // keeps stale creation bounds and overflows into the adjacent pane
          // (issue #644). Poll across frames until the rect stops changing
          // (or a frame budget elapses) so we adopt *settled*, not transient,
          // bounds — then reveal. Mirrors ConnectedTerminal's
          // waitForStableLayout approach for the same class of race.
          const MAX_RESYNC_WAIT_FRAMES = 30 // ~0.5s at 60fps; covers the 150ms transition
          let prev: BrowserBounds | null = null
          let frames = 0
          const resyncAndShow = async (): Promise<void> => {
            if (!mountedRef.current || mountToken !== mountTokenRef.current) return
            const elNow = containerRef.current
            if (!elNow) return
            const next = measureLogicalBounds(elNow)
            // Wait until the full rect (position + size) is stable across two
            // consecutive frames: a same-size container can still MOVE during a
            // layout transition (sidebar toggle, gutter drag), so x/y must
            // settle too, not just width/height.
            const stable =
              prev !== null &&
              prev.x === next.x &&
              prev.y === next.y &&
              prev.width === next.width &&
              prev.height === next.height
            prev = next
            frames += 1
            if (!stable && frames < MAX_RESYNC_WAIT_FRAMES) {
              requestAnimationFrame(() => {
                void resyncAndShow()
              })
              return
            }
            // Settled (or budget exhausted): resync via the shared helper so the
            // result.success branch is inspected (no silent failure), then
            // reveal. Await the resize before showing so the webview is never
            // revealed at stale creation bounds.
            await updateBounds()
            if (visibilityRef.current) {
              browserTabShow(browserTabId)
                .then((r) => {
                  if (!r.success) {
                    void logFrontendError({
                      message: `browserTabShow failed for tab ${browserTabId}: ${r.error}`,
                      source: 'useBrowserWebview'
                    })
                  }
                })
                .catch((err) => {
                  void logFrontendError({
                    message: `browserTabShow rejected for tab ${browserTabId}: ${
                      err instanceof Error ? err.message : String(err)
                    }`,
                    source: 'useBrowserWebview'
                  })
                })
            } else {
              browserTabHide(browserTabId)
                .then((r) => {
                  if (!r.success) {
                    void logFrontendError({
                      message: `browserTabHide failed for tab ${browserTabId}: ${r.error}`,
                      source: 'useBrowserWebview'
                    })
                  }
                })
                .catch((err) => {
                  void logFrontendError({
                    message: `browserTabHide rejected for tab ${browserTabId}: ${
                      err instanceof Error ? err.message : String(err)
                    }`,
                    source: 'useBrowserWebview'
                  })
                })
            }
          }
          requestAnimationFrame(() => {
            void resyncAndShow()
          })
        } else if (result.code === 'BROWSER_TAB_UNKNOWN_AGENT_TAB') {
          // A replayed agent-tab open the host no longer waits on: the
          // agent never gets this tab, so the pane drops it.
          useWorkspaceStore.getState().removeTab(workspaceBrowserTabId(browserTabId))
          useBrowserSessionStore.getState().removeTab(browserTabId)
        } else {
          void logFrontendError({
            message: `browserTabCreate failed for tab ${browserTabId}: ${result.error ?? 'unknown'} [${result.code ?? 'NO_CODE'}]`,
            source: 'useBrowserWebview'
          })
          useBrowserSessionStore.getState().setLoading(browserTabId, false)
        }
      })
      .catch((err) => {
        void logFrontendError({
          message: `browserTabCreate rejected for tab ${browserTabId}: ${
            err instanceof Error ? err.message : String(err)
          }`,
          source: 'useBrowserWebview'
        })
        useBrowserSessionStore.getState().setLoading(browserTabId, false)
      })

    return () => {
      mountedRef.current = false
      mountTokenRef.current += 1
      browserTabDestroy(browserTabId)
        .then((result) => {
          if (!result.success) {
            void logFrontendError({
              message: `browserTabDestroy failed for tab ${browserTabId}: ${result.error ?? 'unknown'}`,
              source: 'useBrowserWebview'
            })
          }
        })
        .catch((e) => {
          void logFrontendError({
            message: `browserTabDestroy rejected for tab ${browserTabId}: ${
              e instanceof Error ? e.message : String(e)
            }`,
            source: 'useBrowserWebview'
          })
        })
      createdRef.current = false
    }
  }, [browserTabId, hasUrl, updateBounds])

  // Show / hide on visibility change
  useEffect(() => {
    visibilityRef.current = isVisible
    if (!createdRef.current) return
    if (isVisible) {
      updateBounds()
      browserTabShow(browserTabId)
        .then((r) => {
          if (!r.success) {
            void logFrontendError({
              message: `browserTabShow failed for tab ${browserTabId}: ${r.error}`,
              source: 'useBrowserWebview'
            })
          }
        })
        .catch((err) => {
          void logFrontendError({
            message: `browserTabShow rejected for tab ${browserTabId}: ${
              err instanceof Error ? err.message : String(err)
            }`,
            source: 'useBrowserWebview'
          })
        })
    } else {
      browserTabHide(browserTabId)
        .then((r) => {
          if (!r.success) {
            void logFrontendError({
              message: `browserTabHide failed for tab ${browserTabId}: ${r.error}`,
              source: 'useBrowserWebview'
            })
          }
        })
        .catch((err) => {
          void logFrontendError({
            message: `browserTabHide rejected for tab ${browserTabId}: ${
              err instanceof Error ? err.message : String(err)
            }`,
            source: 'useBrowserWebview'
          })
        })
    }
  }, [isVisible, browserTabId, updateBounds])

  // Navigate when url prop changes externally (and probe localhost ports for offline view)
  useEffect(() => {
    const localTarget = parseLocalhostPort(url)
    if (localTarget) {
      void browserCheckLocalPort(localTarget.port).then((probe) => {
        if (probe.success && !probe.data) {
          useBrowserSessionStore.getState().setLoadError(browserTabId, {
            code: 'ERR_CONNECTION_REFUSED',
            host: localTarget.host,
            port: localTarget.port,
            message: `No local server is listening on port ${localTarget.port}. Start your dev server or check the port number.`
          })
        }
      })
    }
    if (url === urlRef.current) return
    urlRef.current = url
    if (!createdRef.current) return
    useBrowserSessionStore.getState().setLoading(browserTabId, true)
    browserTabNavigate(browserTabId, url)
      .then((result) => {
        if (!result.success) {
          void logFrontendError({
            message: `browserTabNavigate failed for tab ${browserTabId}: ${result.error}`,
            source: 'useBrowserWebview'
          })
          useBrowserSessionStore.getState().setLoading(browserTabId, false)
        }
      })
      .catch((err) => {
        void logFrontendError({
          message: `browserTabNavigate rejected for tab ${browserTabId}: ${
            err instanceof Error ? err.message : String(err)
          }`,
          source: 'useBrowserWebview'
        })
        useBrowserSessionStore.getState().setLoading(browserTabId, false)
      })
  }, [url, browserTabId])

  // Resize observer — re-attaches when containerRef mounts after Start Page or Offline view
  useEffect(() => {
    if (!hasDocument) return
    const el = containerRef.current
    if (!el) return

    const ro = new ResizeObserver(() => {
      updateBounds()
    })
    ro.observe(el)

    return () => {
      ro.disconnect()
    }
  }, [updateBounds, hasDocument])

  // Resync webview bounds when responsive preset / orientation / width changes
  useEffect(() => {
    if (!createdRef.current || !layoutKey) return
    const raf = requestAnimationFrame(() => {
      void updateBounds()
    })
    return () => cancelAnimationFrame(raf)
  }, [layoutKey, updateBounds])

  // Apply native per-tab zoom factor when changed or right after webview creation
  useEffect(() => {
    if (!createdRef.current || createdTick === 0) return
    if (Math.abs(zoomAppliedRef.current - zoomFactor) < ZOOM_EPSILON) return
    zoomAppliedRef.current = zoomFactor
    void browserTabSetZoom(browserTabId, zoomFactor).then((result) => {
      if (!result.success) {
        void logFrontendError({
          level: 'warn',
          message: `browserTabSetZoom failed for tab ${browserTabId}: ${result.error}`,
          source: 'useBrowserWebview'
        })
      }
    })
  }, [browserTabId, zoomFactor, createdTick])

  // The host registry reports what the engine committed, whoever started
  // it. Record the URL as current first so the url prop catching up does
  // not navigate the webview again.
  useEffect(() => {
    const subscription = onBrowserTabUpdated((view) => {
      if (view.tabId !== browserTabId) return
      urlRef.current = view.url
      useBrowserSessionStore.getState().applyView(view)
    })
    return () => subscription.unlisten()
  }, [browserTabId])

  return { containerRef }
}
