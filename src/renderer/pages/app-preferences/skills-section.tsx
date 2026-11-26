import { useCallback, useMemo, useState } from 'react'
import { toast } from 'sonner'
import { ChevronDown, Folder } from '@/components/icons'
import { SettingsFilter } from '@/components/settings/SettingsFilter'
import {
  SETTINGS_CARD_CLASS,
  SettingsCard,
  SettingsGroup
} from '@/components/settings/SettingsGroup'
import { SettingsSection } from '@/components/settings/SettingsLayout'
import { SettingsSegmented } from '@/components/settings/SettingsSegmented'
import { Button } from '@/components/ui/button'
import { FOCUS_RING_CLASS } from '@/components/ui/panel-styles'
import { useAgentSkills } from '@/hooks/use-agent-skills'
import { openerApi } from '@/lib/api'
import { logFrontendError } from '@/lib/log-api'
import { isMac, isWindows } from '@/lib/platform'
import type { AgentSkillSummary } from '@/lib/skills-api'
import { isTauriContext } from '@/lib/tauri-runtime'
import { cn } from '@/lib/utils'
import { useEditorStore } from '@/stores/editor-store'
import { useActiveProject } from '@/stores/project-store'
import { useSettingsModalStore } from '@/stores/settings-modal-store'
import { useWorkspaceStore } from '@/stores/workspace-store'

type Scope = 'all' | 'global' | 'project'

/** Rows a long list shows before "Show N more". */
const FIRST_ROWS = 6

const REVEAL_LABEL = isMac
  ? 'Show in Finder'
  : isWindows
    ? 'Show in File Explorer'
    : 'Show in file manager'

const countText = (n: number): string => (n === 1 ? '1 skill' : `${n} skills`)

function matches(skill: AgentSkillSummary, query: string): boolean {
  if (!query) return true
  return skill.name.toLowerCase().includes(query) || skill.description.toLowerCase().includes(query)
}

async function openSkill(skill: AgentSkillSummary): Promise<void> {
  try {
    await useEditorStore.getState().openFile(skill.path)
    if (!useEditorStore.getState().openFiles.has(skill.path)) {
      throw new Error('editor tab limit reached')
    }
    useWorkspaceStore.getState().addEditorTab(skill.path)
    useSettingsModalStore.getState().close()
  } catch (err) {
    toast.error(`Could not open ${skill.name}`)
    void logFrontendError({
      level: 'warn',
      source: 'SkillsSection',
      message: `Failed to open skill '${skill.name}': ${err instanceof Error ? err.message : String(err)}`
    })
  }
}

async function revealSkill(skill: AgentSkillSummary): Promise<void> {
  const result = await openerApi.revealInFileManager(skill.path)
  if (result.success) return
  toast.error(`Could not show ${skill.name}`)
  void logFrontendError({
    level: 'warn',
    source: 'SkillsSection.reveal',
    message: `Failed to reveal skill '${skill.name}': ${result.error}`
  })
}

/**
 * The Skills page: a filter and a scope switch, then the open project's
 * skills and the skills in your home folder. A skill's name opens its
 * SKILL.md in the editor; on the desktop a button shows the file in the
 * file manager, with the path in its tooltip.
 */
export function SkillsSection(): React.JSX.Element {
  const project = useActiveProject()
  const { skills: listed, loading } = useAgentSkills(project?.path)
  const [filter, setFilter] = useState('')
  const [scope, setScope] = useState<Scope>('all')
  const query = filter.trim().toLowerCase()

  const { globalSkills, projectSkills } = useMemo(() => {
    const skills = (Array.isArray(listed) ? listed : []).filter((skill) => matches(skill, query))
    return {
      globalSkills: skills.filter((skill) => skill.scope === 'global'),
      projectSkills: skills.filter((skill) => skill.scope === 'project')
    }
  }, [listed, query])

  const showProject = scope !== 'global'
  const showGlobal = scope !== 'project'
  const nothingMatches =
    query.length > 0 &&
    (showProject ? projectSkills.length : 0) + (showGlobal ? globalSkills.length : 0) === 0

  return (
    <SettingsSection id="skills">
      <div className="flex flex-wrap items-center gap-2">
        <SettingsFilter
          className="min-w-48 flex-1"
          label="Filter skills"
          placeholder="Filter by name or description"
          value={filter}
          onValueChange={setFilter}
        />
        <SettingsSegmented
          label="Which skills"
          size="toolbar"
          value={scope}
          onValueChange={setScope}
          options={[
            { value: 'all', label: 'All', count: globalSkills.length + projectSkills.length },
            { value: 'global', label: 'Global', count: globalSkills.length },
            { value: 'project', label: 'Project', count: projectSkills.length }
          ]}
        />
      </div>

      {loading ? (
        <p className="text-xs text-muted-foreground">Loading skills…</p>
      ) : nothingMatches ? (
        <div className={cn('flex flex-col items-center gap-3 px-4 py-8', SETTINGS_CARD_CLASS)}>
          <p className="text-xs text-muted-foreground">No skills match "{filter.trim()}".</p>
          <Button type="button" size="xs" variant="secondary" onClick={() => setFilter('')}>
            Clear filter
          </Button>
        </div>
      ) : (
        <>
          {showProject && (
            <SettingsGroup
              title="Project"
              meta={project ? `${project.name} · ${countText(projectSkills.length)}` : undefined}
            >
              {project ? (
                <SkillList skills={projectSkills} />
              ) : (
                <EmptyNote>Open a project to see its skills.</EmptyNote>
              )}
            </SettingsGroup>
          )}
          {showGlobal && (
            <SettingsGroup
              title="Global"
              meta={`Your home folder · ${countText(globalSkills.length)}`}
            >
              <SkillList skills={globalSkills} />
            </SettingsGroup>
          )}
        </>
      )}
    </SettingsSection>
  )
}

function EmptyNote({ children }: { children: React.ReactNode }): React.JSX.Element {
  return (
    <div className={cn('px-4 py-3 text-xs text-muted-foreground', SETTINGS_CARD_CLASS)}>
      {children}
    </div>
  )
}

function SkillList({ skills }: { skills: AgentSkillSummary[] }): React.JSX.Element {
  const [showAll, setShowAll] = useState(false)
  if (skills.length === 0) return <EmptyNote>No skills in this folder.</EmptyNote>
  const shown = showAll ? skills : skills.slice(0, FIRST_ROWS)
  const hidden = skills.length - shown.length
  return (
    <>
      <SettingsCard>
        {shown.map((skill) => (
          <SkillRow key={`${skill.scope}:${skill.name}`} skill={skill} />
        ))}
      </SettingsCard>
      {hidden > 0 && (
        <Button
          type="button"
          variant="ghost"
          size="xs"
          className="self-start text-secondary-foreground"
          onClick={() => setShowAll(true)}
        >
          Show {hidden} more
          <ChevronDown size={12} />
        </Button>
      )}
    </>
  )
}

function SkillRow({ skill }: { skill: AgentSkillSummary }): React.JSX.Element {
  const open = useCallback(() => void openSkill(skill), [skill])
  return (
    <div className="flex items-center gap-3 py-2.5 pl-4 pr-2.5">
      <div className="min-w-0 flex-1">
        <button
          type="button"
          onClick={open}
          aria-label={`Open ${skill.name}`}
          title="Open SKILL.md"
          className={cn(
            'max-w-full truncate rounded-sm text-left text-xs font-medium leading-4 text-foreground underline-offset-2 hover:underline',
            FOCUS_RING_CLASS
          )}
        >
          {skill.name}
        </button>
        {skill.description && (
          <p className="fade-end mt-0.5 text-xs leading-4 text-muted-foreground">
            {skill.description}
          </p>
        )}
      </div>
      {isTauriContext() && (
        <Button
          type="button"
          size="icon-sm"
          variant="ghost"
          className="size-7 shrink-0 text-muted-foreground"
          onClick={() => void revealSkill(skill)}
          aria-label={`${REVEAL_LABEL}: ${skill.name}`}
          title={`${REVEAL_LABEL}\n${skill.path}`}
        >
          <Folder size={14} />
        </Button>
      )}
    </div>
  )
}
