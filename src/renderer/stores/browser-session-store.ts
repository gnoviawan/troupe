import { create } from 'zustand'
import type { BrowserTabView } from '@/lib/browser-api'

export type ViewportPreset = 'desktop' | 'tablet' | 'mobile'
export type ViewportOrientation = 'portrait' | 'landscape'
export type AnnotateTool = 'comment' | 'rectangle' | 'ruler' | 'screenshot-area'

export interface BrowserLoadError {
  code: string
  host: string
  port?: number
  message: string
}

export interface PendingAnnotationItem {
  id: string
  num: number
  kind: 'feedback' | 'rectangle' | 'guide' | 'measurement'
  element: string
  elementPath: string
  comment: string
  meta?: string
  axis?: 'horizontal' | 'vertical'
  positionPx?: number
  deltaPx?: number
  gapPx?: number
  cropDataUrl?: string
  cropBase64?: string
  /** Absolute file path where the cropped PNG was persisted on disk (also stored in SQLite/MCP). */
  cropPath?: string
}

export interface RecentBrowserUrl {
  url: string
  host: string
  path: string
  visitedAt: number
}

export interface BrowserTab {
  id: string
  url: string
  title: string
  loading: boolean
  canGoBack: boolean
  canGoForward: boolean
  /** True for a tab an ACP session drives via the `browser` tool. */
  agentControlled?: boolean
  /** Native webview zoom factor (0.5..=2.0, default 1.0). */
  zoomFactor: number
  viewportPreset: ViewportPreset
  viewportOrientation: ViewportOrientation
  customViewportWidth?: number
  annotateMode: boolean
  annotateTool: AnnotateTool
  standaloneScreenshotArea?: boolean
  animationsPaused: boolean
  pinsHidden: boolean
  pendingAnnotations: PendingAnnotationItem[]
  loadError: BrowserLoadError | null
  previousUrl?: string
  stopGeneration?: number
  /** Latest host navigation epoch (bumped on each committed document). */
  epoch?: number
}

export const MIN_BROWSER_ZOOM = 0.5
export const MAX_BROWSER_ZOOM = 2.0
export const ZOOM_STEPS = [0.5, 0.75, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2] as const

export const VIEWPORT_PRESET_SIZES: Record<
  Exclude<ViewportPreset, 'desktop'>,
  { width: number; height: number; label: string }
> = {
  tablet: { width: 768, height: 1024, label: 'Tablet' },
  mobile: { width: 390, height: 844, label: 'Mobile' }
}

/** Resolve the effective target viewport width/height for a tab's preset + orientation + custom width. */
export function resolveViewportDimensions(
  viewportPreset: ViewportPreset,
  orientation: ViewportOrientation,
  customWidth?: number
): {
  presetSpec: { width: number; height: number; label: string } | null
  targetWidth: number | undefined
  targetHeight: number | undefined
} {
  const presetSpec = viewportPreset === 'desktop' ? null : VIEWPORT_PRESET_SIZES[viewportPreset]
  if (!presetSpec) {
    return { presetSpec: null, targetWidth: undefined, targetHeight: undefined }
  }
  const targetWidth =
    customWidth ?? (orientation === 'portrait' ? presetSpec.width : presetSpec.height)
  const targetHeight = orientation === 'portrait' ? presetSpec.height : presetSpec.width
  return { presetSpec, targetWidth, targetHeight }
}

const VIEWPORT_CYCLE: ViewportPreset[] = ['desktop', 'tablet', 'mobile']
const MAX_RECENT_URLS = 8

export function clampBrowserZoom(factor: number): number {
  if (!Number.isFinite(factor)) return 1
  return Math.round(Math.min(MAX_BROWSER_ZOOM, Math.max(MIN_BROWSER_ZOOM, factor)) * 100) / 100
}

export function nextZoomStep(current: number, direction: 'in' | 'out'): number {
  const c = clampBrowserZoom(current)
  if (direction === 'in') {
    const found = ZOOM_STEPS.find((step) => step > c + 0.005)
    return found ?? MAX_BROWSER_ZOOM
  }
  for (let i = ZOOM_STEPS.length - 1; i >= 0; i -= 1) {
    if (ZOOM_STEPS[i] < c - 0.005) return ZOOM_STEPS[i]
  }
  return MIN_BROWSER_ZOOM
}

function parseUrlParts(rawUrl: string): { host: string; path: string } | null {
  try {
    const u = new URL(rawUrl)
    if (u.protocol !== 'http:' && u.protocol !== 'https:') return null
    return {
      host: u.host,
      path: `${u.pathname}${u.search}` === '/' ? '' : `${u.pathname}${u.search}`
    }
  } catch {
    return null
  }
}

export interface BrowserSessionState {
  tabs: Map<string, BrowserTab>
  recentUrls: RecentBrowserUrl[]

  createTab: (id: string, url?: string) => BrowserTab
  removeTab: (id: string) => void
  /** The URL the user asked for; the tab's webview follows it. */
  updateUrl: (id: string, url: string) => void
  /** Mirror the host registry's view of a tab. */
  applyView: (view: BrowserTabView) => void
  setLoading: (id: string, loading: boolean) => void
  setNavCapabilities: (id: string, canGoBack: boolean, canGoForward: boolean) => void
  setAgentControlled: (id: string, controlled: boolean) => void
  setZoomFactor: (id: string, zoomFactor: number) => void
  stepZoom: (id: string, direction: 'in' | 'out') => number
  resetZoom: (id: string) => void
  setViewportPreset: (id: string, preset: ViewportPreset) => void
  cycleViewportPreset: (id: string) => ViewportPreset
  toggleViewportOrientation: (id: string) => void
  setCustomViewportWidth: (id: string, width: number | undefined) => void
  setAnnotateMode: (id: string, active: boolean) => void
  setAnnotateTool: (id: string, tool: AnnotateTool) => void
  setStandaloneScreenshotArea: (id: string, active: boolean) => void
  toggleAnimationsPaused: (id: string) => boolean
  togglePinsHidden: (id: string) => boolean
  addPendingAnnotation: (id: string, item: Omit<PendingAnnotationItem, 'num'>) => void
  removePendingAnnotation: (id: string, annotationId: string) => void
  removePendingAnnotations: (id: string, annotationIds: string[]) => void
  clearPendingAnnotations: (id: string) => void
  setLoadError: (id: string, error: BrowserLoadError | null) => void
  bumpStopGeneration: (id: string) => void
  clearRecentUrls: () => void
  getTab: (id: string) => BrowserTab | undefined
}

const DEFAULT_BROWSER_URL = ''

function updateTab(
  state: BrowserSessionState,
  id: string,
  updater: (tab: BrowserTab) => BrowserTab
): Partial<BrowserSessionState> | BrowserSessionState {
  const existing = state.tabs.get(id)
  if (!existing) return state
  const next = new Map(state.tabs)
  next.set(id, updater(existing))
  return { tabs: next }
}

function recordRecent(list: RecentBrowserUrl[], url: string): RecentBrowserUrl[] {
  const parts = parseUrlParts(url)
  if (!parts) return list
  const entry: RecentBrowserUrl = {
    url,
    host: parts.host,
    path: parts.path,
    visitedAt: Date.now()
  }
  const filtered = list.filter((item) => item.url !== url)
  return [entry, ...filtered].slice(0, MAX_RECENT_URLS)
}

export const useBrowserSessionStore = create<BrowserSessionState>((set, get) => ({
  tabs: new Map(),
  recentUrls: [],

  createTab: (id: string, url: string = DEFAULT_BROWSER_URL) => {
    const tab: BrowserTab = {
      id,
      url,
      title: '',
      loading: Boolean(url),
      canGoBack: false,
      canGoForward: false,
      zoomFactor: 1,
      viewportPreset: 'desktop',
      viewportOrientation: 'portrait',
      annotateMode: false,
      annotateTool: 'comment',
      animationsPaused: false,
      pinsHidden: false,
      pendingAnnotations: [],
      loadError: null
    }
    set((state) => {
      const next = new Map(state.tabs)
      next.set(id, tab)
      return {
        tabs: next,
        recentUrls: url ? recordRecent(state.recentUrls, url) : state.recentUrls
      }
    })
    return tab
  },

  removeTab: (id: string) => {
    set((state) => {
      const next = new Map(state.tabs)
      next.delete(id)
      return { tabs: next }
    })
  },

  updateUrl: (id: string, url: string) => {
    set((state) => {
      const existing = state.tabs.get(id)
      if (!existing) return state
      const next = new Map(state.tabs)
      const previousUrl =
        existing.url && existing.url !== url && !existing.loadError
          ? existing.url
          : existing.previousUrl
      next.set(id, { ...existing, url, previousUrl, loadError: null, loading: Boolean(url) })
      return {
        tabs: next,
        recentUrls: url ? recordRecent(state.recentUrls, url) : state.recentUrls
      }
    })
  },

  applyView: (view: BrowserTabView) => {
    set((state) => {
      const t = state.tabs.get(view.tabId)
      if (!t) return state
      const next = new Map(state.tabs)
      next.set(view.tabId, {
        ...t,
        url: view.url,
        title: view.title,
        loading: view.loading,
        agentControlled: view.agentControlled,
        epoch: view.epoch,
        loadError: view.loading ? null : t.loadError
      })
      return {
        tabs: next,
        recentUrls: view.url ? recordRecent(state.recentUrls, view.url) : state.recentUrls
      }
    })
  },

  setLoading: (id, loading) => set((state) => updateTab(state, id, (t) => ({ ...t, loading }))),

  setNavCapabilities: (id, canGoBack, canGoForward) =>
    set((state) => updateTab(state, id, (t) => ({ ...t, canGoBack, canGoForward }))),

  setAgentControlled: (id, controlled) =>
    set((state) => updateTab(state, id, (t) => ({ ...t, agentControlled: controlled }))),

  setZoomFactor: (id, zoomFactor) =>
    set((state) =>
      updateTab(state, id, (t) => ({ ...t, zoomFactor: clampBrowserZoom(zoomFactor) }))
    ),

  stepZoom: (id, direction) => {
    const current = get().tabs.get(id)?.zoomFactor ?? 1
    const next = nextZoomStep(current, direction)
    get().setZoomFactor(id, next)
    return next
  },

  resetZoom: (id) => get().setZoomFactor(id, 1),

  setViewportPreset: (id, preset) =>
    set((state) =>
      updateTab(state, id, (t) => ({
        ...t,
        viewportPreset: preset,
        customViewportWidth: undefined
      }))
    ),

  cycleViewportPreset: (id) => {
    const current = get().tabs.get(id)?.viewportPreset ?? 'desktop'
    const next = VIEWPORT_CYCLE[(VIEWPORT_CYCLE.indexOf(current) + 1) % VIEWPORT_CYCLE.length]
    get().setViewportPreset(id, next)
    return next
  },

  toggleViewportOrientation: (id) =>
    set((state) =>
      updateTab(state, id, (t) => ({
        ...t,
        viewportOrientation: t.viewportOrientation === 'portrait' ? 'landscape' : 'portrait',
        customViewportWidth: undefined
      }))
    ),

  setCustomViewportWidth: (id, width) =>
    set((state) => updateTab(state, id, (t) => ({ ...t, customViewportWidth: width }))),

  setAnnotateMode: (id, active) =>
    set((state) =>
      updateTab(state, id, (t) => ({
        ...t,
        annotateMode: active,
        annotateTool: active ? t.annotateTool : 'comment',
        standaloneScreenshotArea: active ? t.standaloneScreenshotArea : false
      }))
    ),

  setAnnotateTool: (id, tool) =>
    set((state) => updateTab(state, id, (t) => ({ ...t, annotateTool: tool }))),

  setStandaloneScreenshotArea: (id, active) =>
    set((state) => updateTab(state, id, (t) => ({ ...t, standaloneScreenshotArea: active }))),

  toggleAnimationsPaused: (id) => {
    const next = !(get().tabs.get(id)?.animationsPaused ?? false)
    set((state) => updateTab(state, id, (t) => ({ ...t, animationsPaused: next })))
    return next
  },

  togglePinsHidden: (id) => {
    const next = !(get().tabs.get(id)?.pinsHidden ?? false)
    set((state) => updateTab(state, id, (t) => ({ ...t, pinsHidden: next })))
    return next
  },

  addPendingAnnotation: (id, item) =>
    set((state) =>
      updateTab(state, id, (t) => ({
        ...t,
        pendingAnnotations: [
          ...t.pendingAnnotations,
          { ...item, num: t.pendingAnnotations.length + 1 }
        ]
      }))
    ),

  removePendingAnnotation: (id, annotationId) =>
    set((state) =>
      updateTab(state, id, (t) => ({
        ...t,
        pendingAnnotations: t.pendingAnnotations
          .filter((a) => a.id !== annotationId)
          .map((a, idx) => ({ ...a, num: idx + 1 }))
      }))
    ),

  removePendingAnnotations: (id, annotationIds) =>
    set((state) => {
      const drop = new Set(annotationIds)
      return updateTab(state, id, (t) => ({
        ...t,
        pendingAnnotations: t.pendingAnnotations
          .filter((a) => !drop.has(a.id))
          .map((a, idx) => ({ ...a, num: idx + 1 }))
      }))
    }),

  clearPendingAnnotations: (id) =>
    set((state) => updateTab(state, id, (t) => ({ ...t, pendingAnnotations: [] }))),

  setLoadError: (id, loadError) =>
    set((state) => updateTab(state, id, (t) => ({ ...t, loadError, loading: false }))),

  bumpStopGeneration: (id) =>
    set((state) =>
      updateTab(state, id, (t) => ({
        ...t,
        loading: false,
        stopGeneration: (t.stopGeneration ?? 0) + 1
      }))
    ),

  clearRecentUrls: () => set({ recentUrls: [] }),

  getTab: (id) => get().tabs.get(id)
}))
