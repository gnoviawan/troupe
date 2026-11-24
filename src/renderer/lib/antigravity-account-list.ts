const listeners = new Set<() => void>()

/** Refresh every mounted Antigravity account list after a store change. */
export function subscribeAntigravityAccountList(listener: () => void): () => void {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

export function notifyAntigravityAccountListChanged(): void {
  for (const listener of listeners) listener()
}
