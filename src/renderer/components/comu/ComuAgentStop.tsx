/**
 * Live in-chat indicator with Stop button shown whenever an agent operates
 * desktop apps via Troupe Comu in a session.
 */

import { useState } from 'react'
import { toast } from 'sonner'
import { Monitor, Square } from '@/components/icons'
import { Button } from '@/components/ui/button'
import { stopComuAgent } from '@/lib/comu-consent-api'
import { logFrontendError } from '@/lib/log-api'
import { useSessionComuStopped, useSessionDrivesComu } from '@/stores/comu-control-store'

export const COMU_STOP_LABEL = 'Stop Computer Use'

export function ComuAgentIndicator({ sessionId }: { sessionId: string }): React.JSX.Element | null {
  const drives = useSessionDrivesComu(sessionId)
  const stopped = useSessionComuStopped(sessionId)
  const [pending, setPending] = useState(false)

  if (!drives) return null

  const onStop = (): void => {
    if (pending) return
    setPending(true)
    void stopComuAgent(sessionId)
      .then((ok) => {
        if (ok) return
        toast.error('Could not stop Computer Use. Try again.')
        void logFrontendError({
          level: 'warn',
          message: `comu stop failed: sessionId=${sessionId}`,
          source: 'ComuAgentStop'
        })
      })
      .finally(() => setPending(false))
  }

  return (
    <div
      role="status"
      data-testid="comu-agent-indicator"
      className="flex items-center gap-2 rounded-xl border border-border bg-card px-3 py-1.5 text-xs"
    >
      <Monitor size={13} className="shrink-0 text-primary" aria-hidden="true" />
      <span className="min-w-0 flex-1 truncate text-muted-foreground">
        {stopped
          ? 'Computer Use stopped until your next message.'
          : 'The agent is using Computer Use on this machine.'}
      </span>
      {stopped ? null : (
        <Button
          type="button"
          size="xs"
          variant="destructive"
          disabled={pending}
          onClick={onStop}
          aria-label={COMU_STOP_LABEL}
        >
          <Square size={12} aria-hidden="true" />
          Stop
        </Button>
      )}
    </div>
  )
}
