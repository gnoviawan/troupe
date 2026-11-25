import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { isTauriContext } from '@/lib/tauri-runtime'
import { randomUUID } from '@/lib/uuid'

export interface BrowserBounds {
  x: number
  y: number
  width: number
  height: number
}

export interface BrowserTabInfo {
  id: string
  url: string
  title: string
  /** True when the host claimed this webview for a pending agent open. */
  agentControlled: boolean
}

/** One tab as the host's tab registry reports it, from engine events. */
export interface BrowserTabView {
  tabId: string
  url: string
  title: string
  loading: boolean
  agentControlled: boolean
  /** Bumped on every new document the tab commits. */
  epoch: number
}

export interface BrowserScreenshotPayload {
  base64: string
  mimeType: string
}

export interface LocalDevServer {
  host: string
  port: number
  url: string
  label: string
}

export type BrowserAnnotationCommand =
  | { type: 'set_active'; active: boolean }
  | { type: 'set_tool'; tool: 'comment' | 'rectangle' | 'ruler' | 'screenshot-area' }
  | { type: 'set_pause'; paused: boolean }
  | { type: 'set_pins_hidden'; hidden: boolean }
  | { type: 'place_guide'; axis: 'horizontal' | 'vertical' }
  | { type: 'clear'; ids?: string[] }

export interface BrowserEventSubscription {
  unlisten: () => void
}

export type IpcResult<T> =
  | { success: true; data: T }
  | { success: false; error: string; code: string }

const USER_TAB_LABEL = /^tab-[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/

/** A label the host registry accepts for a tab the user opens. Agent tab
 *  labels are minted by the host only. */
export function newUserBrowserTabId(): string {
  return `tab-${randomUUID().toLowerCase()}`
}

export function isUserBrowserTabId(id: string): boolean {
  return USER_TAB_LABEL.test(id)
}

function notTauriError<T>(feature: string): IpcResult<T> {
  return {
    success: false,
    error: `${feature} requires Tauri desktop`,
    code: 'NOT_TAURI'
  }
}

export async function browserTabCreate(
  tabId: string,
  url: string,
  bounds: BrowserBounds
): Promise<IpcResult<BrowserTabInfo>> {
  return invoke('browser_tab_create', { tabId, url, bounds })
}

export async function browserTabNavigate(tabId: string, url: string): Promise<IpcResult<void>> {
  return invoke('browser_tab_navigate', { tabId, url })
}

export async function browserTabResize(
  tabId: string,
  bounds: BrowserBounds
): Promise<IpcResult<void>> {
  return invoke('browser_tab_resize', { tabId, bounds })
}

export async function browserTabShow(tabId: string): Promise<IpcResult<void>> {
  return invoke('browser_tab_show', { tabId })
}

export async function browserTabHide(tabId: string): Promise<IpcResult<void>> {
  return invoke('browser_tab_hide', { tabId })
}

export async function browserTabDestroy(tabId: string): Promise<IpcResult<void>> {
  return invoke('browser_tab_destroy', { tabId })
}

export async function browserTabGoBack(tabId: string): Promise<IpcResult<void>> {
  return invoke('browser_tab_go_back', { tabId })
}

export async function browserTabGoForward(tabId: string): Promise<IpcResult<void>> {
  return invoke('browser_tab_go_forward', { tabId })
}

export async function browserTabReload(tabId: string): Promise<IpcResult<void>> {
  return invoke('browser_tab_reload', { tabId })
}

export async function browserTabStop(tabId: string): Promise<IpcResult<void>> {
  if (!isTauriContext()) return notTauriError('Stop loading')
  return invoke('browser_tab_stop', { tabId })
}

export async function browserTabSetZoom(
  tabId: string,
  zoomFactor: number
): Promise<IpcResult<number>> {
  if (!isTauriContext()) return notTauriError('Browser zoom')
  return invoke('browser_tab_set_zoom', { tabId, zoomFactor })
}

export async function browserTabScreenshot(
  tabId: string
): Promise<IpcResult<BrowserScreenshotPayload>> {
  if (!isTauriContext()) return notTauriError('Browser screenshot')
  return invoke('browser_tab_screenshot', { tabId })
}

export async function browserTabAnnotationCmd(
  tabId: string,
  command: BrowserAnnotationCommand
): Promise<IpcResult<void>> {
  if (!isTauriContext()) return notTauriError('Browser annotation')
  return invoke('browser_tab_annotation_cmd', { tabId, command })
}

export async function browserListLocalServers(): Promise<IpcResult<LocalDevServer[]>> {
  if (!isTauriContext()) return notTauriError('Local dev server discovery')
  return invoke('browser_list_local_servers')
}

export async function browserCheckLocalPort(port: number): Promise<IpcResult<boolean>> {
  if (!isTauriContext()) return notTauriError('Local port check')
  return invoke('browser_check_local_port', { port })
}

export async function browserTabOpenDevtools(tabId: string): Promise<IpcResult<void>> {
  return invoke('browser_tab_open_devtools', { tabId })
}

export async function browserTabInjectAgentation(tabId: string): Promise<IpcResult<void>> {
  if (!isTauriContext()) {
    return {
      success: false,
      error: 'Agentation injection requires Tauri desktop',
      code: 'NOT_TAURI'
    }
  }
  return invoke('browser_tab_inject_agentation', { tabId })
}

function createBrowserEventSubscription<T>(
  eventName: string,
  callback: (payload: T) => void
): BrowserEventSubscription {
  const unlistenPromise = listen<T>(eventName, (event) => {
    callback(event.payload)
  })
  return {
    unlisten: () => {
      void unlistenPromise.then((unlisten) => unlisten())
    }
  }
}

/** Every change the host registry makes to a tab's URL, title, loading
 *  state or document epoch. */
export function onBrowserTabUpdated(
  callback: (view: BrowserTabView) => void
): BrowserEventSubscription {
  return createBrowserEventSubscription('browser_tab_updated', callback)
}

export interface AgentationCreatedPayload {
  id: string
  url?: string
  kind?: 'feedback' | 'rectangle' | 'guide' | 'measurement' | 'placement' | 'rearrange'
  element: string
  elementPath: string
  comment: string
  boundingBox?: { x: number; y: number; width: number; height: number }
  measurement?: {
    axis?: 'horizontal' | 'vertical'
    positionPx?: number
    toSelector?: string
    deltaPx?: number
  }
}

export interface AgentationActionPayload {
  sessionId?: string
  output?: string
}

export function onAgentationAnnotationCreated(
  callback: (payload: AgentationCreatedPayload) => void
): BrowserEventSubscription {
  if (!isTauriContext()) return { unlisten: () => {} }
  return createBrowserEventSubscription('agentation:annotation_created', callback)
}

export function onAgentationActionRequested(
  callback: (payload: AgentationActionPayload) => void
): BrowserEventSubscription {
  if (!isTauriContext()) return { unlisten: () => {} }
  return createBrowserEventSubscription('agentation:action_requested', callback)
}
