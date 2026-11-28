import { useSSHStore } from '../../../../src/renderer/stores/ssh-store'
import type { Scenario, StoreRecorder, VectorEvent } from '../record'

const ev = (action: string, ...args: unknown[]): VectorEvent => ({ action, args })

const host = (id: string, extra: Record<string, unknown> = {}) => ({
  id,
  name: `host ${id}`,
  host: 'prod.example',
  port: 22,
  username: 'deploy',
  authMethod: 'key',
  portForwards: [],
  ...extra
})

const session = (id: string, profileId: string, extra: Record<string, unknown> = {}) => ({
  id,
  attemptId: id,
  profileId,
  status: 'connecting',
  stage: 'connecting',
  hostKeyPrompt: null,
  credentialPrompt: null,
  error: null,
  ended: null,
  ptyId: null,
  claim: null,
  attached: false,
  connectedAt: null,
  ...extra
})

const transfer = (connectionId: string, remotePath: string, status: string, bytes: number) => ({
  connectionId,
  remotePath,
  localPath: `/tmp${remotePath}`,
  bytesTransferred: bytes,
  totalBytes: 10,
  direction: 'upload',
  status,
  error: status === 'failed' ? 'Write error' : null
})

const initial = (extra: Record<string, unknown> = {}) => ({
  profiles: [host('p1'), host('p2', { port: 2222, favorite: true })],
  isLoaded: true,
  workspaceOpen: false,
  activeTab: 'hosts',
  selectedProfileId: null,
  showAddresses: false,
  sessions: [],
  pendingClose: null,
  transfers: [],
  editingFile: null,
  editingContent: '',
  ...extra
})

export const sshScenarios: Scenario[] = [
  {
    name: 'tabs_focus_the_neighbour_when_one_closes',
    initial: initial(),
    steps: [
      ev('addSession', session('t1', 'p1')),
      ev('addSession', session('t2', 'p2')),
      ev('addSession', session('t3', 'p1')),
      ev('setActiveTab', 't2'),
      ev('removeSession', 't2'),
      ev('removeSession', 't3'),
      ev('setActiveTab', 'missing'),
      ev('setActiveTab', 't1'),
      ev('removeSession', 't1'),
      ev('removeSession', 't1')
    ]
  },
  {
    name: 'select_a_host_opens_its_latest_session',
    initial: initial(),
    steps: [
      ev('selectProfile', 'p1'),
      ev('addSession', session('t1', 'p1')),
      ev('addSession', session('t2', 'p1')),
      ev('selectProfile', 'p2'),
      ev('selectProfile', 'p1'),
      ev('selectProfile', null),
      ev('openWorkspace'),
      ev('toggleShowAddresses'),
      ev('setPendingClose', 't2'),
      ev('setPendingClose', null)
    ]
  },
  {
    name: 'a_session_patch_replaces_its_fields',
    initial: initial({ sessions: [session('t1', 'p1')], activeTab: 't1', workspaceOpen: true }),
    steps: [
      ev('patchSession', 't1', {
        status: 'awaitingInput',
        hostKeyPrompt: {
          sessionId: 't1',
          status: 'changed',
          host: 'prod.example',
          port: 22,
          keyType: 'ssh-ed25519',
          fingerprint: 'SHA256:new'
        }
      }),
      ev('patchSession', 't1', { status: 'connecting', hostKeyPrompt: null }),
      ev('patchSession', 't1', {
        status: 'connected',
        stage: null,
        ptyId: 'pty-1',
        claim: 'claim-1',
        connectedAt: '2026-11-28T10:00:00Z'
      }),
      ev('patchSession', 'other', { status: 'failed' })
    ]
  },
  {
    name: 'transfers_merge_and_clear',
    initial: initial(),
    steps: [
      ev('updateTransferProgress', transfer('a', '/x', 'in-progress', 1)),
      ev('updateTransferProgress', transfer('a', '/x', 'completed', 10)),
      ev('updateTransferProgress', transfer('b', '/y', 'failed', 4)),
      ev('updateTransferProgress', transfer('a', '/z', 'in-progress', 3)),
      ev('clearCompletedTransfers', 'a'),
      ev('clearCompletedTransfers'),
      ev('updateTransferProgress', transfer('a', '/z', 'completed', 10)),
      ev('clearCompletedTransfers', 'b')
    ]
  },
  {
    name: 'the_remote_editor_keeps_its_file',
    initial: initial(),
    steps: [
      ev('setEditingFile', {
        connectionId: 't1',
        path: '/home/deploy/notes.txt',
        name: 'notes.txt',
        content: 'a',
        originalContent: 'a'
      }),
      ev('setEditingContent', 'ab'),
      ev('setEditingFile', null)
    ]
  }
]

export const sshRecorder: StoreRecorder = {
  store: 'ssh',
  sources: ['src/renderer/stores/ssh-store.ts'],
  reset() {
    useSSHStore.setState(initial({ profiles: [], isLoaded: false }))
  },
  seed(state) {
    useSSHStore.setState(structuredClone(state))
  },
  snapshot() {
    const s = useSSHStore.getState()
    return {
      profiles: s.profiles,
      isLoaded: s.isLoaded,
      workspaceOpen: s.workspaceOpen,
      activeTab: s.activeTab,
      selectedProfileId: s.selectedProfileId,
      showAddresses: s.showAddresses,
      sessions: s.sessions,
      pendingClose: s.pendingClose,
      transfers: s.transfers,
      editingFile: s.editingFile,
      editingContent: s.editingContent
    }
  },
  dispatch({ action, args }) {
    const store = useSSHStore.getState() as unknown as Record<string, (...a: unknown[]) => unknown>
    const run = store[action]
    if (typeof run !== 'function') throw new Error(`ssh store has no action ${action}`)
    return run(...args)
  }
}
