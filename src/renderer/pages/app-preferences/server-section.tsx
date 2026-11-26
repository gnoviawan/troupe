/**
 * Web-only "Server" preferences page (gh-1105): reconfigure the
 * `termul-server` the browser is connected to — address, port, updates —
 * using the same {@link ServerSetupWizard} as first-run setup, in
 * `mode="reconfigure"` pre-filled from `GET /server/config`.
 *
 * Applying persists `server-config.json` and restarts the server, so the
 * page warns up front and the wizard hands off to the restarting screen
 * (the browser is polled+redirected to the new origin, with `#token=` when
 * the server issued a web auth token).
 *
 * The page is hidden on the desktop app (no `termul-server` to configure —
 * the mirror of `UpdatesSection`'s desktop-only gate, issue #843). On a
 * desktop shared-live host the `/server/config` routes aren't registered
 * (404) — the fetch failure surfaces as the error state with Try again.
 *
 * Standalone `troupe-server` also shows a "Connect another device" QR card.
 * The card is absent on desktop because this whole section is.
 */

import type { ServerSetupAnswers } from '@shared/types/server-setup.types'
import { useEffect, useState } from 'react'
import { ServerQrPanel } from '@/components/ServerQrPanel'
import { rowId } from '@/components/settings/row-id'
import { SETTINGS_CARD_CLASS, SettingsGroup } from '@/components/settings/SettingsGroup'
import { SettingsNotice } from '@/components/settings/SettingsNotice'
import { ServerSetupWizard } from '@/components/setup/ServerSetupWizard'
import { Button } from '@/components/ui/button'
import { Spinner } from '@/components/ui/spinner'
import { useServerAdmitsRemoteWrites } from '@/hooks/use-server-admits-remote-writes'
import { logFrontendError } from '@/lib/log-api'
import { shareLinkAllowsWrites } from '@/lib/server-qr-url'
import { isTauriContext } from '@/lib/tauri-runtime'
import { cn } from '@/lib/utils'
import { webSetupApi } from '@/lib/web-setup-api'

/** The web-only Server section of About. Renders nothing on the desktop. */
export function ServerSection(): React.JSX.Element | null {
  // Web-only (AGENTS.md): the whole page is hidden on the desktop runtime.
  if (isTauriContext()) return null
  return <ServerSectionInner />
}

function ServerSectionInner(): React.JSX.Element {
  const [config, setConfig] = useState<ServerSetupAnswers | null>(null)
  const [lanHost, setLanHost] = useState<string | undefined>(undefined)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [attempt, setAttempt] = useState(0)
  const admitsWrites = useServerAdmitsRemoteWrites()
  const allowWrites = shareLinkAllowsWrites({
    origin: typeof window !== 'undefined' ? window.location.origin : '',
    hostname: typeof window !== 'undefined' ? window.location.hostname : '',
    admitsWrites,
    allowRemoteWrites: config?.allowRemoteWrites ?? false
  })

  // biome-ignore lint/correctness/useExhaustiveDependencies: attempt intentionally re-runs the load (Try again button)
  useEffect(() => {
    let cancelled = false
    setLoading(true)
    setError(null)
    void webSetupApi
      .getServerConfig()
      .then((res) => {
        if (cancelled) return
        if (res.success) {
          setConfig(res.data.config)
          setLanHost(res.data.lanHost)
          setLoading(false)
        } else {
          void logFrontendError({
            level: 'warn',
            source: 'ServerSection',
            message: `GET /server/config failed (code ${res.code})`
          })
          setError('Could not load the server settings.')
          setLoading(false)
        }
      })
      .catch((err) => {
        if (cancelled) return
        void logFrontendError({
          level: 'warn',
          source: 'ServerSection',
          message: `GET /server/config threw: ${err instanceof Error ? err.message : String(err)}`
        })
        setError('Could not load the server settings.')
        setLoading(false)
      })
    return () => {
      cancelled = true
    }
  }, [attempt])

  return (
    <>
      {config && (
        <SettingsGroup title="Connect another device">
          <p className="text-xs text-muted-foreground">
            Scan the QR code to open this Troupe server on a phone or tablet.
          </p>
          <div id={rowId('server-qr')} className={cn('scroll-mt-4 px-4 py-3', SETTINGS_CARD_CLASS)}>
            <ServerQrPanel config={config} lanHost={lanHost} allowWrites={allowWrites} />
          </div>
        </SettingsGroup>
      )}
      <SettingsGroup title="Server">
        <SettingsNotice tone="warning">
          Saving these settings restarts the server. You're taken to the new address automatically —
          anyone else connected has to reload.
        </SettingsNotice>
        <div id={rowId('server-config')} className="scroll-mt-4">
          {loading ? (
            <div className={cn('flex items-center justify-center py-6', SETTINGS_CARD_CLASS)}>
              <Spinner label="Loading server settings" />
            </div>
          ) : error ? (
            <div className={cn('flex items-center gap-3 px-4 py-3', SETTINGS_CARD_CLASS)}>
              <p role="alert" className="min-w-0 flex-1 text-xs text-destructive">
                {error}
              </p>
              <Button
                type="button"
                size="xs"
                variant="secondary"
                onClick={() => setAttempt((n) => n + 1)}
              >
                Try again
              </Button>
            </div>
          ) : config ? (
            <ServerSetupWizard mode="reconfigure" initial={config} />
          ) : null}
        </div>
      </SettingsGroup>
    </>
  )
}
