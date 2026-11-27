/**
 * The mocked persistence of the snapshot store's vectors. It lives apart from
 * the recorder so the `vi.mock` factory imports no store.
 */

import { emit } from '../record'

/** While `active`, writes to `snapshots/*` are recorded and fail for the keys in `writeFailures`. */
export const snapshotMock = {
  active: false,
  writeFailures: new Map<string, string>()
}

/** The mocked `persistenceApi.write` of a `snapshots/<projectId>` key, without the wall-clock `updatedAt`. */
export function writeSnapshots(
  key: string,
  { updatedAt: _stamp, ...record }: Record<string, unknown>
) {
  emit({ kind: 'writeSnapshots', key, record })
  const failed = snapshotMock.writeFailures.get(key)
  return failed
    ? { success: false, code: 'IO_ERROR', error: failed }
    : { success: true, data: undefined }
}
