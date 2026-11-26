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
  resolve: (ack: { ok: boolean; id?: string; sessionId?: string; error?: string }) => void
  timer: number
}

interface BridgeGlobals {
  __TERMUL_AGENTATION_ENDPOINT__?: string
  __TERMUL_AGENTATION_SESSION_ID__?: string
  __TERMUL_AGENTATION_NONCE__?: string
  __TERMUL_TAB_ID__?: string
}

export class OverlayTransport {
  private host: HTMLElement
  private sessionId: string | null = null
  private sessionPromise: Promise<string | null> | null = null
  private bridgeNonce: string | null = null
  private reqSeq = 0
  private pendingOutbox = new Map<string, PendingOutboxRequest>()

  constructor(host: HTMLElement) {
    this.host = host
    this.captureBridgeNonce()
  }

  public captureBridgeNonce(): void {
    const win = window as unknown as BridgeGlobals
    if (typeof win.__TERMUL_AGENTATION_NONCE__ === 'string' && win.__TERMUL_AGENTATION_NONCE__) {
      this.bridgeNonce = win.__TERMUL_AGENTATION_NONCE__
      try {
        delete win.__TERMUL_AGENTATION_NONCE__
      } catch {
        // Non-configurable in some environments
      }
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
    if (!msg.reqId) return
    const pending = this.pendingOutbox.get(msg.reqId)
    if (!pending) return
    this.pendingOutbox.delete(msg.reqId)
    window.clearTimeout(pending.timer)
    if (msg.sessionId) {
      this.sessionId = msg.sessionId
    }
    pending.resolve({
      ok: Boolean(msg.ok),
      id: msg.id,
      sessionId: msg.sessionId,
      error: msg.error
    })
  }

  public destroy(): void {
    for (const [, pending] of this.pendingOutbox) {
      window.clearTimeout(pending.timer)
      pending.resolve({ ok: false, error: 'Overlay unmounted' })
    }
    this.pendingOutbox.clear()
  }

  private sendViaHostBridge(
    kind: 'annotation' | 'action',
    payload: Record<string, unknown>
  ): Promise<{ ok: boolean; id?: string; error?: string }> {
    this.captureBridgeNonce()
    if (!this.bridgeNonce) {
      return Promise.resolve({ ok: false, error: 'Host bridge unavailable' })
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

    return new Promise((resolve) => {
      const timer = window.setTimeout(() => {
        this.pendingOutbox.delete(reqId)
        resolve({ ok: false, error: 'Host bridge timed out' })
      }, OUTBOX_TIMEOUT_MS)
      this.pendingOutbox.set(reqId, { resolve, timer })
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
    this.captureBridgeNonce()
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
      // Fall through to HTTP attempt only if bridge failed unexpectedly
    }

    const ep = this.getEndpoint()
    if (ep) {
      const sid = await this.ensureHttpSession()
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

    if (!this.preferHostBridge() && this.bridgeNonce) {
      const bridged = await this.sendViaHostBridge('annotation', body)
      if (bridged.ok) return { ok: true, id: bridged.id }
    }

    if (!ep && !this.bridgeNonce) {
      return { ok: true }
    }
    return { ok: false }
  }

  public async postAction(payload: Record<string, unknown>): Promise<boolean> {
    this.captureBridgeNonce()
    const output = JSON.stringify({
      viewportWidth: window.innerWidth,
      ...payload,
      tabId: this.getTabId()
    })

    if (this.preferHostBridge()) {
      const bridged = await this.sendViaHostBridge('action', { output })
      if (bridged.ok) return true
    }

    const ep = this.getEndpoint()
    if (ep) {
      const sid = await this.ensureHttpSession()
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

    if (!this.preferHostBridge() && this.bridgeNonce) {
      const bridged = await this.sendViaHostBridge('action', { output })
      return bridged.ok
    }

    return false
  }
}
