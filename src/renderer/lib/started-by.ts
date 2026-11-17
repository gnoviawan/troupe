import type { SessionIndexEntry } from '@/lib/acp-history-persistence'

/**
 * Resolves an entry's `startedBy` to the "Started by <parent title>" label.
 * `undefined` when the entry has no parent or the index does not hold it.
 */
export function startedByLabeler(
  index: readonly SessionIndexEntry[]
): (entry: Pick<SessionIndexEntry, 'startedBy'>) => string | undefined {
  const titles = new Map(index.map((entry) => [entry.id, entry.title]))
  return (entry) => {
    const title = entry.startedBy ? titles.get(entry.startedBy) : undefined
    return title === undefined ? undefined : `Started by ${title}`
  }
}
