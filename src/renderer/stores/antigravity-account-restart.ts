import { create } from 'zustand'

/** Chats whose Antigravity account restart has not finished. */
const restarting = new Set<string>()

/** Reactive copy so the composer can disable while a restart runs. */
export const useAntigravityRestartingStore = create<{ ids: readonly string[] }>(() => ({ ids: [] }))

export function setAntigravityAccountRestarting(sessionId: string, active: boolean): void {
  if (active) restarting.add(sessionId)
  else restarting.delete(sessionId)
  useAntigravityRestartingStore.setState({ ids: [...restarting] })
}

/** True while this chat's Antigravity account restart has not finished. */
export function isAntigravityAccountRestarting(sessionId: string): boolean {
  return restarting.has(sessionId)
}
