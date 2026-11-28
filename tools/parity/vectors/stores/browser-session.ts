import type { BrowserTabView } from '../../../../src/renderer/lib/browser-api'
import {
  displayHost,
  normalizeBrowserInputUrl,
  parseLocalhostPort,
  splitDisplayUrl
} from '../../../../src/renderer/lib/browser-url'
import {
  type BrowserLoadError,
  type BrowserTab,
  clampBrowserZoom,
  nextZoomStep,
  resolveViewportDimensions,
  useBrowserSessionStore,
  type ViewportOrientation,
  type ViewportPreset
} from '../../../../src/renderer/stores/browser-session-store'
import type { Scenario, StoreRecorder, VectorEvent } from '../record'

const ev = (action: string, ...args: unknown[]): VectorEvent => ({ action, args })
const q = (action: string, ...args: unknown[]): VectorEvent & { query: true } => ({
  action,
  args,
  query: true
})

function projectTab(tab: BrowserTab): Record<string, unknown> {
  const out: Record<string, unknown> = {
    id: tab.id,
    url: tab.url,
    title: tab.title,
    loading: tab.loading,
    canGoBack: tab.canGoBack,
    canGoForward: tab.canGoForward,
    zoomFactor: tab.zoomFactor,
    viewportPreset: tab.viewportPreset,
    viewportOrientation: tab.viewportOrientation,
    loadError: tab.loadError
  }
  if (tab.agentControlled !== undefined) out.agentControlled = tab.agentControlled
  if (tab.customViewportWidth !== undefined) out.customViewportWidth = tab.customViewportWidth
  if (tab.previousUrl !== undefined) out.previousUrl = tab.previousUrl
  if (tab.stopGeneration !== undefined) out.stopGeneration = tab.stopGeneration
  if (tab.epoch !== undefined) out.epoch = tab.epoch
  return out
}

export const browserSessionScenarios: Scenario[] = [
  {
    name: 'tab_lifecycle_zoom_and_viewport',
    steps: [
      ev('createTab', 'b1', 'http://localhost:3000/app'),
      ev('setNavCapabilities', 'b1', true, false),
      ev('applyView', {
        tabId: 'b1',
        url: 'http://localhost:3000/app?tab=1',
        title: 'Local App',
        loading: false,
        agentControlled: false,
        epoch: 2
      } satisfies BrowserTabView),
      ev('stepZoom', 'b1', 'in'),
      ev('stepZoom', 'b1', 'in'),
      ev('stepZoom', 'b1', 'out'),
      ev('resetZoom', 'b1'),
      ev('setZoomFactor', 'b1', 5),
      ev('cycleViewportPreset', 'b1'),
      ev('toggleViewportOrientation', 'b1'),
      ev('setCustomViewportWidth', 'b1', 820),
      ev('cycleViewportPreset', 'b1'),
      ev('setViewportPreset', 'b1', 'desktop'),
      ev('updateUrl', 'b1', 'https://example.com/docs'),
      ev('setLoadError', 'b1', {
        code: 'ERR_CONNECTION_REFUSED',
        host: '127.0.0.1:9999',
        port: 9999,
        message: 'Connection refused'
      } satisfies BrowserLoadError),
      ev('bumpStopGeneration', 'b1'),
      ev('setAgentControlled', 'b1', true),
      ev('clearRecentUrls'),
      ev('removeTab', 'b1')
    ]
  },
  {
    name: 'url_and_viewport_helpers',
    steps: [
      q('normalizeBrowserInputUrl', ''),
      q('normalizeBrowserInputUrl', ':3000/api'),
      q('normalizeBrowserInputUrl', 'localhost:5173/test'),
      q('normalizeBrowserInputUrl', '127.0.0.1:8199'),
      q('normalizeBrowserInputUrl', 'example.com/path'),
      q('normalizeBrowserInputUrl', 'https://troupe.dev/docs'),
      q('splitDisplayUrl', 'https://example.com/a/b?x=1'),
      q('splitDisplayUrl', 'http://localhost:3000/'),
      q('displayHost', 'http://127.0.0.1:8199/foo'),
      q('parseLocalhostPort', 'http://localhost:3000/foo'),
      q('parseLocalhostPort', 'https://example.com'),
      q('clampBrowserZoom', 0.1),
      q('clampBrowserZoom', 1.254),
      q('nextZoomStep', 1, 'in'),
      q('nextZoomStep', 1, 'out'),
      q('resolveViewportDimensions', 'desktop', 'portrait', undefined),
      q('resolveViewportDimensions', 'tablet', 'landscape', undefined),
      q('resolveViewportDimensions', 'mobile', 'portrait', 412)
    ]
  }
]

export const browserSessionRecorder: StoreRecorder = {
  store: 'browser-session',
  sources: ['src/renderer/stores/browser-session-store.ts', 'src/renderer/lib/browser-url.ts'],
  reset() {
    useBrowserSessionStore.setState({ tabs: new Map(), recentUrls: [] })
  },
  seed(initial) {
    const raw = initial as {
      tabs?: Record<string, BrowserTab>
      recentUrls?: Array<{ url: string; host: string; path: string; visitedAt: number }>
    }
    const tabs = new Map<string, BrowserTab>()
    for (const [id, tab] of Object.entries(raw.tabs ?? {})) {
      tabs.set(id, structuredClone(tab))
    }
    useBrowserSessionStore.setState({
      tabs,
      recentUrls: structuredClone(raw.recentUrls ?? [])
    })
  },
  snapshot() {
    const { tabs, recentUrls } = useBrowserSessionStore.getState()
    const projected: Record<string, unknown> = {}
    for (const [id, tab] of [...tabs.entries()].sort(([a], [b]) => a.localeCompare(b))) {
      projected[id] = projectTab(tab)
    }
    return {
      tabs: projected,
      recentUrls: recentUrls.map((item) => ({
        url: item.url,
        host: item.host,
        path: item.path,
        visitedAt: 0
      }))
    }
  },
  dispatch({ action, args }) {
    switch (action) {
      case 'normalizeBrowserInputUrl':
        return normalizeBrowserInputUrl(args[0] as string)
      case 'splitDisplayUrl':
        return splitDisplayUrl(args[0] as string)
      case 'displayHost':
        return displayHost(args[0] as string)
      case 'parseLocalhostPort':
        return parseLocalhostPort(args[0] as string)
      case 'clampBrowserZoom':
        return clampBrowserZoom(args[0] as number)
      case 'nextZoomStep':
        return nextZoomStep(args[0] as number, args[1] as 'in' | 'out')
      case 'resolveViewportDimensions': {
        const res = resolveViewportDimensions(
          args[0] as ViewportPreset,
          args[1] as ViewportOrientation,
          args[2] as number | undefined
        )
        return { targetWidth: res.targetWidth ?? null, targetHeight: res.targetHeight ?? null }
      }
      default:
        break
    }
    const store = useBrowserSessionStore.getState()
    switch (action) {
      case 'createTab':
        return projectTab(store.createTab(args[0] as string, args[1] as string | undefined))
      case 'removeTab':
        store.removeTab(args[0] as string)
        return undefined
      case 'updateUrl':
        store.updateUrl(args[0] as string, args[1] as string)
        return undefined
      case 'applyView':
        store.applyView(args[0] as BrowserTabView)
        return undefined
      case 'setLoading':
        store.setLoading(args[0] as string, args[1] as boolean)
        return undefined
      case 'setNavCapabilities':
        store.setNavCapabilities(args[0] as string, args[1] as boolean, args[2] as boolean)
        return undefined
      case 'setAgentControlled':
        store.setAgentControlled(args[0] as string, args[1] as boolean)
        return undefined
      case 'setZoomFactor':
        store.setZoomFactor(args[0] as string, args[1] as number)
        return undefined
      case 'stepZoom':
        return store.stepZoom(args[0] as string, args[1] as 'in' | 'out')
      case 'resetZoom':
        store.resetZoom(args[0] as string)
        return undefined
      case 'setViewportPreset':
        store.setViewportPreset(args[0] as string, args[1] as ViewportPreset)
        return undefined
      case 'cycleViewportPreset':
        return store.cycleViewportPreset(args[0] as string)
      case 'toggleViewportOrientation':
        store.toggleViewportOrientation(args[0] as string)
        return undefined
      case 'setCustomViewportWidth':
        store.setCustomViewportWidth(args[0] as string, args[1] as number | undefined)
        return undefined
      case 'setLoadError':
        store.setLoadError(args[0] as string, args[1] as BrowserLoadError | null)
        return undefined
      case 'bumpStopGeneration':
        store.bumpStopGeneration(args[0] as string)
        return undefined
      case 'clearRecentUrls':
        store.clearRecentUrls()
        return undefined
      default:
        throw new Error(`browser-session store has no action ${action}`)
    }
  }
}
