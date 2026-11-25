/**
 * The host moves an Antigravity chat that reaches its account's limit, and
 * only the agent's own limit message marks a chat at its limit. A usage check
 * is for display only, so this probe reads no usage. It only keeps the resume
 * turn watch running while no limit card is on screen.
 */

import { useEffect } from 'react'
import { ensureAntigravityResumeTurnWatch } from './use-antigravity-limit'

/** Mount once per renderer root. */
export function useAntigravityLimitProbe(): void {
  useEffect(() => ensureAntigravityResumeTurnWatch(), [])
}
