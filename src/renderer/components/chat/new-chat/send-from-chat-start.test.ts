import { beforeEach, describe, expect, it, vi } from 'vitest'

const { launchChat, openNewChat, writesRef, toastError } = vi.hoisted(() => ({
  launchChat: vi.fn(),
  openNewChat: vi.fn(),
  writesRef: { current: true },
  toastError: vi.fn()
}))

vi.mock('@/lib/log-api', () => ({ logFrontendError: vi.fn(async () => {}) }))
vi.mock('@/lib/api', () => ({
  persistenceApi: { write: vi.fn(async () => ({ success: true })) }
}))
vi.mock('sonner', () => ({ toast: Object.assign(vi.fn(), { error: toastError }) }))
vi.mock('@/lib/tauri-runtime', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/tauri-runtime')>()),
  serverAdmitsRemoteWrites: () => writesRef.current
}))
vi.mock('@/components/agents/launcher/launch-chat', () => ({ launchChat }))
vi.mock('./open-new-chat', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./open-new-chat')>()),
  openNewChat
}))

import type { ContentBlock } from '@/lib/acp-api'
import { registerSessionTempFiles } from '@/lib/attachment-temp-cleanup'
import { useAcpStore } from '@/stores/acp-store'
import { type ChatStartPick, useChatStartPickStore } from '@/stores/chat-start-pick-store'
import { useProjectStore } from '@/stores/project-store'
import { agentChatTabId, useWorkspaceStore } from '@/stores/workspace-store'
import { chatStartTarget, sendFromChatStart } from './send-from-chat-start'

const WT = '/work/.termul/worktrees/feat'
const project = {
  id: 'p1',
  name: 'termul',
  path: '/work',
  isGitRepo: true,
  worktrees: [{ id: 'w1', name: 'feat', branch: 'feat/x', path: WT, createdAt: '2026-01-01' }]
}
const wire: ContentBlock[] = [{ type: 'text', text: 'wire text' }]
const display: ContentBlock[] = [{ type: 'text', text: 'hello there' }]

const pick = (over: Partial<ChatStartPick>): ChatStartPick => ({
  isolationMode: 'current',
  existingWorktreeId: null,
  baseBranch: null,
  ...over
})

let started: { resolve: (id: string) => void; reject: (err: unknown) => void }
const closeSession = vi.fn(async () => {})
const deleteHistorySession = vi.fn(async () => {})

function send(): boolean {
  return sendFromChatStart({
    sessionId: 's1',
    blocks: wire,
    displayBlocks: display,
    onBlocked
  })
}
const onBlocked = vi.fn()

beforeEach(() => {
  vi.clearAllMocks()
  writesRef.current = true
  useProjectStore.setState({ activeProjectId: 'p1', projects: [project] } as never)
  const tabId = agentChatTabId('s1')
  useWorkspaceStore.setState({
    root: {
      type: 'leaf',
      id: 'pane-1',
      tabs: [{ type: 'agent-chat', id: tabId, sessionId: 's1', mountKey: tabId }],
      activeTabId: tabId
    },
    activePaneId: 'pane-1'
  } as never)
  useAcpStore.setState({
    sessions: {
      s1: {
        id: 's1',
        agentId: '',
        projectId: 'p1',
        cwd: '/work',
        status: 'active',
        launchConfigId: 'cfg-1',
        models: { currentModelId: 'm2', availableModels: [{ modelId: 'm2', name: 'M2' }] },
        modes: null,
        configOptions: []
      }
    },
    messages: { s1: [] },
    launchingSessionIds: {},
    sessionIndex: [],
    configToLiveAgent: {},
    closeSession,
    deleteHistorySession
  } as never)
  useChatStartPickStore.setState({ picks: {} })
  launchChat.mockImplementation(() => ({
    placeholderId: 'launch-1',
    started: new Promise<string>((resolve, reject) => {
      started = { resolve, reject }
    })
  }))
})

describe('chatStartTarget', () => {
  it('stays when the pick is the folder the chat runs in', () => {
    expect(chatStartTarget({ cwd: '/work' }, pick({}), project)).toEqual({ kind: 'stay' })
    expect(
      chatStartTarget(
        { cwd: WT },
        pick({ isolationMode: 'existing', existingWorktreeId: 'w1' }),
        project
      )
    ).toEqual({ kind: 'stay' })
  })

  it('moves a worktree chat back to the current checkout', () => {
    expect(chatStartTarget({ cwd: WT }, pick({}), project)).toEqual({
      kind: 'move',
      checkout: { launchCwd: '/work' }
    })
  })

  it('makes a new worktree only where the server admits writes', () => {
    const worktree = pick({ isolationMode: 'worktree', baseBranch: 'dev' })
    expect(chatStartTarget({ cwd: '/work' }, worktree, project)).toEqual({
      kind: 'new-worktree',
      projectRoot: '/work',
      baseBranch: 'dev'
    })
    writesRef.current = false
    expect(chatStartTarget({ cwd: '/work' }, worktree, project)).toEqual({ kind: 'stay' })
  })

  it('blocks a pick that cannot apply', () => {
    expect(
      chatStartTarget({ cwd: '/work' }, pick({ isolationMode: 'worktree' }), project).kind
    ).toBe('blocked')
    expect(
      chatStartTarget(
        { cwd: '/work' },
        pick({ isolationMode: 'existing', existingWorktreeId: 'gone' }),
        project
      ).kind
    ).toBe('blocked')
  })
})

describe('sendFromChatStart', () => {
  it('sends as usual when the chat has no pick, or the pick changes nothing', () => {
    expect(send()).toBe(false)
    useChatStartPickStore.getState().setPick('s1', pick({}))
    expect(send()).toBe(false)
    expect(launchChat).not.toHaveBeenCalled()
  })

  it('never applies a pick once the chat has messages', () => {
    useChatStartPickStore
      .getState()
      .setPick('s1', pick({ isolationMode: 'existing', existingWorktreeId: 'w1' }))
    useAcpStore.setState({ messages: { s1: [{ id: 'm', role: 'user', blocks: [] }] } } as never)
    expect(send()).toBe(false)
  })

  it('starts the empty chat again in the picked worktree with the message and options', async () => {
    useChatStartPickStore
      .getState()
      .setPick('s1', pick({ isolationMode: 'existing', existingWorktreeId: 'w1' }))
    registerSessionTempFiles('s1', ['/tmp/shot.png'])

    expect(send()).toBe(true)

    const request = launchChat.mock.calls[0]?.[0]
    expect(request).toMatchObject({
      configId: 'cfg-1',
      projectId: 'p1',
      paneId: 'pane-1',
      checkout: { launchCwd: WT, worktreePath: WT, worktreeBranch: 'feat/x' },
      placeholder: { initialUserBlocks: display }
    })
    expect(request.newWorktree).toBeUndefined()
    await expect(request.beforeStart()).resolves.toEqual({
      blocks: wire,
      pending: expect.objectContaining({ modelId: 'm2' })
    })

    // The new chat takes the old chat's tab in place.
    const remap = vi.spyOn(useWorkspaceStore.getState(), 'remapAgentChatSession')
    request.onOpened('launch-1')
    expect(remap).toHaveBeenCalledWith('s1', 'launch-1', 'pane-1')

    started.resolve('real-1')
    await vi.waitFor(() => expect(deleteHistorySession).toHaveBeenCalledWith('s1'))
    expect(closeSession).toHaveBeenCalledWith('s1')
    expect(useChatStartPickStore.getState().picks.s1).toBeUndefined()
  })

  it('makes the new worktree on the first send and reopens an empty chat when it fails', async () => {
    useChatStartPickStore
      .getState()
      .setPick('s1', pick({ isolationMode: 'worktree', baseBranch: 'dev' }))
    openNewChat.mockResolvedValue({ kind: 'opened', sessionId: 'launch-2' })

    expect(send()).toBe(true)

    const request = launchChat.mock.calls[0]?.[0]
    expect(request.newWorktree).toEqual({ projectRoot: '/work', baseBranch: 'dev' })
    expect(request.checkout).toEqual({ launchCwd: '/work' })
    request.onWorktreeFailed(new Error('Worktree creation failed: boom'))
    started.reject(new Error('Worktree creation failed: boom'))
    expect(openNewChat).toHaveBeenCalledWith({
      paneId: 'pane-1',
      projectId: 'p1',
      reason: 'worktree-failed'
    })
    await vi.waitFor(() =>
      expect(toastError).toHaveBeenCalledWith('Worktree creation failed: boom')
    )
  })

  it('keeps the message when the pick cannot apply', () => {
    useChatStartPickStore.getState().setPick('s1', pick({ isolationMode: 'worktree' }))
    expect(send()).toBe(true)
    expect(launchChat).not.toHaveBeenCalled()
    expect(onBlocked).toHaveBeenCalledWith('hello there')
    expect(toastError).toHaveBeenCalledWith('Pick a base branch for the new worktree.')
  })
})
