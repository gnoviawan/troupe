import type { IpcResult } from '@shared/types/ipc.types'
import { listen } from '@tauri-apps/api/event'
import { invokeIpc } from '@/lib/ipc/tauri'
import { isTauriContext } from '@/lib/tauri-runtime'
import { randomUUID } from '@/lib/uuid'

export type { IpcResult }

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
  | { type: 'set_capture_hidden'; hidden: boolean }
  | { type: 'place_guide'; axis: 'horizontal' | 'vertical' }
  | { type: 'clear'; ids?: string[] }

export interface BrowserEventSubscription {
  unlisten: () => void
}

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
  if (!isTauriContext()) return notTauriError('Browser tabs')
  return invokeIpc('browser_tab_create', { tabId, url, bounds })
}

export async function browserTabNavigate(tabId: string, url: string): Promise<IpcResult<void>> {
  if (!isTauriContext()) return notTauriError('Browser navigation')
  return invokeIpc('browser_tab_navigate', { tabId, url })
}

export async function browserTabResize(
  tabId: string,
  bounds: BrowserBounds
): Promise<IpcResult<void>> {
  if (!isTauriContext()) return notTauriError('Browser resize')
  return invokeIpc('browser_tab_resize', { tabId, bounds })
}

export async function browserTabShow(tabId: string): Promise<IpcResult<void>> {
  if (!isTauriContext()) return notTauriError('Browser tabs')
  return invokeIpc('browser_tab_show', { tabId })
}

export async function browserTabHide(tabId: string): Promise<IpcResult<void>> {
  if (!isTauriContext()) return notTauriError('Browser tabs')
  return invokeIpc('browser_tab_hide', { tabId })
}

export async function browserTabDestroy(tabId: string): Promise<IpcResult<void>> {
  if (!isTauriContext()) return notTauriError('Browser tabs')
  return invokeIpc('browser_tab_destroy', { tabId })
}

export async function browserTabGoBack(tabId: string): Promise<IpcResult<void>> {
  if (!isTauriContext()) return notTauriError('Browser history')
  return invokeIpc('browser_tab_go_back', { tabId })
}

export async function browserTabGoForward(tabId: string): Promise<IpcResult<void>> {
  if (!isTauriContext()) return notTauriError('Browser history')
  return invokeIpc('browser_tab_go_forward', { tabId })
}

export async function browserTabReload(tabId: string): Promise<IpcResult<void>> {
  if (!isTauriContext()) return notTauriError('Browser reload')
  return invokeIpc('browser_tab_reload', { tabId })
}

export async function browserTabStop(tabId: string): Promise<IpcResult<void>> {
  if (!isTauriContext()) return notTauriError('Stop loading')
  return invokeIpc('browser_tab_stop', { tabId })
}

export async function browserTabSetZoom(
  tabId: string,
  zoomFactor: number
): Promise<IpcResult<number>> {
  if (!isTauriContext()) return notTauriError('Browser zoom')
  return invokeIpc('browser_tab_set_zoom', { tabId, zoomFactor })
}

export async function browserTabScreenshot(
  tabId: string
): Promise<IpcResult<BrowserScreenshotPayload>> {
  if (!isTauriContext()) return notTauriError('Browser screenshot')
  return invokeIpc('browser_tab_screenshot', { tabId })
}

export async function browserTabAnnotationCmd(
  tabId: string,
  command: BrowserAnnotationCommand
): Promise<IpcResult<void>> {
  if (!isTauriContext()) return notTauriError('Browser annotation')
  return invokeIpc('browser_tab_annotation_cmd', { tabId, command })
}

export async function browserTabPollAnnotationOutbox(tabId: string): Promise<IpcResult<number>> {
  if (!isTauriContext()) return notTauriError('Annotation outbox sync')
  return invokeIpc('browser_tab_poll_annotation_outbox', { tabId })
}

export async function browserAnnotationsSync(
  action: 'delete' | 'resolve',
  ids: string[]
): Promise<IpcResult<number>> {
  if (!isTauriContext()) return notTauriError('Annotation sync')
  return invokeIpc('browser_annotations_sync', { action, ids })
}

export async function browserSaveAnnotationCrop(
  annotationId: string | undefined,
  base64Png: string
): Promise<IpcResult<string>> {
  if (!isTauriContext()) return notTauriError('Save annotation crop')
  return invokeIpc('browser_save_annotation_crop', {
    annotationId: annotationId ?? null,
    base64Png
  })
}

export async function browserListLocalServers(): Promise<IpcResult<LocalDevServer[]>> {
  if (!isTauriContext()) return notTauriError('Local dev server discovery')
  return invokeIpc('browser_list_local_servers')
}

export async function browserCheckLocalPort(port: number): Promise<IpcResult<boolean>> {
  if (!isTauriContext()) return notTauriError('Local port check')
  return invokeIpc('browser_check_local_port', { port })
}

export async function browserTabOpenDevtools(tabId: string): Promise<IpcResult<void>> {
  if (!isTauriContext()) return notTauriError('Browser devtools')
  return invokeIpc('browser_tab_open_devtools', { tabId })
}

export async function browserTabInjectAgentation(tabId: string): Promise<IpcResult<void>> {
  if (!isTauriContext()) return notTauriError('Agentation injection')
  return invokeIpc('browser_tab_inject_agentation', { tabId })
}

function createBrowserEventSubscription<T>(
  eventName: string,
  callback: (payload: T) => void
): BrowserEventSubscription {
  if (!isTauriContext()) return { unlisten: () => {} }
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
  tabId?: string
  viewportWidth?: number
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
    gapPx?: number
  }
}

export interface AgentationUpdatedPayload {
  id: string
  tabId?: string
  status?: 'pending' | 'acknowledged' | 'resolved' | 'dismissed'
}

export interface AgentationActionPayload {
  sessionId?: string
  tabId?: string
  output?: string
}

export function onAgentationAnnotationCreated(
  callback: (payload: AgentationCreatedPayload) => void
): BrowserEventSubscription {
  return createBrowserEventSubscription('agentation:annotation_created', callback)
}

export function onAgentationAnnotationUpdated(
  callback: (payload: AgentationUpdatedPayload) => void
): BrowserEventSubscription {
  return createBrowserEventSubscription('agentation:annotation_updated', callback)
}

export function onAgentationActionRequested(
  callback: (payload: AgentationActionPayload) => void
): BrowserEventSubscription {
  return createBrowserEventSubscription('agentation:action_requested', callback)
}
