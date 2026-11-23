import { resolveLocalImagePath } from '@/components/chat/chat-markdown-images'
import { readMediaBytes } from '@/lib/media-bytes-api'

const PASSTHROUGH_URL_RE = /^(?:https?:|data:|blob:)/i

export interface EditorFileUrlResolver {
  resolveFileUrl: (url: string) => Promise<string>
  dispose: () => void
}

function directoryOf(filePath: string): string {
  return filePath.replace(/[\\/][^\\/]*$/, '') || '/'
}

/**
 * Turns a markdown image destination into a URL the webview can load. A local
 * path, relative ones against the open file's folder, is read through the host
 * and shown from an object URL, so it works on desktop and in the browser
 * client alike. Anything unreadable comes back unchanged and shows as broken.
 */
export function createEditorFileUrlResolver(getFilePath: () => string): EditorFileUrlResolver {
  const objectUrls = new Map<string, Promise<string | null>>()

  const read = (path: string): Promise<string | null> => {
    const cached = objectUrls.get(path)
    if (cached) return cached
    const pending = readMediaBytes(path).then((result) => {
      if (result.success) return URL.createObjectURL(result.data)
      objectUrls.delete(path)
      return null
    })
    objectUrls.set(path, pending)
    return pending
  }

  return {
    async resolveFileUrl(url) {
      if (PASSTHROUGH_URL_RE.test(url.trim())) return url
      const path = resolveLocalImagePath(url, directoryOf(getFilePath()))
      if (path === null) return url
      return (await read(path)) ?? url
    },
    dispose() {
      const pending = [...objectUrls.values()]
      objectUrls.clear()
      for (const objectUrl of pending) {
        void objectUrl.then((value) => value && URL.revokeObjectURL(value))
      }
    }
  }
}
