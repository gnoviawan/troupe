/**
 * Troupe Comu consent tickets and Stop facade, across desktop and remote clients.
 *
 * Host-minted Comu tickets ride `acp:permission_request` with a `host:<uuid>`
 * `requestId` and `_meta.termul.origin_kind === "comu"`. Answering reuses
 * `HostTickets::answer` (`browser_ticket_respond` on desktop `main` webview,
 * `/ws respond_permission` on authenticated remote clients).
 */

import type {
  ComuConsentAction,
  ComuConsentOptionId,
  ComuConsentTicket
} from '@shared/types/comu.types'
import { getAcpTransport } from './acp-transport'
import { type BrowserTicketAnswerResult, HOST_TICKET_PREFIX } from './browser-consent-api'
import { postJsonOnce } from './ipc/http'
import { invokeIpc } from './ipc/tauri'
import { isTauriContext } from './tauri-runtime'

const OPTION_IDS: readonly ComuConsentOptionId[] = ['allow_session', 'allow_project', 'deny']
const ACTIONS: readonly ComuConsentAction[] = ['drive_app', 'foreground_desktop', 'clipboard_read']

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null
}

/**
 * Parse a host-minted Comu consent ticket from an `acp:permission_request`
 * payload, or return `null` for agent/browser permission requests.
 */
export function parseComuConsentTicket(event: unknown): ComuConsentTicket | null {
  if (!isRecord(event)) return null
  const { requestId, sessionId, options, _meta: meta } = event
  if (typeof requestId !== 'string' || !requestId.startsWith(HOST_TICKET_PREFIX)) return null
  if (typeof sessionId !== 'string' || sessionId.length === 0) return null
  const termul = isRecord(meta) && isRecord(meta.termul) ? meta.termul : null
  if (termul?.origin_kind !== 'comu' || !isRecord(termul.comu)) return null
  const comu = termul.comu
  if (typeof comu.appName !== 'string' || typeof comu.appId !== 'string') return null
  const action = ACTIONS.find((a) => a === comu.action) ?? 'drive_app'
  const toolName = typeof comu.toolName === 'string' ? comu.toolName : 'comu_click'
  const offered = Array.isArray(options)
    ? options.flatMap((o) => {
        const id = isRecord(o) ? OPTION_IDS.find((known) => known === o.optionId) : undefined
        return id ? [id] : []
      })
    : []
  return {
    requestId,
    sessionId,
    appName: comu.appName,
    appId: comu.appId,
    action,
    toolName,
    options: offered.length > 0 ? offered : [...OPTION_IDS]
  }
}

function classify(code: string | undefined): BrowserTicketAnswerResult {
  switch (code) {
    case 'TICKET_NOT_FOUND':
    case 'TICKET_ALREADY_RESOLVED':
    case 'stale':
    case 'already_resolved':
      return 'stale'
    case 'TICKET_TOO_EARLY':
    case 'rate_limited':
      return 'too_early'
    default:
      return 'refused'
  }
}

function errorCode(error: unknown): string | undefined {
  return isRecord(error) && typeof error.code === 'string' ? error.code : undefined
}

/** Answer a pending Comu consent ticket on desktop or remote `/ws`. */
export async function answerComuConsent(
  ticket: Pick<ComuConsentTicket, 'requestId'>,
  optionId: ComuConsentOptionId,
  agentId = ''
): Promise<BrowserTicketAnswerResult> {
  if (isTauriContext()) {
    const result = await invokeIpc<null>('browser_ticket_respond', {
      requestId: ticket.requestId,
      optionId
    })
    return result.success ? 'ok' : classify(result.code)
  }
  try {
    await getAcpTransport().respondPermission(agentId, ticket.requestId, optionId)
    return 'ok'
  } catch (error) {
    return classify(errorCode(error))
  }
}

/** Stop Computer Use for `sessionId` until the user's next message. */
export async function stopComuAgent(sessionId: string): Promise<boolean> {
  if (isTauriContext()) {
    const result = await invokeIpc<boolean>('comu_stop', { sessionId })
    return result.success
  }
  const result = await postJsonOnce<boolean>('/host/comu/stop', { sessionId })
  return result.success
}
