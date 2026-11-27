import { trimSnapshotList } from '../../../../src/renderer/lib/persistence-trim'
import { useSnapshotStore } from '../../../../src/renderer/stores/snapshot-store'
import type {
  PersistedSnapshot,
  PersistedSnapshotList,
  PersistedTerminal
} from '../../../../src/shared/types/persistence.types'
import { readFailures, reads, type Scenario, type StoreRecorder, type VectorEvent } from '../record'
import { snapshotMock } from './snapshot-mock'

/** The clock every create runs at, so ids and `createdAt` are the same on every run. */
const CLOCK = Date.parse('2026-11-20T10:00:00.000Z')
/** `Math.random()` during a create: its id ends in `4fzyo83rr`. */
const RANDOM = 0.123456789

const KEYS = ['snapshots/p1', 'snapshots/p2']

/** Runs `run` with the clock and `Math.random` fixed, as the store reads both for a new id. */
async function atFixedTime<T>(run: () => Promise<T>): Promise<T> {
  const RealDate = Date
  const realRandom = Math.random
  class FixedDate extends RealDate {
    constructor(...args: unknown[]) {
      if (args.length === 0) super(CLOCK)
      else super(...(args as [string]))
    }
    static now(): number {
      return CLOCK
    }
  }
  globalThis.Date = FixedDate as DateConstructor
  Math.random = () => RANDOM
  try {
    return await run()
  } finally {
    globalThis.Date = RealDate
    Math.random = realRandom
  }
}

/** An action's outcome: what it returned, or the message it rejected with. */
async function settle(run: () => Promise<unknown>): Promise<unknown> {
  try {
    const value = await run()
    return value === undefined ? { ok: true } : { ok: value }
  } catch (error) {
    return { error: error instanceof Error ? error.message : String(error) }
  }
}

/**
 * The snapshot store (`snapshot-store.ts`) against a mocked host store: load, create, rename
 * and delete, each run to completion, with the reads it answered from `context.reads` and
 * `context.readFailures` and the writes it made. `tests/snapshots` replays them against
 * `model::snapshots`, answering each read and write the model asks for the same way.
 */
export const snapshotsRecorder: StoreRecorder = {
  store: 'snapshots',
  sources: ['src/renderer/stores/snapshot-store.ts', 'src/renderer/lib/persistence-trim.ts'],
  reset(context) {
    useSnapshotStore.setState({ snapshots: [], isLoading: false })
    snapshotMock.active = true
    snapshotMock.writeFailures.clear()
    for (const key of KEYS) {
      reads.delete(key)
      readFailures.delete(key)
    }
    const given = context as {
      reads?: Record<string, unknown>
      readFailures?: Record<string, string>
      writeFailures?: Record<string, string>
    }
    for (const [key, data] of Object.entries(given.reads ?? {})) reads.set(key, data)
    for (const [key, code] of Object.entries(given.readFailures ?? {})) readFailures.set(key, code)
    for (const [key, error] of Object.entries(given.writeFailures ?? {}))
      snapshotMock.writeFailures.set(key, error)
  },
  seed() {},
  snapshot() {
    const { snapshots, isLoading } = useSnapshotStore.getState()
    return { snapshots, isLoading }
  },
  dispatch({ action, args }) {
    const store = useSnapshotStore.getState()
    switch (action) {
      case 'loadSnapshots':
        return store.loadSnapshots(args[0] as string)
      case 'createSnapshot':
        return atFixedTime(() =>
          settle(() =>
            store.createSnapshot(
              args[0] as string,
              args[1] as string | undefined,
              args[2] as string,
              args[3] as PersistedTerminal[],
              args[4] as string | null
            )
          )
        )
      case 'renameSnapshot':
        return settle(() => store.renameSnapshot(args[0] as string, args[1] as string))
      case 'deleteSnapshot':
        return settle(() => store.deleteSnapshot(args[0] as string))
      case 'getSnapshot':
        return store.getSnapshot(args[0] as string)
      case 'hostForgets':
        reads.delete(args[0] as string)
        return undefined
      case 'hostFails':
        readFailures.set(args[0] as string, args[1] as string)
        return undefined
      case 'trim':
        return trimSnapshotList(args[0] as PersistedSnapshotList, args[1] as number)
      default:
        throw new Error(`snapshots has no action ${action}`)
    }
  }
}

const ev = (action: string, ...args: unknown[]): VectorEvent => ({ action, args })
const read = (action: string, ...args: unknown[]): VectorEvent & { query: true } => ({
  action,
  args,
  query: true
})

const terminal = (
  id: string,
  name: string,
  cwd: string,
  scrollback: string[]
): PersistedTerminal => ({
  id,
  name,
  shell: 'bash',
  cwd,
  scrollback
})

const saved = (
  id: string,
  name: string,
  extra: Partial<PersistedSnapshot> = {}
): PersistedSnapshot => ({
  id,
  projectId: 'p1',
  name,
  createdAt: '2026-11-19T08:30:00.000Z',
  terminals: [terminal('t1', 'Terminal 1', '/w/alpha', ['$ ls', 'README.md'])],
  activeTerminalId: 't1',
  ...extra
})

const LIST: PersistedSnapshotList = {
  snapshots: [
    saved('s3', 'Release candidate', { description: 'Green CI', tag: 'stable' }),
    saved('s2', 'Before the refactor', {
      terminals: [
        terminal('t1', 'Terminal 1', '/w/alpha', []),
        terminal('t2', 'Server', '/w/alpha/server', ['listening on 8080'])
      ]
    }),
    saved('s1', 'First', { description: 'Empty workspace', terminals: [], activeTerminalId: null })
  ],
  updatedAt: '2026-11-19T09:00:00.000Z'
}

const TERMINALS = [
  terminal('a', 'Terminal 1', '/w/alpha', ['$ git status', 'clean']),
  terminal('b', 'Logs', '/w/alpha/logs', [])
]

const big = (id: string, lines: number): PersistedSnapshot =>
  saved(id, `Snapshot ${id}`, {
    terminals: [
      terminal(
        't1',
        'Terminal 1',
        '/w',
        Array.from({ length: lines }, (_, i) => `line ${i}`)
      ),
      { ...terminal('t2', 'Agent', '/w', []), transcript: 'first\nsecond\nthird\nfourth\n' }
    ]
  })

export const snapshotsScenarios: Scenario[] = [
  {
    name: 'loads_a_projects_list_and_an_empty_one',
    context: { reads: { 'snapshots/p1': LIST } },
    steps: [ev('loadSnapshots', 'p1'), ev('loadSnapshots', 'p2')]
  },
  {
    name: 'creates_on_top_of_the_saved_list',
    context: { reads: { 'snapshots/p1': LIST } },
    steps: [
      ev('loadSnapshots', 'p1'),
      ev('createSnapshot', 'Before deploy', '', 'p1', TERMINALS, 'b'),
      ev('createSnapshot', 'With notes', 'Two terminals', 'p1', [], null)
    ]
  },
  {
    name: 'creates_the_first_snapshot_of_a_project',
    steps: [ev('createSnapshot', 'First', undefined, 'p2', TERMINALS, null)]
  },
  {
    name: 'create_rolls_back_when_the_list_cannot_be_read',
    context: { reads: { 'snapshots/p1': LIST }, readFailures: { 'snapshots/p1': 'IO_ERROR' } },
    steps: [ev('createSnapshot', 'Lost', 'never saved', 'p1', TERMINALS, 'a')]
  },
  {
    name: 'create_rolls_back_when_the_write_fails',
    context: { reads: { 'snapshots/p1': LIST }, writeFailures: { 'snapshots/p1': 'disk full' } },
    steps: [
      ev('loadSnapshots', 'p1'),
      ev('createSnapshot', 'Lost', undefined, 'p1', TERMINALS, 'a')
    ]
  },
  {
    name: 'renames_and_deletes_only_the_picked_snapshot',
    context: { reads: { 'snapshots/p1': LIST } },
    steps: [
      ev('loadSnapshots', 'p1'),
      ev('renameSnapshot', 's2', 'After the refactor'),
      ev('deleteSnapshot', 's3'),
      ev('renameSnapshot', 'nope', 'x'),
      ev('deleteSnapshot', 'nope')
    ]
  },
  {
    name: 'rename_rolls_back_when_the_write_fails_or_the_list_is_gone',
    context: { reads: { 'snapshots/p1': LIST }, writeFailures: { 'snapshots/p1': 'disk full' } },
    steps: [
      ev('loadSnapshots', 'p1'),
      ev('renameSnapshot', 's1', 'Renamed'),
      ev('deleteSnapshot', 's1'),
      ev('hostForgets', 'snapshots/p1'),
      ev('renameSnapshot', 's3', 'Gone')
    ]
  },
  {
    name: 'delete_keeps_the_local_removal_when_the_list_is_missing_or_unreadable',
    context: { reads: { 'snapshots/p1': LIST } },
    steps: [
      ev('loadSnapshots', 'p1'),
      ev('hostForgets', 'snapshots/p1'),
      ev('deleteSnapshot', 's2'),
      ev('hostFails', 'snapshots/p1', 'IO_ERROR'),
      ev('deleteSnapshot', 's1')
    ]
  },
  {
    name: 'gets_a_snapshot_with_its_terminals',
    context: { reads: { 'snapshots/p1': LIST } },
    steps: [ev('loadSnapshots', 'p1'), read('getSnapshot', 's2'), read('getSnapshot', 'nope')]
  },
  {
    name: 'trims_output_first_then_the_oldest_snapshots',
    steps: [
      read('trim', { snapshots: [big('n', 4), big('o', 4)], updatedAt: LIST.updatedAt }, 100000),
      read('trim', { snapshots: [big('n', 40), big('o', 40)], updatedAt: LIST.updatedAt }, 1200),
      read('trim', { snapshots: [big('n', 40), big('o', 4)], updatedAt: LIST.updatedAt }, 700),
      read(
        'trim',
        { snapshots: [big('n', 4), big('o', 4), big('p', 4)], updatedAt: LIST.updatedAt },
        500
      )
    ]
  }
]
