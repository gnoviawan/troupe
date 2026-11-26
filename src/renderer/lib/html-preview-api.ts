import type { IpcResult } from '@shared/types/ipc.types'
import { postJson } from './ipc/http'
import { invokeIpc } from './ipc/tauri'
import { isTauriContext } from './tauri-runtime'

/**
 * A host-served preview of an `.html` file. The page loads from
 * `${baseUrl}/preview/${token}/${entry}`: the desktop's loopback listener, or
 * the web host itself (`baseUrl` empty).
 */
export interface HtmlPreviewSession {
  token: string
  entry: string
  baseUrl: string
}

/**
 * Desktop invokes the `html_preview_*` commands; web, shared-live and phone
 * post `/html-preview/*`. Both reach the same host registry.
 */
export const htmlPreviewApi = {
  open(path: string): Promise<IpcResult<HtmlPreviewSession>> {
    return isTauriContext()
      ? invokeIpc<HtmlPreviewSession>('html_preview_open', { path })
      : postJson<HtmlPreviewSession>('/html-preview/open', { path })
  },

  /** Serve `content` in place of the file until the session closes. */
  update(token: string, content: string): Promise<IpcResult<null>> {
    return isTauriContext()
      ? invokeIpc<null>('html_preview_update', { token, content })
      : postJson<null>('/html-preview/update', { token, content })
  },

  close(token: string): Promise<IpcResult<null>> {
    return isTauriContext()
      ? invokeIpc<null>('html_preview_close', { token })
      : postJson<null>('/html-preview/close', { token })
  }
}
