import { useEffect } from 'react'
import { loadAgentOverlay } from '@/lib/agents/agent-overlay'

/** The host refreshes its copy hourly; ask again at the same cadence. */
const RELOAD_INTERVAL_MS = 60 * 60 * 1000

/** Load Troupe's registry overlay on mount and every hour. Both roots. */
export function useAgentOverlayLoader(): void {
  useEffect(() => {
    void loadAgentOverlay()
    const interval = window.setInterval(() => void loadAgentOverlay(), RELOAD_INTERVAL_MS)
    return () => window.clearInterval(interval)
  }, [])
}
