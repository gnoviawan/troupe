import type { Surface } from '../types'
import { SNAPSHOT_NAME, seedSnapshot } from '../world'
import { click, css, see, step, txt } from './dsl'

const start = { hash: '#/snapshots', ready: SNAPSHOT_NAME }
const LIST = step('a-list', [], see('Workspace Snapshots', SNAPSHOT_NAME, 'Restore'))

export const SNAPSHOTS: Surface[] = [
  {
    id: 'snapshots.page',
    area: 'snapshots',
    title: 'Workspace snapshots page',
    source: { screen: 'route', name: '/snapshots' },
    start,
    prepare: seedSnapshot,
    steps: [LIST]
  },
  {
    id: 'snapshots.create',
    area: 'snapshots',
    title: 'Create snapshot dialog',
    source: { overlay: ['components/CreateSnapshotModal.tsx'] },
    start,
    prepare: seedSnapshot,
    steps: [LIST, step('b-modal', [click(txt('Create New Snapshot'))], see('Create Snapshot'))]
  },
  {
    id: 'snapshots.delete',
    area: 'snapshots',
    title: 'Delete snapshot confirm',
    source: { overlay: ['components/DeleteSnapshotModal.tsx'] },
    start,
    prepare: seedSnapshot,
    steps: [LIST, step('b-modal', [click(css('button[title="Delete"]'))], see('Delete Snapshot'))]
  },
  {
    id: 'snapshots.restore',
    area: 'snapshots',
    title: 'Restore snapshot confirm',
    source: { overlay: ['components/RestoreSnapshotModal.tsx'] },
    start,
    prepare: seedSnapshot,
    steps: [LIST, step('b-modal', [click(txt('Restore'))], see('Restore Snapshot'))]
  }
]
