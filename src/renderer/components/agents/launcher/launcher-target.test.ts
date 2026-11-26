import { act, cleanup, renderHook } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { useAcpStore } from '@/stores/acp-store'
import { useProjectStore } from '@/stores/project-store'
import type { Project } from '@/types/project'
import { useLauncherPlace } from './launcher-target'

vi.mock('@/lib/log-api', () => ({ logFrontendError: vi.fn(() => Promise.resolve()) }))

const initialProjects = useProjectStore.getState()
const initialAcp = useAcpStore.getState()

function project(id: string, path: string): Project {
  return { id, name: id, path } as Project
}

function renderPaneLauncher() {
  return renderHook(() => useLauncherPlace({ kind: 'pane', paneId: 'pane1' }))
}

describe('useLauncherPlace for a pane', () => {
  afterEach(() => {
    cleanup()
    useProjectStore.setState(initialProjects, true)
    useAcpStore.setState(initialAcp, true)
  })

  it('follows a project path edited while the launcher is shown', () => {
    const retargetWarmPool = vi.fn()
    useAcpStore.setState({ retargetWarmPool, setSelectedAgentConfigId: vi.fn() })
    useProjectStore.setState({ projects: [project('p1', '/old')], activeProjectId: 'p1' })
    const { result } = renderPaneLauncher()
    expect(result.current.launchCwd).toBe('/old')

    act(() => useProjectStore.setState({ projects: [project('p1', '/new')] }))

    expect(result.current.projectRoot).toBe('/new')
    expect(result.current.skillsRoot).toBe('/new')
    expect(result.current.launchCwd).toBe('/new')
    expect(result.current.agentScope).toEqual({ cwd: '/new', projectId: 'p1' })
    result.current.warm?.('claude')
    expect(retargetWarmPool).toHaveBeenCalledWith('claude', '/new', 'p1')
  })

  it('can launch once a project picked before it was listed shows up', () => {
    const retargetWarmPool = vi.fn()
    useAcpStore.setState({ retargetWarmPool, setSelectedAgentConfigId: vi.fn() })
    useProjectStore.setState({ projects: [], activeProjectId: 'p2' })
    const { result } = renderPaneLauncher()
    expect(result.current.launchCwd).toBeNull()
    expect(result.current.warm).toBeNull()

    act(() => useProjectStore.setState({ projects: [project('p2', '/p2')] }))

    expect(result.current.launchCwd).toBe('/p2')
    result.current.warm?.('codex')
    expect(retargetWarmPool).toHaveBeenCalledWith('codex', '/p2', 'p2')
  })
})
