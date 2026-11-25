import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type {
  SessionConfigOption,
  SubagentProfile,
  SubagentProfileView,
  SubagentSettingsInput,
  SubagentSettingsView
} from '@/lib/acp-api'
import { storedProfile } from './subagent-profiles/profile-model'
import { SubagentSettingsGroup } from './subagent-settings-group'

if (typeof document.elementFromPoint !== 'function') {
  Object.defineProperty(document, 'elementFromPoint', {
    value: () => null,
    configurable: true,
    writable: true
  })
}

const CLAUDE = 'acp-registry:claude-acp'
const PI = 'acp-registry:pi-acp'

const { getSettings, setSettings, toastError, prepareChat, acpState, entries } = vi.hoisted(() => ({
  getSettings: vi.fn(),
  setSettings: vi.fn(),
  toastError: vi.fn(),
  prepareChat: vi.fn(),
  acpState: { current: {} as Record<string, unknown> },
  entries: { current: [] as unknown[] }
}))

vi.mock('@/lib/api', () => ({
  acpApi: { getSubagentSettings: getSettings, setSubagentSettings: setSettings },
  persistenceApi: {
    read: async () => ({ success: true, data: null }),
    write: async () => ({ success: true }),
    writeDebounced: async () => ({ success: true }),
    delete: async () => ({ success: true })
  }
}))
vi.mock('@/lib/log-api', () => ({ logFrontendError: vi.fn() }))
vi.mock('sonner', () => ({ toast: { error: toastError, success: vi.fn() } }))
vi.mock('@/hooks/use-resolved-supported-acp-agents', () => ({
  useResolvedSupportedAcpAgents: () => entries.current
}))
vi.mock('@/stores/project-store', () => ({
  useProjectStore: (selector: (s: { activeProjectId: string }) => unknown) =>
    selector({ activeProjectId: 'p1' })
}))
vi.mock('@/lib/worktree-context', () => ({ getDefaultCwdForProject: () => '/work' }))
vi.mock('@/stores/acp-store', () => {
  const state = () => ({
    agentConfigs: [],
    preparedSessions: {},
    preparingChatKeys: {},
    prepareChatErrors: {},
    agentOptionsCache: {},
    sessions: {},
    ...acpState.current,
    prepareChat,
    cancelPreparedChat: vi.fn(),
    cancelAgentSwitch: vi.fn()
  })
  const useAcpStore = (selector: (s: Record<string, unknown>) => unknown) => selector(state())
  useAcpStore.getState = state
  return {
    useAcpStore,
    useAgentTemplateId: () => null,
    useAgentIcon: () => null
  }
})

const profile = (overrides: Partial<SubagentProfileView>): SubagentProfileView => ({
  id: 'claude-deep',
  agentId: CLAUDE,
  agentName: 'Claude Agent',
  enabled: true,
  model: null,
  mode: null,
  configOptions: {},
  approvals: 'ask',
  description: null,
  implicit: false,
  ...overrides
})

const VIEW: SubagentSettingsView = {
  enabled: true,
  maxRunning: 4,
  maxDepth: 2,
  profiles: [
    profile({ id: 'claude-deep', model: 'opus', description: 'Hard Rust refactors.' }),
    profile({ id: 'claude-quick', model: 'haiku' }),
    profile({ id: PI, agentId: PI, agentName: 'Pi', implicit: true })
  ]
}

const option = (
  id: string,
  category: string,
  values: Array<[string, string]>,
  currentValue: string
): SessionConfigOption => ({
  id,
  name: id === 'model' ? 'Model' : 'Thinking',
  category,
  type: 'select',
  currentValue,
  options: values.map(([value, name]) => ({ value, name }))
})

const CACHE = {
  [PI]: {
    models: null,
    modes: null,
    configOptions: [
      option(
        'model',
        'model',
        [
          ['gpt-mini', 'GPT mini'],
          ['gemini-pro', 'Gemini Pro']
        ],
        'gpt-mini'
      ),
      option(
        'thinking',
        'thought_level',
        [
          ['low', 'Low'],
          ['high', 'High']
        ],
        'low'
      )
    ],
    updatedAt: 1
  },
  [CLAUDE]: {
    models: {
      currentModelId: 'opus',
      availableModels: [
        { modelId: 'opus', name: 'Opus' },
        { modelId: 'haiku', name: 'Haiku' }
      ]
    },
    modes: {
      currentModeId: 'default',
      availableModes: [
        { id: 'default', name: 'Default' },
        { id: 'plan', name: 'Plan' },
        { id: 'bypassPermissions', name: 'Bypass permissions' }
      ]
    },
    configOptions: [],
    updatedAt: 1
  }
}

const AGENTS: Readonly<Record<string, string>> = { [CLAUDE]: 'Claude Agent', [PI]: 'Pi' }

/**
 * The host's view of `stored`, as `Launchable::resolve` builds it: the stored
 * profiles, then the implicit default of every agent that no stored profile
 * points at or uses the key of.
 */
function resolve(stored: readonly SubagentProfile[]): SubagentProfileView[] {
  const implicit = Object.entries(AGENTS)
    .filter(
      ([key]) => !stored.some((p) => p.agentId === key || p.id.toLowerCase() === key.toLowerCase())
    )
    .map(([key, name]) => profile({ id: key, agentId: key, agentName: name, implicit: true }))
  return [
    ...stored.map((p) => ({ ...p, agentName: AGENTS[p.agentId] ?? null, implicit: false })),
    ...implicit
  ]
}

/**
 * The host: stores the profiles of `initial` that are not implicit and
 * answers `subagent_settings_set` as it does, refusing the whole call on an
 * upsert it refuses. `hold` keeps each call unapplied until `release`.
 */
function host(initial: SubagentSettingsView) {
  let gates = {
    enabled: initial.enabled,
    maxRunning: initial.maxRunning,
    maxDepth: initial.maxDepth
  }
  let stored: SubagentProfile[] = initial.profiles.filter((p) => !p.implicit).map(storedProfile)
  const view = (): SubagentSettingsView => ({ ...gates, profiles: resolve(stored) })
  const apply = (input: SubagentSettingsInput): SubagentSettingsView => {
    let next = stored.filter((p) => !input.deleteProfiles?.includes(p.id))
    for (const { previousId, profile: upsert } of input.upsertProfiles ?? []) {
      const owner = Object.entries(AGENTS).find(
        ([key]) => key.toLowerCase() === upsert.id.toLowerCase() && key !== upsert.agentId
      )
      if (owner) throw new Error(`the profile id ${upsert.id} is the id of the agent ${owner[1]}`)
      if (previousId && !resolve(next).some((p) => p.id === previousId)) {
        throw new Error(`no sub-agent profile is named ${previousId}`)
      }
      const at = next.findIndex((p) => p.id === (previousId ?? upsert.id))
      next = at >= 0 ? next.map((p, i) => (i === at ? upsert : p)) : [...next, upsert]
    }
    stored = next
    gates = {
      enabled: input.enabled ?? gates.enabled,
      maxRunning: input.maxRunning ?? gates.maxRunning,
      maxDepth: input.maxDepth ?? gates.maxDepth
    }
    return view()
  }
  const held: Array<() => void> = []
  getSettings.mockReset().mockImplementation(async () => view())
  setSettings.mockReset().mockImplementation(async (input: SubagentSettingsInput) => apply(input))
  return {
    stored: () => stored,
    hold: () =>
      setSettings.mockImplementation(
        (input: SubagentSettingsInput) =>
          new Promise<SubagentSettingsView>((resolve, reject) => {
            held.push(() => {
              try {
                resolve(apply(input))
              } catch (error) {
                reject(error)
              }
            })
          })
      ),
    /** Answers the newest held call: the host may run keyless calls in any order. */
    release: async () => {
      await act(async () => held.pop()?.())
    }
  }
}

const row = (id: string): HTMLElement => screen.getByTestId(`subagent-profile-${id}`)
const lastUpsert = () => {
  const input = setSettings.mock.calls.at(-1)?.[0] as SubagentSettingsInput
  return input.upsertProfiles?.[0]
}

async function renderPage(): Promise<void> {
  render(<SubagentSettingsGroup />)
  await screen.findByTestId('subagent-profile-claude-deep')
}

function expand(id: string): HTMLElement {
  fireEvent.click(within(row(id)).getByRole('button', { name: new RegExp(`^${id}`) }))
  return within(row(id)).getByRole('region', { name: `${id} settings` })
}

function openMenu(id: string): HTMLElement {
  const trigger = within(row(id)).getByRole('button', { name: `Actions for ${id}` })
  fireEvent.pointerDown(trigger, { button: 0, ctrlKey: false })
  return screen.getByRole('menu')
}

/** Both agents installed, so the model picker shows their tabs. */
function withAgentTabs(): void {
  const configs = [
    { id: CLAUDE, name: 'Claude Agent', command: 'claude', args: [] },
    { id: PI, name: 'Pi', command: 'pi', args: [] }
  ]
  acpState.current = { agentOptionsCache: CACHE, agentConfigs: configs }
  entries.current = configs.map((config) => ({
    id: config.id,
    configId: config.id,
    agent: { id: config.id, name: config.name, version: '', description: '', distribution: {} },
    config,
    status: 'ready',
    install: null,
    manualInstall: null,
    runtimeLauncher: null,
    unavailableReason: null
  }))
}

/** The agent tabs the model picker of profile `id` offers. */
function agentTabs(id: string): string[] {
  fireEvent.click(within(row(id)).getByTestId('agent-model-selector-trigger'))
  const names = within(screen.getAllByTestId('agent-model-selector-panel').at(-1) as HTMLElement)
    .queryAllByRole('tab')
    .map((tab) => tab.getAttribute('aria-label') ?? '')
  fireEvent.keyDown(document.body, { key: 'Escape' })
  return names
}

describe('SubagentSettingsGroup', () => {
  beforeEach(() => {
    host(VIEW)
    toastError.mockReset()
    prepareChat.mockReset()
    acpState.current = { agentOptionsCache: CACHE }
    entries.current = []
  })

  it("offers other agents' tabs, except to a profile named after its own agent", async () => {
    withAgentTabs()
    await renderPage()

    expect(agentTabs('claude-quick')).toEqual([
      expect.stringContaining('Claude Agent'),
      expect.stringContaining('Pi')
    ])
    expect(agentTabs(PI).filter((name) => name.includes('Claude'))).toEqual([])
  })

  it("offers no other agent to an agent's only profile, and says why", async () => {
    withAgentTabs()
    host({ ...VIEW, profiles: VIEW.profiles.filter((p) => p.id !== 'claude-quick') })
    await renderPage()

    expect(agentTabs('claude-deep')).toEqual([])
    expect(
      within(expand('claude-deep')).getByText(
        "This agent's only profile stays on it. Duplicate it to use another agent."
      )
    ).toBeInTheDocument()
  })

  it("stores the other agent's implicit default with a profile moved to it", async () => {
    withAgentTabs()
    const h = host(VIEW)
    await renderPage()
    fireEvent.click(within(row('claude-quick')).getByTestId('agent-model-selector-trigger'))
    fireEvent.click(screen.getByRole('tab', { name: /Pi/ }))

    await waitFor(() => expect(setSettings).toHaveBeenCalled())
    expect(setSettings).toHaveBeenLastCalledWith({
      upsertProfiles: [
        { profile: storedProfile(VIEW.profiles[2]) },
        { profile: expect.objectContaining({ id: 'claude-quick', agentId: PI, model: null }) }
      ]
    })
    await waitFor(() =>
      expect(h.stored().map((p) => [p.id, p.agentId])).toEqual([
        ['claude-deep', CLAUDE],
        ['claude-quick', PI],
        [PI, PI]
      ])
    )
    expect(row(PI)).toBeInTheDocument()
  })

  it('lists every profile with its note, and names the enabled ones the calling agent sees', async () => {
    host({
      ...VIEW,
      profiles: VIEW.profiles.map((p) => (p.id === 'claude-quick' ? { ...p, enabled: false } : p))
    })
    await renderPage()

    expect(screen.getByText('Profiles · 3 · 2 on')).toBeInTheDocument()
    expect(within(row('claude-deep')).getByText('Hard Rust refactors.')).toBeInTheDocument()
    expect(within(row('claude-quick')).getByText('Claude Agent. No note yet.')).toBeInTheDocument()
    expect(
      screen.getByText(
        (_, node) => node?.tagName === 'P' && /calling agent sees/.test(node.textContent ?? '')
      )
    ).toHaveTextContent(`The calling agent sees claude-deep and ${PI} with their notes.`)
  })

  it('adds a profile for the first agent, named after it, and opens it', async () => {
    await renderPage()
    fireEvent.click(screen.getByRole('button', { name: 'Add profile' }))

    await waitFor(() => expect(row('claude-2')).toBeInTheDocument())
    expect(lastUpsert()).toEqual({
      previousId: undefined,
      profile: {
        id: 'claude-2',
        agentId: CLAUDE,
        enabled: true,
        model: null,
        mode: null,
        configOptions: {},
        approvals: 'ask',
        description: null
      }
    })
    expect(within(row('claude-2')).getByLabelText('Name')).toHaveValue('claude-2')
  })

  it('renames a profile through previousId and refuses a taken or malformed name', async () => {
    await renderPage()
    const panel = expand('claude-deep')
    const name = within(panel).getByLabelText('Name')

    fireEvent.change(name, { target: { value: 'CLAUDE-QUICK' } })
    expect(within(panel).getByText('Another profile has this name.')).toBeInTheDocument()
    fireEvent.blur(name)
    fireEvent.change(name, { target: { value: 'two words' } })
    expect(within(panel).getByText('Use letters, digits, and . _ : - only.')).toBeInTheDocument()
    fireEvent.blur(name)
    expect(setSettings).not.toHaveBeenCalled()

    fireEvent.change(name, { target: { value: 'claude-rust' } })
    fireEvent.keyDown(name, { key: 'Enter' })

    await waitFor(() => expect(row('claude-rust')).toBeInTheDocument())
    expect(lastUpsert()).toEqual({
      previousId: 'claude-deep',
      profile: expect.objectContaining({ id: 'claude-rust', model: 'opus' })
    })
    expect(within(row('claude-rust')).getByLabelText('Name')).toHaveValue('claude-rust')
  })

  it('duplicates a profile under the next free name', async () => {
    await renderPage()
    fireEvent.click(within(openMenu('claude-deep')).getByRole('menuitem', { name: 'Duplicate' }))

    await waitFor(() => expect(row('claude-deep-2')).toBeInTheDocument())
    expect(lastUpsert()?.profile).toEqual(
      expect.objectContaining({
        id: 'claude-deep-2',
        agentId: CLAUDE,
        model: 'opus',
        description: 'Hard Rust refactors.'
      })
    )
  })

  it('keeps an implicit default listed when its duplicate is stored', async () => {
    const h = host(VIEW)
    await renderPage()
    fireEvent.click(within(openMenu(PI)).getByRole('menuitem', { name: 'Duplicate' }))

    await waitFor(() => expect(h.stored().map((p) => p.id)).toContain('pi-2'))
    expect(setSettings).toHaveBeenLastCalledWith({
      upsertProfiles: [
        { profile: storedProfile(VIEW.profiles[2]) },
        { profile: expect.objectContaining({ id: 'pi-2', agentId: PI }) }
      ]
    })
    expect(h.stored().map((p) => p.id)).toEqual(['claude-deep', 'claude-quick', PI, 'pi-2'])
    await waitFor(() => expect(row('pi-2')).toBeInTheDocument())
    expect(row(PI)).toBeInTheDocument()
  })

  it("deletes one of an agent's profiles, but not its only one", async () => {
    await renderPage()
    const onlyDelete = within(openMenu(PI)).getByRole('menuitem', { name: 'Delete' })
    expect(onlyDelete).toHaveAttribute('aria-disabled', 'true')
    expect(onlyDelete).toHaveAccessibleDescription(
      "This agent's only profile. Switch it off instead."
    )
    fireEvent.keyDown(screen.getByRole('menu'), { key: 'Escape' })

    fireEvent.click(within(openMenu('claude-quick')).getByRole('menuitem', { name: 'Delete' }))
    await waitFor(() => expect(screen.queryByTestId('subagent-profile-claude-quick')).toBeNull())
    expect(setSettings).toHaveBeenLastCalledWith({ deleteProfiles: ['claude-quick'] })
  })

  it('switches a profile off', async () => {
    await renderPage()
    fireEvent.click(within(row('claude-quick')).getByRole('switch'))

    await waitFor(() => expect(screen.getByText('Profiles · 3 · 2 on')).toBeInTheDocument())
    expect(lastUpsert()?.profile).toEqual(
      expect.objectContaining({ id: 'claude-quick', enabled: false })
    )
  })

  it('sets approvals to Allow everything, warns, and marks the model chip', async () => {
    await renderPage()
    const panel = expand('claude-quick')
    expect(within(row('claude-quick')).queryByTestId('subagent-profile-shield')).toBeNull()

    fireEvent.click(within(panel).getByRole('radio', { name: 'Allow everything' }))

    await waitFor(() =>
      expect(
        within(panel).getByText(/answers every permission prompt with Allow/)
      ).toBeInTheDocument()
    )
    expect(lastUpsert()?.profile).toEqual(
      expect.objectContaining({ id: 'claude-quick', approvals: 'allowAll' })
    )
    expect(within(panel).getByRole('radio', { name: 'Allow everything' })).toHaveAttribute(
      'aria-checked',
      'true'
    )
    expect(within(row('claude-quick')).getByTestId('subagent-profile-shield')).toBeInTheDocument()
  })

  it('saves the note on blur, and a blank note as none', async () => {
    await renderPage()
    const panel = expand('claude-deep')
    const note = within(panel).getByLabelText('When to use')
    fireEvent.change(note, { target: { value: '   ' } })
    fireEvent.blur(note)

    await waitFor(() => expect(setSettings).toHaveBeenCalled())
    expect(lastUpsert()?.profile).toEqual(
      expect.objectContaining({ id: 'claude-deep', description: null })
    )
  })

  it('writes a model and its thinking level picked in the composer picker', async () => {
    await renderPage()
    const trigger = within(row(PI)).getByTestId('agent-model-selector-trigger')
    expect(trigger).toHaveTextContent('GPT mini')
    fireEvent.click(trigger)
    const panel = await screen.findByTestId('agent-model-selector-panel')
    expect(prepareChat).toHaveBeenCalledWith(PI, '/work', undefined, 'p1', { silent: true })

    fireEvent.click(within(panel).getByRole('button', { name: /Gemini Pro/ }))
    await waitFor(() => expect(setSettings).toHaveBeenCalled())
    expect(lastUpsert()).toEqual({
      previousId: undefined,
      profile: expect.objectContaining({
        id: PI,
        agentId: PI,
        model: 'gemini-pro',
        configOptions: { model: 'gemini-pro' }
      })
    })

    const thinking = screen.getByRole('group', { name: /Thinking|Effort/ })
    fireEvent.click(within(thinking).getByRole('button', { name: 'High' }))
    await waitFor(() =>
      expect(lastUpsert()?.profile).toEqual(
        expect.objectContaining({
          model: 'gemini-pro',
          configOptions: { model: 'gemini-pro', thinking: 'high' }
        })
      )
    )
    await waitFor(() =>
      expect(within(row(PI)).getByTestId('agent-model-selector-trigger')).toHaveTextContent(
        'Gemini ProHigh'
      )
    )
  })

  it("picks one of the agent's modes, and marks a bypass mode on the chip", async () => {
    await renderPage()
    const panel = expand('claude-quick')
    const modeChip = within(panel).getByRole('button', { name: /Ask first/ })
    fireEvent.pointerDown(modeChip, { button: 0, ctrlKey: false })
    fireEvent.click(modeChip)
    fireEvent.click(await screen.findByRole('button', { name: /Bypass permissions/ }))

    await waitFor(() =>
      expect(lastUpsert()?.profile).toEqual(
        expect.objectContaining({ id: 'claude-quick', mode: 'bypassPermissions' })
      )
    )
    await waitFor(() =>
      expect(within(row('claude-quick')).getByTestId('subagent-profile-shield')).toBeInTheDocument()
    )
  })

  it('keeps a saved model the agent no longer lists', async () => {
    host({ ...VIEW, profiles: [profile({ id: 'claude-deep', model: 'opus-legacy' })] })
    await renderPage()
    expect(
      within(row('claude-deep')).getByTestId('agent-model-selector-trigger')
    ).toHaveTextContent('opus-legacy')
  })

  it('sends only profile edits, never the removed per-agent fields', async () => {
    await renderPage()
    fireEvent.click(within(row('claude-quick')).getByRole('switch'))
    fireEvent.click(screen.getByRole('button', { name: 'Add profile' }))
    await waitFor(() => expect(setSettings).toHaveBeenCalledTimes(2))
    for (const [input] of setSettings.mock.calls) {
      expect(Object.keys(input as object)).not.toEqual(
        expect.arrayContaining([expect.stringMatching(/^(agents|models|descriptions)$/)])
      )
    }
  })

  it('sends one change at a time, each built on the answer to the one before', async () => {
    const h = host(VIEW)
    await renderPage()
    h.hold()
    const panel = expand('claude-quick')
    fireEvent.click(within(row('claude-quick')).getByRole('switch'))
    fireEvent.click(within(panel).getByRole('radio', { name: 'Allow everything' }))
    const note = within(panel).getByLabelText('When to use')
    fireEvent.change(note, { target: { value: 'Quick fixes.' } })
    fireEvent.blur(note)
    await waitFor(() => expect(setSettings).toHaveBeenCalledTimes(1))

    await h.release()
    expect(within(panel).getByRole('radio', { name: 'Allow everything' })).toHaveAttribute(
      'aria-checked',
      'true'
    )
    expect(within(row('claude-quick')).getByRole('switch')).toHaveAttribute('aria-checked', 'false')
    await waitFor(() => expect(setSettings).toHaveBeenCalledTimes(2))
    await h.release()
    await waitFor(() => expect(setSettings).toHaveBeenCalledTimes(3))
    await h.release()

    expect(h.stored().find((p) => p.id === 'claude-quick')).toEqual(
      expect.objectContaining({
        enabled: false,
        approvals: 'allowAll',
        description: 'Quick fixes.'
      })
    )
  })

  it('applies an edit made before a rename was answered to the renamed profile', async () => {
    const h = host(VIEW)
    await renderPage()
    h.hold()
    const name = within(expand('claude-deep')).getByLabelText('Name')
    fireEvent.change(name, { target: { value: 'claude-rust' } })
    fireEvent.keyDown(name, { key: 'Enter' })
    fireEvent.click(within(row('claude-rust')).getByRole('switch'))
    await waitFor(() => expect(setSettings).toHaveBeenCalledTimes(1))

    await h.release()
    await waitFor(() => expect(setSettings).toHaveBeenCalledTimes(2))
    await h.release()

    expect(h.stored().map((p) => [p.id, p.enabled])).toEqual([
      ['claude-rust', false],
      ['claude-quick', true]
    ])
  })

  it('keeps a profile open under its old name when the host refuses the rename', async () => {
    const h = host({
      ...VIEW,
      profiles: [
        ...VIEW.profiles.slice(0, 2),
        profile({ id: 'pi-fast', agentId: PI, agentName: 'Pi' })
      ]
    })
    await renderPage()
    h.hold()
    const name = within(expand('claude-deep')).getByLabelText('Name')
    fireEvent.change(name, { target: { value: PI } })
    fireEvent.keyDown(name, { key: 'Enter' })
    fireEvent.click(within(row(PI)).getByRole('switch'))
    await waitFor(() => expect(setSettings).toHaveBeenCalledTimes(1))

    await h.release()
    await waitFor(() => expect(row('claude-deep')).toBeInTheDocument())
    expect(toastError).toHaveBeenCalledWith('Could not save the sub-agent setting.', {
      description: `the profile id ${PI} is the id of the agent Pi`
    })
    expect(
      within(row('claude-deep')).getByRole('region', { name: 'claude-deep settings' })
    ).toBeInTheDocument()
    expect(screen.queryByTestId(`subagent-profile-${PI}`)).toBeNull()
    expect(setSettings).toHaveBeenCalledTimes(1)
    expect(h.stored().map((p) => p.id)).toEqual(['claude-deep', 'claude-quick', 'pi-fast'])
  })

  it('tells the user when the host refuses a change', async () => {
    await renderPage()
    setSettings.mockRejectedValueOnce(new Error('the profile id pi is the id of the agent Pi'))
    fireEvent.click(within(row('claude-quick')).getByRole('switch'))
    await act(async () => {})
    expect(toastError).toHaveBeenCalledWith('Could not save the sub-agent setting.', {
      description: 'the profile id pi is the id of the agent Pi'
    })
  })
})
