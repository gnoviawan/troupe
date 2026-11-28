import { fireEvent, render, screen, within } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { SubagentProfileView } from '@/lib/acp-api'
import { ProfileRow } from './ProfileRow'

const CLAUDE = 'acp-registry:claude-acp'
const PI = 'acp-registry:pi-acp'
const KEY_CLAUDE = `${CLAUDE}\0/work\0`
const KEY_PI = `${PI}\0/work\0`

const { prepareChat, cancelPreparedChat, acpState, projectState } = vi.hoisted(() => ({
  prepareChat: vi.fn(),
  cancelPreparedChat: vi.fn(),
  acpState: { current: {} as Record<string, unknown> },
  projectState: { activeProjectId: 'p1' as string | null }
}))

vi.mock('@/lib/log-api', () => ({ logFrontendError: vi.fn() }))
vi.mock('@/hooks/use-resolved-supported-acp-agents', () => ({
  useResolvedSupportedAcpAgents: () => []
}))
vi.mock('@/stores/project-store', () => ({
  useProjectStore: (selector: (s: { activeProjectId: string | null }) => unknown) =>
    selector(projectState)
}))
vi.mock('@/lib/worktree-context', () => ({
  getDefaultCwdForProject: (projectId: string) => (projectId ? '/work' : '')
}))
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
    cancelPreparedChat,
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

const makeProfile = (overrides: Partial<SubagentProfileView> = {}): SubagentProfileView => ({
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

function renderRow({
  profile = makeProfile(),
  expanded = true
}: {
  profile?: SubagentProfileView
  expanded?: boolean
} = {}) {
  const save = vi.fn(async () => {})
  const row = (): React.JSX.Element => (
    <ProfileRow
      profile={profile}
      profiles={[profile]}
      launchableAgentIds={new Set([CLAUDE, PI])}
      expanded={expanded}
      onToggle={vi.fn()}
      save={save}
      rename={vi.fn()}
      onDuplicate={vi.fn()}
      onDelete={vi.fn()}
    />
  )
  const view = render(row())
  return { save, rerender: () => view.rerender(row()) }
}

describe('ProfileRow Mode field', () => {
  beforeEach(() => {
    prepareChat.mockReset()
    cancelPreparedChat.mockReset()
    acpState.current = {}
    projectState.activeProjectId = 'p1'
  })

  it('prepares the agent on expand and shows "Loading modes…" while preparing', () => {
    acpState.current = {
      preparingChatKeys: { [KEY_CLAUDE]: true }
    }
    renderRow({ expanded: true })

    const panel = screen.getByRole('region', { name: 'claude-deep settings' })
    expect(within(panel).getByText('Mode')).toBeInTheDocument()
    expect(within(panel).getByText('Loading modes…')).toBeInTheDocument()
  })

  it('triggers prepare once when expanded and not yet prepared', () => {
    renderRow({ expanded: true })

    expect(prepareChat).toHaveBeenCalledTimes(1)
    expect(prepareChat).toHaveBeenCalledWith(CLAUDE, '/work', undefined, 'p1', { silent: true })
    expect(screen.getByText('Loading modes…')).toBeInTheDocument()
  })

  it('renders ModeChip when modes exist in cache', () => {
    acpState.current = {
      agentOptionsCache: {
        [CLAUDE]: {
          models: null,
          modes: {
            currentModeId: 'plan',
            availableModes: [
              { id: 'default', name: 'Default' },
              { id: 'plan', name: 'Plan' }
            ]
          },
          configOptions: [],
          updatedAt: 1
        }
      }
    }
    renderRow({ expanded: true })

    const panel = screen.getByRole('region', { name: 'claude-deep settings' })
    expect(within(panel).getByText('Mode')).toBeInTheDocument()
    expect(within(panel).getByRole('button', { name: 'Plan' })).toBeInTheDocument()
  })

  it('renders ModeChip from a saved mode before live modes load', () => {
    renderRow({
      profile: makeProfile({ mode: 'plan' }),
      expanded: true
    })

    const panel = screen.getByRole('region', { name: 'claude-deep settings' })
    expect(within(panel).getByRole('button', { name: 'plan' })).toBeInTheDocument()
  })

  it('shows "This agent has no modes." after prepare completes with none', () => {
    acpState.current = {
      preparedSessions: { [KEY_PI]: 'session-1' },
      agentOptionsCache: {
        [PI]: {
          models: null,
          modes: null,
          configOptions: [],
          updatedAt: 1
        }
      }
    }
    renderRow({
      profile: makeProfile({ id: 'pi-fast', agentId: PI, agentName: 'Pi' }),
      expanded: true
    })

    const panel = screen.getByRole('region', { name: 'pi-fast settings' })
    expect(within(panel).getByText('Mode')).toBeInTheDocument()
    expect(within(panel).getByText('This agent has no modes.')).toBeInTheDocument()
    expect(prepareChat).not.toHaveBeenCalled()
  })

  it('prepares again when the prepared session goes away while expanded', () => {
    acpState.current = { preparedSessions: { [KEY_CLAUDE]: 'session-1' } }
    const view = renderRow({ expanded: true })
    expect(screen.getByText('This agent has no modes.')).toBeInTheDocument()
    expect(prepareChat).not.toHaveBeenCalled()

    acpState.current = {}
    view.rerender()

    expect(prepareChat).toHaveBeenCalledWith(CLAUDE, '/work', undefined, 'p1', { silent: true })
  })

  it('shows the prepare error label and a Try again button that retries prepare', () => {
    acpState.current = {
      prepareChatErrors: {
        [KEY_CLAUDE]: {
          category: 'spawn',
          label: 'Could not start agent',
          detail: 'ENOENT'
        }
      }
    }
    renderRow({ expanded: true })

    const panel = screen.getByRole('region', { name: 'claude-deep settings' })
    expect(within(panel).getByRole('alert')).toHaveTextContent('Could not start agent')
    fireEvent.click(within(panel).getByRole('button', { name: 'Try again' }))

    expect(cancelPreparedChat).toHaveBeenCalledWith(KEY_CLAUDE)
    expect(prepareChat).toHaveBeenCalledWith(CLAUDE, '/work', undefined, 'p1', { silent: true })
  })

  it('shows the no-project error when no project is open and no modes are cached', () => {
    projectState.activeProjectId = null
    renderRow({ expanded: true })

    const panel = screen.getByRole('region', { name: 'claude-deep settings' })
    expect(within(panel).getByRole('alert')).toHaveTextContent('Open a project to load models')
    expect(within(panel).queryByRole('button', { name: 'Try again' })).toBeNull()
    expect(prepareChat).not.toHaveBeenCalled()
  })

  it('omits the Mode row and does not prepare an uninstalled profile', () => {
    renderRow({
      profile: makeProfile({ id: 'missing', agentId: 'custom:gone', agentName: null }),
      expanded: true
    })

    const panel = screen.getByRole('region', { name: 'missing settings' })
    expect(within(panel).queryByText('Mode')).toBeNull()
    expect(prepareChat).not.toHaveBeenCalled()
  })
})
