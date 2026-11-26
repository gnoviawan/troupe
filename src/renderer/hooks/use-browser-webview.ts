import { type RefObject, useCallback, useEffect, useRef, useState } from 'react'
import { useNativeWebviewOccluded } from '@/hooks/use-native-webview-occlusion'
import { getAppliedUiZoom, useAppliedUiZoom } from '@/hooks/use-ui-zoom'
import {
  browserCheckLocalPort,
  browserTabCreate,
  browserTabDestroy,
  browserTabHide,
  browserTabNavigate,
  browserTabResize,
  browserTabScreenshot,
  browserTabSetZoom,
  browserTabShow,
  onBrowserTabUpdated
} from '@/lib/browser-api'
import { parseLocalhostPort } from '@/lib/browser-url'
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
const OCCLUSION_SNAPSHOT_TIMEOUT_MS = 220

export function useBrowserWebview(
  browserTabId: string,
  paneVisible: boolean,
  url: string,
  opts?: {
    hasError?: boolean
    zoomFactor?: number
    layoutKey?: string
    reloadNonce?: number
    stopGeneration?: number
    stageRef?: RefObject<HTMLDivElement | null>
  }
) {
  // An occluded webview (a shell popover is open), blank Start Page, or offline
  // error view hides the native child webview so React UI renders unobstructed.
  const occluded = useNativeWebviewOccluded()
  const appliedUiZoom = useAppliedUiZoom()
  const hasUrl = url.trim().length > 0
  const hasDocument = hasUrl && !opts?.hasError
  const isVisible = paneVisible && !occluded && hasDocument
  const zoomFactor = opts?.zoomFactor ?? 1
  const layoutKey = opts?.layoutKey ?? 'desktop'
  const reloadNonce = opts?.reloadNonce ?? 0
  const stopGeneration = opts?.stopGeneration ?? 0
  const stageRef = opts?.stageRef
  const containerRef = useRef<HTMLDivElement>(null)
  const createdRef = useRef(false)
  const [createdTick, setCreatedTick] = useState(0)
  const [occlusionPreviewUrl, setOcclusionPreviewUrl] = useState<string | null>(null)
  const occlusionTokenRef = useRef(0)
  const mountedRef = useRef(true)
  const mountTokenRef = useRef(0)
  const probeTokenRef = useRef(0)
  const lastReloadNonceRef = useRef(reloadNonce)
  const urlRef = useRef(url)
  const latestUrlRef = useRef(url)
  latestUrlRef.current = url
  const zoomAppliedRef = useRef(1)
  const visibilityRef = useRef(isVisible)
  visibilityRef.current = isVisible

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
    if (!el || !hasDocument) return
    urlRef.current = latestUrlRef.current

    useBrowserSessionStore.getState().setLoading(browserTabId, true)

    const bounds = measureLogicalBounds(el)
    const targetUrl = urlRef.current
    const localTarget = parseLocalhostPort(targetUrl)

    const createWebview = () => {
      if (!mountedRef.current || mountToken !== mountTokenRef.current) return
      const createUrl = latestUrlRef.current
      if (!createUrl) return
      urlRef.current = createUrl
      browserTabCreate(browserTabId, createUrl, bounds)
        .then((result) => {
          if (!mountedRef.current || mountToken !== mountTokenRef.current) {
            if (result.success) {
              browserTabDestroy(browserTabId).catch((e) => {
                void logFrontendError({
                  message: `browserTabDestroy after stale mount for tab ${browserTabId}: ${
                    e instanceof Error ? e.message : String(e)
                  }`,
                  source: 'useBrowserWebview'
                })
              })
            }
            return
          }
          if (result.success) {
            createdRef.current = true
            zoomAppliedRef.current = 1
            setCreatedTick((n) => n + 1)
            useBrowserSessionStore
              .getState()
              .setAgentControlled(browserTabId, result.data.agentControlled)

            // If the user requested a newer URL while browserTabCreate was in flight,
            // navigate the newly created native webview to that latest URL immediately.
            const latestNow = latestUrlRef.current
            if (latestNow && latestNow !== createUrl) {
              urlRef.current = latestNow
              useBrowserSessionStore.getState().setLoading(browserTabId, true)
              browserTabNavigate(browserTabId, latestNow).catch((err) => {
                void logFrontendError({
                  message: `browserTabNavigate after create for tab ${browserTabId}: ${
                    err instanceof Error ? err.message : String(err)
                  }`,
                  source: 'useBrowserWebview'
                })
                useBrowserSessionStore.getState().setLoading(browserTabId, false)
              })
            }
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
    }

    if (localTarget) {
      void browserCheckLocalPort(localTarget.port)
        .then((probe) => {
          if (!mountedRef.current || mountToken !== mountTokenRef.current) return
          if (latestUrlRef.current !== targetUrl) {
            createWebview()
            return
          }
          if (probe.success && !probe.data) {
            useBrowserSessionStore.getState().setLoadError(browserTabId, {
              code: 'ERR_CONNECTION_REFUSED',
              host: localTarget.host,
              port: localTarget.port,
              message: `No local server is listening on port ${localTarget.port}. Start your dev server or check the port number.`
            })
            return
          }
          createWebview()
        })
        .catch((err) => {
          void logFrontendError({
            level: 'warn',
            message: `browserCheckLocalPort failed before create for tab ${browserTabId}: ${
              err instanceof Error ? err.message : String(err)
            }`,
            source: 'useBrowserWebview'
          })
          createWebview()
        })
    } else {
      createWebview()
    }

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
  }, [browserTabId, hasDocument, updateBounds])

  // Show / hide on visibility change. When hidden due to a shell popover/menu
  // occluding a visible tab, capture a frozen snapshot first so the page stays
  // visible behind open dropdowns instead of flashing black, and clear the
  // snapshot cleanly once the live native webview is shown again.
  useEffect(() => {
    visibilityRef.current = isVisible
    if (!paneVisible || !hasDocument) {
      occlusionTokenRef.current += 1
      setOcclusionPreviewUrl(null)
    }
    if (!createdRef.current) return
    let cancelled = false
    const token = ++occlusionTokenRef.current

    const hideNative = () => {
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

    if (isVisible) {
      void updateBounds().then(() => {
        if (cancelled || !visibilityRef.current || !createdRef.current) return
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
          .finally(() => {
            if (!cancelled && token === occlusionTokenRef.current) {
              setOcclusionPreviewUrl(null)
            }
          })
      })
    } else if (occluded && paneVisible && hasDocument) {
      void (async () => {
        let preview: string | null = null
        let timerId: number | null = null
        try {
          const shot = await Promise.race([
            browserTabScreenshot(browserTabId),
            new Promise<null>((resolve) => {
              timerId = window.setTimeout(() => resolve(null), OCCLUSION_SNAPSHOT_TIMEOUT_MS)
            })
          ])
          if (timerId !== null) window.clearTimeout(timerId)
          if (
            !cancelled &&
            token === occlusionTokenRef.current &&
            !visibilityRef.current &&
            shot?.success &&
            shot.data.base64
          ) {
            preview = `data:${shot.data.mimeType};base64,${shot.data.base64}`
          }
        } catch (err) {
          if (timerId !== null) window.clearTimeout(timerId)
          void logFrontendError({
            level: 'warn',
            message: `occlusion snapshot failed for tab ${browserTabId}: ${
              err instanceof Error ? err.message : String(err)
            }`,
            source: 'useBrowserWebview'
          })
        }
        if (cancelled || token !== occlusionTokenRef.current || visibilityRef.current) return
        if (preview) {
          setOcclusionPreviewUrl(preview)
          await new Promise<void>((resolve) => {
            requestAnimationFrame(() => resolve())
          })
        }
        if (cancelled || token !== occlusionTokenRef.current || visibilityRef.current) return
        hideNative()
      })()
    } else {
      setOcclusionPreviewUrl(null)
      hideNative()
    }
    return () => {
      cancelled = true
    }
  }, [isVisible, occluded, paneVisible, hasDocument, browserTabId, updateBounds])

  // Cancel any pending localhost probe when the user clicks Stop
  useEffect(() => {
    if (stopGeneration > 0) {
      probeTokenRef.current += 1
    }
  }, [stopGeneration])

  // Navigate when url prop changes externally or when reloadNonce bumps (e.g. offline retry)
  useEffect(() => {
    const forceReload = reloadNonce !== lastReloadNonceRef.current
    lastReloadNonceRef.current = reloadNonce
    if (!url) return

    const probeToken = ++probeTokenRef.current
    const localTarget = parseLocalhostPort(url)
    const shouldNavigate = forceReload || url !== urlRef.current

    const navigateNow = () => {
      if (!shouldNavigate) return
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
    }

    if (localTarget) {
      void browserCheckLocalPort(localTarget.port)
        .then((probe) => {
          if (probeToken !== probeTokenRef.current) return
          if (probe.success && !probe.data) {
            useBrowserSessionStore.getState().setLoadError(browserTabId, {
              code: 'ERR_CONNECTION_REFUSED',
              host: localTarget.host,
              port: localTarget.port,
              message: `No local server is listening on port ${localTarget.port}. Start your dev server or check the port number.`
            })
            return
          }
          navigateNow()
        })
        .catch((err) => {
          if (probeToken !== probeTokenRef.current) return
          void logFrontendError({
            level: 'warn',
            message: `browserCheckLocalPort failed for tab ${browserTabId}: ${
              err instanceof Error ? err.message : String(err)
            }`,
            source: 'useBrowserWebview'
          })
          navigateNow()
        })
      return
    }

    navigateNow()
  }, [url, browserTabId, reloadNonce])

  // Observe container, stage wrapper (catches x/y centering shifts when container
  // keeps fixed Tablet/Phone dimensions), window resize, and sibling accordion
  // transitions (ResponsiveViewportBar 250ms expand/collapse & consent strip).
  useEffect(() => {
    if (!hasDocument) return
    const el = containerRef.current
    if (!el) return

    const ro = new ResizeObserver(() => {
      void updateBounds()
    })
    ro.observe(el)
    const stageEl = stageRef?.current
    if (stageEl && stageEl !== el) {
      ro.observe(stageEl)
    }
    const panelEl = stageEl?.parentElement
    if (panelEl && panelEl !== stageEl && panelEl !== el) {
      ro.observe(panelEl)
    }

    const onWindowResize = () => {
      void updateBounds()
    }
    window.addEventListener('resize', onWindowResize)

    const onTransitionEnd = (ev: TransitionEvent) => {
      if (
        ev.propertyName === 'grid-template-rows' ||
        ev.propertyName === 'height' ||
        ev.propertyName === 'width' ||
        ev.propertyName === 'transform'
      ) {
        void updateBounds()
      }
    }
    panelEl?.addEventListener('transitionend', onTransitionEnd)

    return () => {
      ro.disconnect()
      window.removeEventListener('resize', onWindowResize)
      panelEl?.removeEventListener('transitionend', onTransitionEnd)
    }
  }, [updateBounds, hasDocument, stageRef])

  // Resync webview bounds when responsive preset / orientation / width changes,
  // settling across the 250ms ResponsiveViewportBar accordion transition.
  useEffect(() => {
    if (!createdRef.current || !layoutKey) return
    let cancelled = false
    let rafId = 0
    let frames = 0
    let stableFrames = 0
    let prev: BrowserBounds | null = null
    const MAX_SETTLE_FRAMES = 24 // ~400ms at 60fps; covers 250ms accordion transition

    const tick = () => {
      if (cancelled) return
      const el = containerRef.current
      if (!el || !createdRef.current) return
      const next = measureLogicalBounds(el)
      const changed =
        prev === null ||
        prev.x !== next.x ||
        prev.y !== next.y ||
        prev.width !== next.width ||
        prev.height !== next.height
      if (changed) {
        void updateBounds()
        stableFrames = 0
      } else {
        stableFrames += 1
      }
      prev = next
      frames += 1
      if (frames < MAX_SETTLE_FRAMES && stableFrames < 3) {
        rafId = requestAnimationFrame(tick)
      }
    }
    rafId = requestAnimationFrame(tick)
    return () => {
      cancelled = true
      cancelAnimationFrame(rafId)
    }
  }, [layoutKey, updateBounds])

  // Resync webview bounds when whole-UI app zoom changes, even if fixed-size
  // Tablet/Phone containerRef kept the same CSS pixel dimensions.
  const prevUiZoomRef = useRef(appliedUiZoom)
  useEffect(() => {
    if (prevUiZoomRef.current === appliedUiZoom) return
    prevUiZoomRef.current = appliedUiZoom
    if (!createdRef.current) return
    void updateBounds()
  }, [appliedUiZoom, updateBounds])

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

  // Clear any stale frozen snapshot if the tab URL navigates while hidden
  useEffect(() => {
    if (!url) {
      occlusionTokenRef.current += 1
      setOcclusionPreviewUrl(null)
    }
  }, [url])

  return { containerRef, occlusionPreviewUrl }
}
