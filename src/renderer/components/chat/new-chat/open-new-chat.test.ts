import { PersistenceKeys } from '@shared/types/persistence.types'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const { read, write, resolveEntries, tauriRef, readComposerOptions } = vi.hoisted(() => ({
  read: vi.fn(),
  write: vi.fn(async () => ({ success: true, data: undefined })),
  resolveEntries: vi.fn(),
  tauriRef: { current: true },
  readComposerOptions: vi.fn()
}))

vi.mock('@/lib/api', () => ({ persistenceApi: { read, write } }))
vi.mock('@/lib/log-api', () => ({ logFrontendError: vi.fn(async () => {}) }))
vi.mock('sonner', () => ({ toast: Object.assign(vi.fn(), { error: vi.fn() }) }))
vi.mock('@/lib/tauri-runtime', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/tauri-runtime')>()),
  isTauriContext: () => tauriRef.current
}))
vi.mock('@/lib/agents/supported-acp-agents', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/agents/supported-acp-agents')>()),
  resolveSupportedAcpAgents: resolveEntries
}))
vi.mock('@/stores/acp-store/composer-options', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/stores/acp-store/composer-options')>()),
  readComposerOptions
}))

import { useProjectSwitchStatus } from '@/lib/project-switch/status-store'
import { setRouterNavigate } from '@/lib/router-navigate'
import { useAcpStore } from '@/stores/acp-store'
import { useProjectStore } from '@/stores/project-store'
import { useQuickChatStore } from '@/stores/quick-chat-store'
import { useShellLayoutStore } from '@/stores/shell-layout-store'
import { agentChatTabId, useWorkspaceStore } from '@/stores/workspace-store'
import {
  newChatPlaceholderOptions,
  openNewChat,
  openNewQuickChat,
  replaceEmptyChat
} from './open-new-chat'

const config = (id: string) => ({ id, name: id, command: 'agent', args: [] })
const entry = (id: string, status = 'ready') => ({
  id,
  configId: id,
  agent: { id, name: id },
  config: config(id),
  status
})

function pane(tabs: unknown[] = [], activeTabId: string | null = null): void {
  useWorkspaceStore.setState({
    root: { type: 'leaf', id: 'pane-1', tabs, activeTabId },
    activePaneId: 'pane-1',
    agentLauncherPaneId: null,
    activeProjectId: ''
  } as never)
}

function lastSelected(agentId: string | null): void {
  read.mockImplementation(async (key: string) =>
    key === PersistenceKeys.lastSelectedAgent && agentId
      ? { success: true, data: { agentId, mode: 'acp' } }
      : { success: false, error: 'missing', code: 'KEY_NOT_FOUND' }
  )
}

beforeEach(() => {
  vi.clearAllMocks()
  tauriRef.current = true
  useProjectStore.setState({
    activeProjectId: 'p1',
    projects: [{ id: 'p1', name: 'termul', path: '/work', isGitRepo: true }]
  } as never)
  useShellLayoutStore.setState({ worktreePickByProject: {} } as never)
  useAcpStore.setState({
    agentConfigs: [config('cfg-a'), config('cfg-b')],
    agentOptionsCache: {},
    sessions: {},
    messages: {},
    promptQueues: {},
    launchingSessionIds: {}
  } as never)
  resolveEntries.mockResolvedValue([entry('cfg-a'), entry('cfg-b')])
  readComposerOptions.mockResolvedValue(null)
  lastSelected('cfg-b')
  pane()
})

const { createLaunchPlaceholder, finalizeChatLaunch, saveAgentConfig, startChat } =
  useAcpStore.getState()

afterEach(() => {
  vi.restoreAllMocks()
  useAcpStore.setState({ createLaunchPlaceholder, finalizeChatLaunch, saveAgentConfig, startChat })
})

describe('openNewChat', () => {
  it('opens the launcher in the pane and starts no session', async () => {
    useQuickChatStore.setState({ view: { mode: 'closed' } })
    const placeholder = vi.fn(() => 'launch-x')
    const start = vi.fn(async () => 'sess-x')
    useAcpStore.setState({ createLaunchPlaceholder: placeholder, startChat: start } as never)

    expect(await openNewChat({ reason: 'rail' })).toEqual({ kind: 'launcher' })

    expect(useWorkspaceStore.getState().agentLauncherPaneId).toBe('pane-1')
    expect(useAcpStore.getState().sessions).toEqual({})
    expect(placeholder).not.toHaveBeenCalled()
    expect(start).not.toHaveBeenCalled()
  })

  it('puts focus in the launcher composer once it can take typing', async () => {
    useQuickChatStore.setState({ view: { mode: 'closed' } })
    const button = document.createElement('button')
    const paneEl = document.createElement('div')
    paneEl.dataset.paneContent = 'pane-1'
    const editor = document.createElement('div')
    editor.setAttribute('aria-label', 'Agent prompt')
    editor.setAttribute('contenteditable', 'false')
    editor.tabIndex = -1
    paneEl.append(editor)
    document.body.append(button, paneEl)
    button.focus()
    try {
      await openNewChat({ reason: 'rail' })
      await new Promise((resolve) => requestAnimationFrame(resolve))
      expect(document.activeElement).toBe(button)

      editor.setAttribute('contenteditable', 'true')
      await vi.waitFor(() => expect(document.activeElement).toBe(editor))
    } finally {
      button.remove()
      paneEl.remove()
    }
  })

  it("opens the launcher in the project row's project once its switch lands", async () => {
    useProjectStore.setState({
      activeProjectId: 'p2',
      projects: [
        { id: 'p1', name: 'termul', path: '/work', isGitRepo: true },
        { id: 'p2', name: 'other', path: '/other', isGitRepo: true }
      ]
    } as never)
    useWorkspaceStore.setState({ activeProjectId: 'p1' } as never)
    useProjectSwitchStatus.getState().apply({ kind: 'pending', projectId: 'p2' })

    const run = openNewChat({ projectId: 'p2', reason: 'project-row' })
    useWorkspaceStore.setState({
      activeProjectId: 'p2',
      root: { type: 'leaf', id: 'pane-2', tabs: [], activeTabId: null },
      activePaneId: 'pane-2'
    } as never)
    useProjectSwitchStatus.getState().apply({ kind: 'idle' })

    expect(await run).toEqual({ kind: 'launcher' })
    expect(useWorkspaceStore.getState().agentLauncherPaneId).toBe('pane-2')
  })

  it("opens no launcher for the selected project in the previous project's pane (slow switch)", async () => {
    // p2 is selected, but p1's tree (with pane-1) is still the one on screen.
    useWorkspaceStore.setState({ activeProjectId: 'p1' } as never)
    useProjectStore.setState({
      activeProjectId: 'p2',
      projects: [
        { id: 'p1', name: 'termul', path: '/work', isGitRepo: true },
        { id: 'p2', name: 'other', path: '/other', isGitRepo: true }
      ]
    } as never)

    expect(await openNewChat({ paneId: 'pane-1', reason: 'new-tab-menu' })).toEqual({
      kind: 'skipped'
    })
    expect(useWorkspaceStore.getState().agentLauncherPaneId).toBeNull()
  })

  it('closes the open quick chat so the launcher of a project New chat is visible', async () => {
    useQuickChatStore.setState({ view: { mode: 'chat', sessionId: 'quick-1' } })
    expect(await openNewChat({ reason: 'shortcut' })).toEqual({ kind: 'launcher' })
    expect(useQuickChatStore.getState().view).toEqual({ mode: 'closed' })
  })

  it('opens the quick chat launcher when no project is active, and starts no session', async () => {
    useQuickChatStore.setState({ view: { mode: 'closed' } })
    useProjectStore.setState({ activeProjectId: '', projects: [] } as never)
    const createLaunchPlaceholder = vi.fn(() => 'launch-q')
    const finalizeChatLaunch = vi.fn(async () => 'sess-q')
    useAcpStore.setState({ createLaunchPlaceholder, finalizeChatLaunch } as never)

    expect(await openNewChat({ reason: 'rail' })).toEqual({ kind: 'launcher' })

    expect(useQuickChatStore.getState().view).toEqual({ mode: 'launcher' })
    expect(useWorkspaceStore.getState().agentLauncherPaneId).toBeNull()
    expect(useAcpStore.getState().sessions).toEqual({})
    expect(createLaunchPlaceholder).not.toHaveBeenCalled()
    expect(finalizeChatLaunch).not.toHaveBeenCalled()
  })
})

describe('openNewQuickChat', () => {
  const navigate = vi.fn()
  beforeEach(() => {
    navigate.mockReset()
    setRouterNavigate(navigate)
    window.location.hash = '#/'
  })
  afterEach(() => setRouterNavigate(null))

  it('opens the launcher in the quick view with no session and leaves the chat route', () => {
    useQuickChatStore.setState({ view: { mode: 'chat', sessionId: 'q-old' } })
    window.location.hash = '#/c/q-old'
    useWorkspaceStore.setState({ agentLauncherPaneId: 'pane-1' } as never)
    const createLaunchPlaceholder = vi.fn(() => 'launch-q')
    useAcpStore.setState({ createLaunchPlaceholder } as never)

    expect(openNewQuickChat('quick-chats-header')).toEqual({ kind: 'launcher' })

    expect(useQuickChatStore.getState().view).toEqual({ mode: 'launcher' })
    expect(navigate).toHaveBeenCalledWith('/')
    // A project launcher overlay under the quick view is dropped.
    expect(useWorkspaceStore.getState().agentLauncherPaneId).toBeNull()
    expect(createLaunchPlaceholder).not.toHaveBeenCalled()
    expect(useAcpStore.getState().sessions).toEqual({})
  })

  it('opens the launcher over an empty quick chat from history', () => {
    useQuickChatStore.setState({ view: { mode: 'chat', sessionId: 'q-empty' } })
    useAcpStore.setState({
      sessions: { 'q-empty': { id: 'q-empty', kind: 'quick', status: 'active' } },
      messages: { 'q-empty': [] }
    } as never)

    openNewQuickChat('quick-chats-header')

    expect(useQuickChatStore.getState().view).toEqual({ mode: 'launcher' })
  })

  it('focuses the launcher already on screen, once it can take typing', async () => {
    useQuickChatStore.setState({ view: { mode: 'launcher' } })
    const main = document.createElement('div')
    main.dataset.testid = 'quick-chat-main'
    const editor = document.createElement('div')
    editor.setAttribute('aria-label', 'Agent prompt')
    editor.setAttribute('contenteditable', 'true')
    editor.tabIndex = -1
    main.append(editor)
    document.body.append(main)
    try {
      expect(openNewQuickChat('command-palette-quick')).toEqual({ kind: 'launcher' })
      expect(useQuickChatStore.getState().view).toEqual({ mode: 'launcher' })
      expect(navigate).not.toHaveBeenCalled()
      await vi.waitFor(() => expect(document.activeElement).toBe(editor))
    } finally {
      main.remove()
    }
  })
})

describe('newChatPlaceholderOptions (the effort a new chat shows before it starts)', () => {
  const effort = (current: string) => ({
    id: 'effort',
    name: 'Effort',
    category: 'thought_level',
    type: 'select' as const,
    currentValue: current,
    options: ['low', 'medium', 'high'].map((value) => ({ value, name: value }))
  })
  const permission = {
    id: 'permission',
    name: 'Permission',
    category: 'mode',
    type: 'select' as const,
    currentValue: 'default',
    options: ['default', 'bypass'].map((value) => ({ value, name: value }))
  }
  // The agent's last session ran Opus at High: the cache holds its options.
  const cache = {
    models: {
      currentModelId: 'opus',
      availableModels: [
        { modelId: 'opus', name: 'Opus' },
        { modelId: 'haiku', name: 'Haiku' }
      ]
    },
    modes: null,
    configOptions: [effort('high'), permission]
  }

  it("drops another model's effort: the saved Haiku has no effort of its own", () => {
    const shown = newChatPlaceholderOptions(cache as never, {
      modelId: 'haiku',
      configValues: {}
    })
    expect(shown.models?.currentModelId).toBe('haiku')
    expect(shown.configOptions.find((o) => o.id === 'effort')).toBeUndefined()
    // Agent-wide options stay.
    expect(shown.configOptions.find((o) => o.id === 'permission')).toBeDefined()
  })

  it("paints the saved model's own effort", () => {
    const shown = newChatPlaceholderOptions(cache as never, {
      modelId: 'haiku',
      configValues: { effort: 'medium' }
    })
    expect(shown.configOptions.find((o) => o.id === 'effort')?.currentValue).toBe('medium')
  })

  it('keeps the cached effort when the saved model is the cached one', () => {
    const shown = newChatPlaceholderOptions(cache as never, { modelId: 'opus', configValues: {} })
    expect(shown.configOptions.find((o) => o.id === 'effort')?.currentValue).toBe('high')
  })
})

describe('replaceEmptyChat (another agent picked in an empty chat)', () => {
  function stubLaunch(): {
    createLaunchPlaceholder: ReturnType<typeof vi.fn>
    finalizeChatLaunch: ReturnType<typeof vi.fn>
    closeSession: ReturnType<typeof vi.fn>
    deleteHistorySession: ReturnType<typeof vi.fn>
    armAgentSwitch: ReturnType<typeof vi.fn>
  } {
    const stubs = {
      createLaunchPlaceholder: vi.fn(() => 'launch-r'),
      finalizeChatLaunch: vi.fn(async () => 'sess-r'),
      closeSession: vi.fn(async () => {}),
      deleteHistorySession: vi.fn(async () => {}),
      armAgentSwitch: vi.fn(async () => true)
    }
    useAcpStore.setState(stubs as never)
    return stubs
  }

  it('moves the empty chat to the new agent in the same tab, with no armed switch', async () => {
    const tabId = agentChatTabId('s-empty')
    pane([{ type: 'agent-chat', id: tabId, sessionId: 's-empty' }], tabId)
    useAcpStore.setState({
      sessions: {
        's-empty': { id: 's-empty', projectId: 'p1', cwd: '/work', status: 'active' }
      },
      messages: { 's-empty': [] }
    } as never)
    const stubs = stubLaunch()

    await replaceEmptyChat('s-empty', 'cfg-b', { modelId: 'glm-flash', modelConfigId: null })

    expect(stubs.createLaunchPlaceholder).toHaveBeenCalledWith(
      expect.objectContaining({ configId: 'cfg-b', cwd: '/work', projectId: 'p1' })
    )
    expect(stubs.finalizeChatLaunch).toHaveBeenCalledWith(
      expect.objectContaining({
        placeholderId: 'launch-r',
        configId: 'cfg-b',
        pending: expect.objectContaining({ modelId: 'glm-flash' })
      })
    )
    const leaf = useWorkspaceStore.getState().root as { tabs: { sessionId: string }[] }
    expect(leaf.tabs.map((t) => t.sessionId)).toEqual(['launch-r'])
    expect(stubs.deleteHistorySession).toHaveBeenCalledWith('s-empty')
    expect(stubs.armAgentSwitch).not.toHaveBeenCalled()
  })

  it('keeps a chat that started while the pick was read', async () => {
    useAcpStore.setState({
      sessions: {
        's-empty': { id: 's-empty', projectId: 'p1', cwd: '/work', status: 'active' }
      },
      messages: { 's-empty': [] }
    } as never)
    const stubs = stubLaunch()
    readComposerOptions.mockImplementation(async () => {
      useAcpStore.setState({ messages: { 's-empty': [{ id: 'm', role: 'user', blocks: [] }] } })
      return null
    })

    await replaceEmptyChat('s-empty', 'cfg-b', { modelId: 'glm-flash', modelConfigId: null })

    expect(stubs.createLaunchPlaceholder).not.toHaveBeenCalled()
    expect(stubs.deleteHistorySession).not.toHaveBeenCalled()
  })
})
