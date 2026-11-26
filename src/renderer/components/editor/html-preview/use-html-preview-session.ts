import { useCallback, useEffect, useRef, useState } from 'react'
import { type HtmlPreviewSession, htmlPreviewApi } from '@/lib/html-preview-api'
import { logFrontendError } from '@/lib/log-api'
import { basename } from '@/lib/path-basename'
import { injectConsoleBridge } from './console-bridge'

/** Typing pauses this long before the preview reloads. */
export const UPDATE_DEBOUNCE_MS = 200

export type PreviewSessionState =
  | { status: 'opening' }
  | { status: 'failed' }
  | { status: 'open'; src: string }

/** The iframe URL; `reload` changes it, so the frame navigates again. */
export function previewSrc(session: HtmlPreviewSession, reload: number): string {
  return `${session.baseUrl}/preview/${session.token}/${encodeURIComponent(session.entry)}?r=${reload}`
}

function logFailure(source: string, filePath: string, code: string): void {
  void logFrontendError({
    level: 'warn',
    source,
    message: `html preview of '${basename(filePath)}' failed: ${code}`
  })
}

/**
 * A host preview session for `filePath` that serves `content` (the editor
 * buffer) as the page. Opens on mount and on path change, closes on unmount;
 * each buffer change is sent after a short pause and reloads the frame.
 * `reload` resends the buffer and reloads the frame, reopens a session the
 * host forgot, or retries the open after a failure.
 */
export function useHtmlPreviewSession(
  filePath: string,
  content: string
): { state: PreviewSessionState; reload: () => void } {
  const [session, setSession] = useState<HtmlPreviewSession | null>(null)
  const [failed, setFailed] = useState(false)
  const [reloads, setReloads] = useState(0)
  // A host restart forgets sessions; bumping this opens a new one.
  const [generation, setGeneration] = useState(0)
  const contentRef = useRef(content)
  contentRef.current = content
  /** The last buffer the host accepted. */
  const sentRef = useRef<string | null>(null)
  // Updates go out one at a time: the host keeps whichever arrives last.
  const queueRef = useRef<Promise<unknown>>(Promise.resolve())

  const send = useCallback(
    (token: string, text: string): Promise<boolean> => {
      const result = queueRef.current.then(() =>
        htmlPreviewApi.update(token, injectConsoleBridge(text))
      )
      queueRef.current = result.catch(() => undefined)
      return result.then(
        (sent) => {
          if (sent.success) {
            sentRef.current = text
            return true
          }
          logFailure('htmlPreview.update', filePath, sent.code)
          if (sent.code === 'SESSION_NOT_FOUND') setGeneration((count) => count + 1)
          return false
        },
        () => false
      )
    },
    [filePath]
  )

  // biome-ignore lint/correctness/useExhaustiveDependencies: `generation` reopens on purpose
  useEffect(() => {
    let live = true
    let token: string | null = null
    setSession(null)
    setFailed(false)
    sentRef.current = null
    void (async () => {
      const opened = await htmlPreviewApi.open(filePath)
      if (!opened.success) {
        logFailure('htmlPreview.open', filePath, opened.code)
        if (live) setFailed(true)
        return
      }
      token = opened.data.token
      if (!live) {
        void htmlPreviewApi.close(token)
        return
      }
      await send(token, contentRef.current)
      if (live) setSession(opened.data)
    })()
    return () => {
      live = false
      if (token) void htmlPreviewApi.close(token)
    }
  }, [filePath, generation])

  useEffect(() => {
    if (!session || content === sentRef.current) return
    const timer = setTimeout(() => {
      void send(session.token, content).then((ok) => {
        if (ok) setReloads((count) => count + 1)
      })
    }, UPDATE_DEBOUNCE_MS)
    return () => clearTimeout(timer)
  }, [content, session, send])

  const reload = useCallback(() => {
    if (failed || !session) {
      setGeneration((count) => count + 1)
      return
    }
    void send(session.token, contentRef.current).then(() => setReloads((count) => count + 1))
  }, [failed, session, send])

  const state: PreviewSessionState = failed
    ? { status: 'failed' }
    : session
      ? { status: 'open', src: previewSrc(session, reloads) }
      : { status: 'opening' }
  return { state, reload }
}
