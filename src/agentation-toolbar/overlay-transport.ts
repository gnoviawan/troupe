/**
 * Dual-path transport for the injected Troupe annotation overlay.
 *
 * - On `http://` pages, tries direct loopback HTTP first and automatically falls
 *   back to the host DOM outbox bridge if CSP/network blocks the fetch.
 * - On `https://` pages (including strict-CSP / WKWebView mixed-content sites),
 *   uses the host DOM outbox bridge directly so no blocked `http://127.0.0.1`
 *   requests ever run in page context.
 */

export const OUTBOX_ATTR = 'data-troupe-outbox'
const OUTBOX_TIMEOUT_MS = 6000

export interface OutboxAckMessage {
  reqId?: string
  ok?: boolean
  id?: string
  sessionId?: string
  error?: string
}

interface PendingOutboxRequest {
  resolve: (ack: {
    ok: boolean
    id?: string
    sessionId?: string
    error?: string
    drained?: boolean
  }) => void
  timer: number
}

interface BridgeGlobals {
  __TERMUL_AGENTATION_ENDPOINT__?: string
  __TERMUL_AGENTATION_SESSION_ID__?: string
  __TERMUL_TAB_ID__?: string
}

interface HostBridgeResult {
  ok: boolean
  id?: string
  error?: string
  /** True when the envelope was already drained by the host before timeout. */
  drained?: boolean
}

export class OverlayTransport {
  private host: HTMLElement
  private sessionId: string | null = null
  private sessionPromise: Promise<string | null> | null = null
  private bridgeSessionInit: Promise<void> | null = null
  private bridgeNonce: string | null = null
  private disposed = false
  private reqSeq = 0
  private pendingOutbox = new Map<string, PendingOutboxRequest>()

  constructor(host: HTMLElement, bridgeNonce?: string | null) {
    this.host = host
    this.setBridgeNonce(bridgeNonce)
  }

  public setBridgeNonce(nonce: string | null | undefined): void {
    if (typeof nonce === 'string' && nonce.length > 0) {
      this.bridgeNonce = nonce
    }
  }

  private getEndpoint(): string | undefined {
    return (window as unknown as BridgeGlobals).__TERMUL_AGENTATION_ENDPOINT__
  }

  private getTabId(): string | undefined {
    return (window as unknown as BridgeGlobals).__TERMUL_TAB_ID__
  }

  private preferHostBridge(): boolean {
    return Boolean(this.bridgeNonce) && window.location.protocol === 'https:'
  }

  public handleOutboxAck(msg: OutboxAckMessage): void {
    if (msg.sessionId) {
      this.sessionId = msg.sessionId
    }
    if (!msg.reqId) return
    const pending = this.pendingOutbox.get(msg.reqId)
    if (!pending) return
    this.pendingOutbox.delete(msg.reqId)
    window.clearTimeout(pending.timer)
    pending.resolve({
      ok: Boolean(msg.ok),
      id: msg.id,
      sessionId: msg.sessionId,
      error: msg.error
    })
  }

  /**
   * Remove a queued envelope by `reqId` from `OUTBOX_ATTR`.
   * Returns `true` if it was still in the DOM outbox and reclaimed before the
   * host drained it, or `false` if the host already drained it.
   */
  private removeFromOutbox(reqId: string): boolean {
    const existing = this.host.getAttribute(OUTBOX_ATTR)
    if (!existing) return false
    try {
      const parsed = JSON.parse(existing)
      if (!Array.isArray(parsed)) return false
      const next = parsed.filter(
        (item) =>
          !(item && typeof item === 'object' && (item as { reqId?: string }).reqId === reqId)
      )
      if (next.length === parsed.length) return false
      if (next.length === 0) {
        this.host.removeAttribute(OUTBOX_ATTR)
      } else {
        this.host.setAttribute(OUTBOX_ATTR, JSON.stringify(next))
      }
      return true
    } catch {
      return false
    }
  }

  public destroy(): void {
    this.disposed = true
    for (const [reqId, pending] of this.pendingOutbox) {
      window.clearTimeout(pending.timer)
      const reclaimed = this.removeFromOutbox(reqId)
      pending.resolve({ ok: false, error: 'Overlay unmounted', drained: !reclaimed })
    }
    this.pendingOutbox.clear()
  }

  /**
   * Coalesce concurrent initial submissions until the first submission establishes
   * `this.sessionId` (or finishes initialization). Call `releaseGate()` as soon as
   * `sessionId` is known so waiting submissions do not block behind slow HTTP POSTs.
   */
  private async withSessionInitGate<T>(
    disposedFallback: T,
    fn: (releaseGate: () => void) => Promise<T>
  ): Promise<T> {
    if (this.disposed) return disposedFallback
    while (!this.sessionId && this.bridgeSessionInit) {
      await this.bridgeSessionInit
      if (this.disposed) return disposedFallback
    }
    if (this.disposed) return disposedFallback

    let finishInit: (() => void) | null = null
    if (!this.sessionId) {
      this.bridgeSessionInit = new Promise<void>((resolve) => {
        finishInit = resolve
      })
    }

    const releaseGate = () => {
      if (finishInit) {
        const done = finishInit
        finishInit = null
        this.bridgeSessionInit = null
        done()
      }
    }

    try {
      return await fn(releaseGate)
    } finally {
      releaseGate()
    }
  }

  private async sendViaHostBridge(
    kind: 'annotation' | 'action',
    payload: Record<string, unknown>
  ): Promise<HostBridgeResult> {
    if (!this.bridgeNonce) {
      return { ok: false, error: 'Host bridge unavailable', drained: false }
    }

    const reqId = `ov-${Date.now()}-${++this.reqSeq}`
    const envelope = {
      reqId,
      nonce: this.bridgeNonce,
      kind,
      url: window.location.href,
      sessionId: this.sessionId ?? undefined,
      payload
    }

    let queue: unknown[] = []
    const existing = this.host.getAttribute(OUTBOX_ATTR)
    if (existing) {
      try {
        const parsed = JSON.parse(existing)
        if (Array.isArray(parsed)) queue = parsed
      } catch {
        queue = []
      }
    }
    queue.push(envelope)
    this.host.setAttribute(OUTBOX_ATTR, JSON.stringify(queue))

    return await new Promise<HostBridgeResult>((resolve) => {
      const timer = window.setTimeout(() => {
        this.pendingOutbox.delete(reqId)
        const reclaimed = this.removeFromOutbox(reqId)
        resolve({ ok: false, error: 'Host bridge timed out', drained: !reclaimed })
      }, OUTBOX_TIMEOUT_MS)
      this.pendingOutbox.set(reqId, {
        resolve: (ack) => resolve({ drained: false, ...ack }),
        timer
      })
    })
  }

  private async ensureHttpSession(): Promise<string | null> {
    if (this.sessionId) return this.sessionId
    const preset = (window as unknown as BridgeGlobals).__TERMUL_AGENTATION_SESSION_ID__
    if (preset) {
      this.sessionId = preset
      return preset
    }
    const ep = this.getEndpoint()
    if (!ep) return null
    if (!this.sessionPromise) {
      this.sessionPromise = (async () => {
        try {
          const res = await fetch(`${ep}/sessions`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ url: window.location.href })
          })
          if (!res.ok) return null
          const json = (await res.json()) as { id?: string }
          if (json.id) {
            this.sessionId = json.id
            return json.id
          }
        } catch {
          // Caller falls back to host bridge when available
        }
        return null
      })().finally(() => {
        if (!this.sessionId) this.sessionPromise = null
      })
    }
    return this.sessionPromise
  }

  public async postAnnotation(
    payload: Record<string, unknown>
  ): Promise<{ ok: boolean; id?: string }> {
    return this.withSessionInitGate({ ok: false }, async (releaseGate) => {
      const body = {
        url: window.location.href,
        timestamp: Date.now(),
        tabId: this.getTabId(),
        viewportWidth: window.innerWidth,
        ...payload
      }

      if (this.preferHostBridge()) {
        const bridged = await this.sendViaHostBridge('annotation', body)
        if (bridged.ok) return { ok: true, id: bridged.id }
        // Never fall back to HTTP if the host already drained the outbox envelope,
        // otherwise a slow host ack plus HTTP fallback would duplicate the annotation.
        if (bridged.drained || this.disposed) return { ok: false }
      }

      const ep = this.getEndpoint()
      if (ep) {
        const sid = await this.ensureHttpSession()
        releaseGate()
        if (this.disposed) return { ok: false }
        if (sid) {
          try {
            const res = await fetch(`${ep}/sessions/${sid}/annotations`, {
              method: 'POST',
              headers: { 'Content-Type': 'application/json' },
              body: JSON.stringify(body)
            })
            if (res.ok) {
              const parsed = (await res.json().catch(() => null)) as { id?: string } | null
              return { ok: true, id: parsed?.id }
            }
          } catch {
            // Fall back to host bridge below
          }
        }
      }

      if (!this.preferHostBridge() && this.bridgeNonce && !this.disposed) {
        const bridged = await this.sendViaHostBridge('annotation', body)
        if (bridged.ok) return { ok: true, id: bridged.id }
      }

      if (!ep && !this.bridgeNonce) {
        return { ok: true }
      }
      return { ok: false }
    })
  }

  public async postAction(payload: Record<string, unknown>): Promise<boolean> {
    return this.withSessionInitGate(false, async (releaseGate) => {
      const output = JSON.stringify({
        viewportWidth: window.innerWidth,
        ...payload,
        tabId: this.getTabId()
      })

      if (this.preferHostBridge()) {
        const bridged = await this.sendViaHostBridge('action', { output })
        if (bridged.ok) return true
        if (bridged.drained || this.disposed) return false
      }

      const ep = this.getEndpoint()
      if (ep) {
        const sid = await this.ensureHttpSession()
        releaseGate()
        if (this.disposed) return false
        if (sid) {
          try {
            const res = await fetch(`${ep}/sessions/${sid}/action`, {
              method: 'POST',
              headers: { 'Content-Type': 'application/json' },
              body: JSON.stringify({
                sessionId: sid,
                tabId: this.getTabId(),
                annotations: [],
                output,
                timestamp: new Date().toISOString()
              })
            })
            if (res.ok) return true
          } catch {
            // Fall back to host bridge below
          }
        }
      }

      if (!this.preferHostBridge() && this.bridgeNonce && !this.disposed) {
        const bridged = await this.sendViaHostBridge('action', { output })
        return bridged.ok
      }

      return false
    })
  }
}
