import { fireEvent, render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { AgentSkillSummary } from '@/lib/skills-api'
import { SkillsSection } from './skills-section'

const openFile = vi.fn()
const addEditorTab = vi.fn()
const closeSettings = vi.fn()
const skillsRef = { current: [] as AgentSkillSummary[] }
const projectRef = {
  current: { id: 'p1', name: 'Cloud Hello', path: '/tmp/termul-hello' } as
    | { id: string; name: string; path: string }
    | undefined
}

vi.mock('@/hooks/use-agent-skills', () => ({
  useAgentSkills: () => ({ skills: skillsRef.current, loading: false, reload: () => {} })
}))

vi.mock('@/stores/project-store', () => ({
  useActiveProject: () => projectRef.current
}))

vi.mock('@/stores/editor-store', () => ({
  useEditorStore: {
    getState: () => ({
      openFile,
      openFiles: new Map([['/home/u/.agents/skills/review/SKILL.md', {}]])
    })
  }
}))

vi.mock('@/stores/workspace-store', () => ({
  useWorkspaceStore: { getState: () => ({ addEditorTab }) }
}))

vi.mock('@/stores/settings-modal-store', () => ({
  useSettingsModalStore: { getState: () => ({ close: closeSettings }) }
}))

vi.mock('@/lib/log-api', () => ({
  logFrontendError: vi.fn()
}))

const globalSkill: AgentSkillSummary = {
  name: 'review',
  description: 'Review code',
  scope: 'global',
  path: '/home/u/.agents/skills/review/SKILL.md'
}

const projectSkill: AgentSkillSummary = {
  name: 'writing-guidelines',
  description: 'Review docs',
  scope: 'project',
  path: '/tmp/termul-hello/.agents/skills/writing-guidelines/SKILL.md'
}

describe('SkillsSection', () => {
  beforeEach(() => {
    skillsRef.current = [globalSkill, projectSkill]
    projectRef.current = { id: 'p1', name: 'Cloud Hello', path: '/tmp/termul-hello' }
    openFile.mockReset().mockResolvedValue(undefined)
    addEditorTab.mockReset()
    closeSettings.mockReset()
  })

  it('lists the project skills first, then the global skills', () => {
    render(<SkillsSection />)
    const project = screen.getByRole('heading', { name: 'Project' })
    const global = screen.getByRole('heading', { name: 'Global' })
    expect(project.compareDocumentPosition(global) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
    expect(screen.getByText('Cloud Hello · 1 skill')).toBeInTheDocument()
    expect(screen.getByText('Your home folder · 1 skill')).toBeInTheDocument()
    expect(screen.getByText('review')).toBeInTheDocument()
    expect(screen.getByText('writing-guidelines')).toBeInTheDocument()
  })

  it('filters by name or description and counts what matches', () => {
    render(<SkillsSection />)
    fireEvent.change(screen.getByLabelText('Filter skills'), { target: { value: 'docs' } })

    expect(screen.queryByText('review')).not.toBeInTheDocument()
    expect(screen.getByText('writing-guidelines')).toBeInTheDocument()
    expect(screen.getByRole('radio', { name: 'All 1' })).toBeInTheDocument()
    expect(screen.getByRole('radio', { name: 'Global 0' })).toBeInTheDocument()
  })

  it('shows a no-match panel with Clear filter', () => {
    render(<SkillsSection />)
    fireEvent.change(screen.getByLabelText('Filter skills'), { target: { value: 'zzz' } })

    expect(screen.getByText('No skills match "zzz".')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Clear filter' }))
    expect(screen.getByText('review')).toBeInTheDocument()
  })

  it('shows only one scope when the switch says so', () => {
    render(<SkillsSection />)
    fireEvent.click(screen.getByRole('radio', { name: /^Global/ }))

    expect(screen.queryByRole('heading', { name: 'Project' })).not.toBeInTheDocument()
    expect(screen.getByText('review')).toBeInTheDocument()
  })

  it('shows six rows, then Show N more', () => {
    skillsRef.current = Array.from({ length: 9 }, (_, i) => ({
      ...globalSkill,
      name: `skill-${i}`
    }))
    render(<SkillsSection />)

    expect(screen.getByText('skill-5')).toBeInTheDocument()
    expect(screen.queryByText('skill-6')).not.toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Show 3 more' }))
    expect(screen.getByText('skill-8')).toBeInTheDocument()
  })

  it('opens the skill file and closes settings', async () => {
    render(<SkillsSection />)
    fireEvent.click(screen.getByRole('button', { name: 'Open review' }))
    await vi.waitFor(() => {
      expect(openFile).toHaveBeenCalledWith(globalSkill.path)
    })
    expect(addEditorTab).toHaveBeenCalledWith(globalSkill.path)
    expect(closeSettings).toHaveBeenCalled()
  })

  it('asks for a project when none is open', () => {
    projectRef.current = undefined
    skillsRef.current = [globalSkill]
    render(<SkillsSection />)
    expect(screen.getByText('Open a project to see its skills.')).toBeInTheDocument()
    expect(screen.getByText('review')).toBeInTheDocument()
  })
})
