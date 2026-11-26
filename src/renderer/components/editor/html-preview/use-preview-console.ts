import { type RefObject, useEffect, useRef, useState } from 'react'
import { type ConsoleEntry, parseBridgeMessage } from './console-bridge'

/** Entries kept per page load; older ones drop off. */
export const MAX_CONSOLE_ENTRIES = 500

/**
 * Console output of the page in `frameRef`, cleared whenever `src` changes
 * (each reload is a new page). Messages from any other window are ignored:
 * every frame on the page can `postMessage` the editor.
 */
export function usePreviewConsole(
  frameRef: RefObject<HTMLIFrameElement | null>,
  src: string | null
): ConsoleEntry[] {
  const [log, setLog] = useState<{ src: string | null; entries: ConsoleEntry[] }>({
    src,
    entries: []
  })
  const srcRef = useRef(src)
  srcRef.current = src

  useEffect(() => {
    const onMessage = (event: MessageEvent): void => {
      const frame = frameRef.current
      if (!frame?.contentWindow || event.source !== frame.contentWindow) return
      const entry = parseBridgeMessage(event.data)
      if (!entry) return
      const shown = srcRef.current
      setLog((current) => {
        const entries = current.src === shown ? current.entries : []
        return { src: shown, entries: [...entries, entry].slice(-MAX_CONSOLE_ENTRIES) }
      })
    }
    window.addEventListener('message', onMessage)
    return () => window.removeEventListener('message', onMessage)
  }, [frameRef])

  return log.src === src ? log.entries : []
}
