/**
 * In-chat (and corner fallback) approval card for Troupe Comu consent tickets.
 *
 * Uses `useConsentArmed` (750ms anti-clickjacking floor matching `HostTickets`)
 * and semantic tokens from `DESIGN.md`.
 */

import type {
  ComuConsentAction,
  ComuConsentOptionId,
  ComuConsentTicket
} from '@shared/types/comu.types'
import { useEffect } from 'react'
import { useConsentArmed } from '@/components/browser/consent-intent'
import { Monitor, ShieldAlert, ShieldCheck } from '@/components/icons'
import { Button } from '@/components/ui/button'
import { useAgentDisplayName } from '@/hooks/use-agent-display-name'
import { logFrontendError } from '@/lib/log-api'
import { useAcpStore } from '@/stores/acp-store'
import { useComuControlStore } from '@/stores/comu-control-store'

const loggedRequestIds = new Set<string>()

function comuConsentTitle(action: ComuConsentAction, appName: string): string {
  switch (action) {
    case 'clipboard_read':
      return 'Allow clipboard text read?'
    case 'foreground_desktop':
      return 'Allow foreground desktop control?'
    default:
      return `Allow Computer Use in ${appName}?`
  }
}

function comuConsentSummary(agentName: string | undefined, ticket: ComuConsentTicket): string {
  const who = agentName || 'The agent'
  switch (ticket.action) {
    case 'clipboard_read':
      return `${who} wants to read plain text from your system clipboard.`
    case 'foreground_desktop':
      return `${who} wants to bring ${ticket.appName} to the foreground or inspect the full desktop.`
    default:
      return `${who} wants to inspect and operate ${ticket.appName} on this machine.`
  }
}

function comuOptionLabel(optionId: ComuConsentOptionId, action: ComuConsentAction): string {
  if (optionId === 'deny') return 'Deny'
  if (optionId === 'allow_project') return 'Always allow in this project'
  return action === 'clipboard_read' ? 'Allow once' : 'Allow for this chat'
}

export function ComuConsentCard({
  consent
}: {
  consent: ComuConsentTicket
}): React.JSX.Element | null {
  const respond = useComuControlStore((s) => s.respondConsent)
  const agentId = useAcpStore((s) => s.sessions[consent.sessionId]?.agentId)
  const agentName = useAgentDisplayName(agentId)
  const armed = useConsentArmed(consent.requestId)

  useEffect(() => {
    if (loggedRequestIds.has(consent.requestId)) return
    loggedRequestIds.add(consent.requestId)
    void logFrontendError({
      level: 'info',
      message: `comu consent card hosting prompt: requestId=${consent.requestId}`,
      source: 'ComuConsentCard'
    })
  }, [consent.requestId])

  const titleId = `comu-consent-title-${consent.requestId}`

  return (
    <section
      aria-labelledby={titleId}
      aria-live="polite"
      className="rounded-lg border border-border bg-card px-3.5 py-3 shadow-md sm:px-4"
      data-testid="comu-consent-card"
    >
      <div className="flex items-start gap-2.5">
        <div className="min-w-0 flex-1">
          <div className="flex items-center justify-between gap-2">
            <h2 id={titleId} className="text-sm font-medium leading-5 text-foreground">
              {comuConsentTitle(consent.action, consent.appName)}
            </h2>
            <span className="inline-flex h-5 shrink-0 items-center gap-1 rounded-full bg-primary/15 px-2 text-xs font-medium text-primary">
              <Monitor size={11} aria-hidden="true" />
              Computer Use
            </span>
          </div>
          <p className="mt-0.5 text-xs leading-relaxed text-muted-foreground">
            {comuConsentSummary(agentName, consent)}
          </p>
          <dl className="mt-2 grid grid-cols-[auto_minmax(0,1fr)] gap-x-2 gap-y-0.5 text-2xs leading-relaxed">
            <dt className="text-muted-foreground">Target</dt>
            <dd className="truncate font-medium text-foreground">{consent.appName}</dd>
            <dt className="text-muted-foreground">Identifier</dt>
            <dd className="break-all font-mono text-muted-foreground">{consent.appId}</dd>
            <dt className="text-muted-foreground">Tool</dt>
            <dd className="font-mono text-muted-foreground">{consent.toolName}</dd>
          </dl>
        </div>
      </div>

      <fieldset className="mt-2.5 flex flex-wrap items-center gap-2">
        <legend className="sr-only">Computer Use consent options</legend>
        {consent.options.map((optionId) => (
          <Button
            key={optionId}
            variant={optionId === 'allow_session' ? 'default' : 'outline'}
            size="sm"
            disabled={!armed}
            onClick={() => respond(consent.requestId, optionId, agentId ?? '')}
            className={
              optionId === 'deny'
                ? 'h-8 min-w-0 rounded-lg border-transparent px-3 text-xs font-medium text-muted-foreground whitespace-nowrap transition-colors duration-150 hover:border-destructive/25 hover:bg-destructive/10 hover:text-destructive'
                : 'h-8 min-w-0 rounded-lg px-3 text-xs font-medium whitespace-nowrap transition-[scale,color,background-color,border-color] duration-150 ease-[cubic-bezier(0.23,1,0.32,1)] active:scale-[0.97] motion-reduce:active:scale-100'
            }
          >
            {optionId === 'deny' ? (
              <ShieldAlert size={14} className="shrink-0" aria-hidden="true" />
            ) : (
              <ShieldCheck size={14} className="shrink-0" aria-hidden="true" />
            )}
            <span className="truncate">{comuOptionLabel(optionId, consent.action)}</span>
          </Button>
        ))}
      </fieldset>
    </section>
  )
}
