import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type {
  SessionConfigOption,
  SubagentProfileView,
  SubagentSettingsInput,
  SubagentSettingsView
} from '@/lib/acp-api'
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

const { getSettings, setSettings, toastError, prepareChat, acpState } = vi.hoisted(() => ({
  getSettings: vi.fn(),
  setSettings: vi.fn(),
  toastError: vi.fn(),
  prepareChat: vi.fn(),
  acpState: { current: {} as Record<string, unknown> }
}))

vi.mock('@/lib/api', () => ({
  acpApi: { getSubagentSettings: getSettings, setSubagentSettings: setSettings }
}))
vi.mock('@/lib/log-api', () => ({ logFrontendError: vi.fn() }))
vi.mock('sonner', () => ({ toast: { error: toastError, success: vi.fn() } }))
vi.mock('@/hooks/use-resolved-supported-acp-agents', () => ({
  useResolvedSupportedAcpAgents: () => []
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

/** The host: applies upserts and deletes to the view it holds, as `subagent_settings_set` does. */
function host(initial: SubagentSettingsView) {
  let current = initial
  getSettings.mockReset().mockImplementation(async () => current)
  setSettings.mockReset().mockImplementation(async (input: SubagentSettingsInput) => {
    let profiles = current.profiles.filter((p) => !input.deleteProfiles?.includes(p.id))
    for (const { previousId, profile: next } of input.upsertProfiles ?? []) {
      const agentName = next.agentId === PI ? 'Pi' : 'Claude Agent'
      const stored = { ...next, agentName, implicit: false }
      const at = profiles.findIndex((p) => p.id === (previousId ?? next.id))
      profiles = at >= 0 ? profiles.map((p, i) => (i === at ? stored : p)) : [...profiles, stored]
    }
    current = {
      ...current,
      enabled: input.enabled ?? current.enabled,
      maxRunning: input.maxRunning ?? current.maxRunning,
      maxDepth: input.maxDepth ?? current.maxDepth,
      profiles
    }
    return current
  })
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

describe('SubagentSettingsGroup', () => {
  beforeEach(() => {
    host(VIEW)
    toastError.mockReset()
    prepareChat.mockReset()
    acpState.current = { agentOptionsCache: CACHE }
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

  it("deletes one of an agent's profiles, but not its only one", async () => {
    await renderPage()
    expect(within(openMenu(PI)).getByRole('menuitem', { name: 'Delete' })).toHaveAttribute(
      'aria-disabled',
      'true'
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
