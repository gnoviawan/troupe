/**
 * Root-level corner fallback host for pending Troupe Comu consent cards
 * when no visible chat panel hosts the session's card (or an overlay modal
 * covers the workspace). Mounted on both `TauriApp.tsx` and `App.tsx`.
 */

import { ComuConsentCard } from '@/components/comu/ComuConsentCard'
import {
  useInterfaceOverlayOpen,
  useNativeWebviewOcclusion
} from '@/hooks/use-native-webview-occlusion'
import { useComuControlStore } from '@/stores/comu-control-store'

export function ComuConsentCardHost(): React.JSX.Element | null {
  const pending = useComuControlStore((s) => s.pendingConsents)
  const hosted = useComuControlStore((s) => s.hostedSessionIds)
  const overlayOpen = useInterfaceOverlayOpen()

  const unhosted = Object.values(pending).filter((c) => overlayOpen || !hosted.has(c.sessionId))

  useNativeWebviewOcclusion(unhosted.length > 0, 'docked')

  if (unhosted.length === 0) return null

  return (
    <div
      className="pointer-events-auto fixed right-4 bottom-4 z-[60] flex w-full max-w-sm flex-col gap-2"
      data-testid="comu-consent-card-host"
    >
      {unhosted.map((consent) => (
        <ComuConsentCard key={consent.requestId} consent={consent} />
      ))}
    </div>
  )
}
