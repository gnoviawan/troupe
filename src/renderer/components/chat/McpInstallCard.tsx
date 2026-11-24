import type { McpInstallField } from '@shared/types/mcp-install.types'
import { useEffect, useId, useRef, useState } from 'react'
import { toast } from 'sonner'
import { McpConsentReview } from '@/components/settings/McpConsentDialog'
import {
  initialInstallValues,
  McpInstallForm,
  splitInstallValues
} from '@/components/settings/McpInstallForm'
import { Button } from '@/components/ui/button'
import { useReloadChat } from '@/hooks/use-reload-chat'
import { logFrontendError } from '@/lib/log-api'
import type { PendingMcpInstall } from '@/lib/mcp-install-request'
import { cn } from '@/lib/utils'
import { useAcpStore } from '@/stores/acp-store'
import { CHAT_GUTTER_X, PROMPT_PANEL_CLASS, QUESTION_PANEL_MAX_H } from './chat-layout'

interface McpInstallCardProps {
  install: PendingMcpInstall
  projectId: string | null
}

/**
 * Approval card for `mcp_install`. Secret field values stay in this component
 * until Install. They are not written to the store, the tool result, or a log.
 */
export function McpInstallCard({ install, projectId }: McpInstallCardProps): React.JSX.Element {
  const answer = useAcpStore((s) => s.answerMcpInstall)
  const dismiss = useAcpStore((s) => s.dismissMcpInstall)
  const connect = useAcpStore((s) => s.connectMcpOAuth)
  const checkOAuth = useAcpStore((s) => s.checkMcpOAuthStatus)
  const serverId = install.result?.serverId ?? null
  const connecting = useAcpStore((s) =>
    serverId ? s.mcpOAuthConnecting[serverId] === true : false
  )
  const connected = useAcpStore((s) => (serverId ? s.mcpOAuthConnected[serverId] === true : false))
  const chatReload = useReloadChat(install.sessionId)
  const [values, setValues] = useState(() => initialInstallValues(install.form.fields))
  const [fieldErrors, setFieldErrors] = useState<Record<string, string>>({})
  const valuesRef = useRef(values)
  valuesRef.current = values
  const [baseline, setBaseline] = useState<string | null>(null)
  const titleId = useId()

  useEffect(() => {
    if (!install.launchDigest) {
      setBaseline(null)
      return
    }
    setBaseline(JSON.stringify(splitInstallValues(install.form.fields, valuesRef.current).inputs))
  }, [install.launchDigest, install.form])

  // A token stored before this card (an earlier sign-in) shows as signed in.
  const needsOAuth = install.result?.oauth === true
  useEffect(() => {
    if (!needsOAuth || !serverId || projectId === null) return
    void checkOAuth(projectId, serverId)
  }, [needsOAuth, serverId, projectId, checkOAuth])

  // The reload started (directly or after its confirm): the card has done
  // its job, and the reloaded chat carries the new server.
  useEffect(() => {
    if (chatReload.reloading) dismiss(install.requestId)
  }, [chatReload.reloading, dismiss, install.requestId])

  const { inputs } = splitInstallValues(install.form.fields, values)
  const reviewed = baseline !== null && JSON.stringify(inputs) === baseline
  const consent = install.consent

  const missing = (fields: McpInstallField[]): Record<string, string> =>
    Object.fromEntries(
      fields
        .filter((field) => field.isRequired && (values[field.id] ?? '').trim() === '')
        .map((field) => [field.id, 'Fill this in to continue.'])
    )

  const send = (decision: 'decline' | 'preview' | 'approve'): void => {
    const required =
      decision === 'decline'
        ? []
        : install.form.fields.filter((field) => (decision === 'preview' ? !field.isSecret : true))
    const errors = missing(required)
    if (Object.keys(errors).length > 0) {
      setFieldErrors(errors)
      return
    }
    const split = splitInstallValues(install.form.fields, values)
    void answer(
      install.requestId,
      decision,
      split.inputs,
      decision === 'approve' ? split.secretValues : undefined,
      decision === 'approve' ? install.planId : null,
      decision === 'approve' ? install.launchDigest : null
    ).catch((error: unknown) => {
      void logFrontendError({
        level: 'warn',
        source: 'chat.McpInstallCard',
        message: `mcp install ${decision} failed (${error instanceof Error ? error.name : 'error'})`
      })
      toast.error('Could not send your decision. Try again.')
    })
  }

  const signIn = (): void => {
    if (!serverId || projectId === null) return
    const run = async (): Promise<boolean> => {
      // The registry reload after an agent install can still be in flight;
      // without the server entry the store would return false silently.
      const known = (): boolean =>
        useAcpStore.getState().mcpServers.some((server) => server.id === serverId)
      if (!known()) await useAcpStore.getState().loadMcpServers()
      if (!known()) {
        void logFrontendError({
          level: 'warn',
          source: 'chat.McpInstallCard',
          message: `mcp sign-in skipped: server ${serverId} is not in the loaded registry`
        })
        toast.error('Could not find this server yet. Try again from Settings.')
        return false
      }
      return connect(projectId, serverId)
    }
    void run()
      .then((ok) => {
        if (ok) toast.success(`Signed in to ${install.name}`)
      })
      .catch(() => toast.error('Could not start sign-in. Try again from Settings.'))
  }

  const result = install.result
  const showSignIn = Boolean(result?.oauth && serverId) && !connected
  // Reload is offered once nothing else blocks the new tools: no sign-in is
  // left, and the server was saved (or was already there).
  const saved = result?.outcome === 'installed' || result?.outcome === 'already_exists'
  const canReload = saved && !showSignIn
  const signInStatus = !result?.oauth
    ? null
    : connected
      ? `Signed in. Reload this chat so the agent can use ${install.name}.`
      : connecting
        ? 'Finish signing in in the browser window that opened.'
        : null

  return (
    <div
      role="dialog"
      aria-labelledby={titleId}
      className={cn(CHAT_GUTTER_X, 'pb-6 pt-3')}
      data-testid="mcp-install-card"
    >
      <section className={cn('mx-auto w-full max-w-3xl', PROMPT_PANEL_CLASS)}>
        <div
          className={cn('grid gap-3 overflow-y-auto p-4', QUESTION_PANEL_MAX_H, 'scroller-thin')}
        >
          <div>
            <p className="text-xs text-muted-foreground">
              {install.title !== install.name ? install.title : 'Install MCP server'}
            </p>
            <h2 id={titleId} className="text-sm font-medium text-foreground">
              {install.name}
            </h2>
          </div>
          {consent && <McpConsentReview view={consent} launchDigest={install.launchDigest} />}
          {install.form.fields.length > 0 && !result && (
            <McpInstallForm
              fields={install.form.fields}
              values={values}
              disabled={install.saving}
              errors={fieldErrors}
              onChange={(id, value) => {
                setValues((current) => ({ ...current, [id]: value }))
                setFieldErrors((current) => {
                  const { [id]: _cleared, ...rest } = current
                  return rest
                })
              }}
            />
          )}
          {install.error && <p className="text-xs text-destructive">{install.error}</p>}
          {result && (
            <div className="grid gap-2">
              <p className="text-sm text-foreground">
                {result.outcome === 'installed'
                  ? `${install.name} is installed.`
                  : result.outcome === 'already_exists'
                    ? `${result.name ?? install.name} is already installed.`
                    : 'Nothing was saved.'}
              </p>
              <ul className="grid gap-1 text-xs text-muted-foreground">
                {result.nextSteps.map((step) => (
                  <li key={`${step.kind}-${step.runtime ?? ''}`}>{step.message}</li>
                ))}
              </ul>
              {signInStatus && (
                <p className="text-xs text-foreground" role="status">
                  {signInStatus}
                </p>
              )}
            </div>
          )}
        </div>
        <div className="flex flex-wrap items-center justify-end gap-2 border-t border-border/60 px-4 py-3">
          {result ? (
            <>
              <Button type="button" variant="outline" onClick={() => dismiss(install.requestId)}>
                Dismiss
              </Button>
              {showSignIn && (
                <Button type="button" disabled={connecting} onClick={signIn}>
                  {connecting ? 'Signing in…' : 'Sign in'}
                </Button>
              )}
              {canReload && chatReload.reload && (
                <Button
                  type="button"
                  disabled={chatReload.disabled}
                  title={chatReload.disabled ? (chatReload.reason ?? undefined) : undefined}
                  onClick={chatReload.reload}
                >
                  {chatReload.reloading ? 'Reloading…' : 'Reload chat'}
                </Button>
              )}
            </>
          ) : (
            <>
              <Button
                type="button"
                variant="outline"
                disabled={install.saving}
                onClick={() => send('decline')}
              >
                Decline
              </Button>
              {consent && reviewed ? (
                <Button type="button" disabled={install.saving} onClick={() => send('approve')}>
                  {install.saving ? 'Installing…' : 'Install'}
                </Button>
              ) : (
                <Button type="button" disabled={install.saving} onClick={() => send('preview')}>
                  Review command
                </Button>
              )}
            </>
          )}
        </div>
      </section>
    </div>
  )
}
