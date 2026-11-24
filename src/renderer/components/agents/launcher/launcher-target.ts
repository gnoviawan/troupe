import { UNASSIGNED_PROJECT_ID } from '@shared/types/web-projects.types'
import { useMemo } from 'react'
import { leaveQuickChatView } from '@/lib/no-project-chat'
import { getDefaultCwdForProject, getProjectRootPath } from '@/lib/worktree-context'
import { QUICK_CHAT_AGENT_CWD } from '@/stores/acp-reuse-keys'
import { useAcpStore } from '@/stores/acp-store'
import { useActiveProject, useProjectStore } from '@/stores/project-store'
import { useWorkspaceStore } from '@/stores/workspace-store'
import type { Project } from '@/types/project'
import type { LaunchSurface } from './launch-chat'

/**
 * Where an agent launcher sits and what its chat belongs to: a pane of the
 * active project (empty pane or the New chat overlay), or the quick chat view
 * (a quick chat has no project; the host owns its folder).
 */
export type LauncherTarget = { kind: 'pane'; paneId: string } | { kind: 'quick' }

export const QUICK_LAUNCHER_TARGET: LauncherTarget = { kind: 'quick' }

/** Everything the launcher reads from its target, resolved in one place. */
export interface LauncherPlace {
  /** Stable key of this launcher (the model selector's draft key). */
  key: string
  /** The pane the launcher sits in; absent for a quick chat. */
  paneId: string | undefined
  /** The project the chat belongs to: the active one (`''` when none), `__unassigned__` for a quick chat. */
  projectId: string
  /** The project shown for the workspace chip and the hero; `null` for a quick chat. */
  project: Project | null
  /** The project folder: mentions, the warm pool. Absent for a quick chat. */
  projectRoot: string | undefined
  /** Where project skills live; absent for a quick chat (global skills only). */
  skillsRoot: string | undefined
  /**
   * The cwd the chat starts with; `null` when it cannot start (no active
   * project). Empty for a quick chat: the host picks its folder.
   */
  launchCwd: string | null
  /** A quick chat: no project, no workspace chip, the hero names no folder. */
  quick: boolean
  /**
   * The cwd and project the agent process runs under (its reuse key): sign-in
   * reads the live agent's auth methods there.
   */
  agentScope: { cwd: string | undefined; projectId: string | null }
  /** Where `launchChat` opens the chat. */
  surface: LaunchSurface
  /** Where the launch morph looks for the new chat's composer. */
  chatHostSelector: string
  /** Close the launcher with no launch (Escape). */
  dismiss: () => void
  /**
   * Warm the picked agent so Send pays less: a pane retargets the warm pool
   * (a prepared session), a quick chat starts the agent process only.
   * `null` when nothing can be warmed (no active project).
   */
  warm: ((configId: string) => void) | null
}

/** The {@link LauncherPlace} of `target`. */
export function useLauncherPlace(target: LauncherTarget): LauncherPlace {
  const activeProjectId = useProjectStore((s) => s.activeProjectId)
  const activeProject = useActiveProject() ?? null
  const paneId = target.kind === 'pane' ? target.paneId : undefined
  // The project is read per render; the rest (and the callbacks effects
  // depend on) stays stable while the pane and the active project do.
  const place = useMemo((): Omit<LauncherPlace, 'project'> => {
    if (paneId === undefined) {
      return {
        key: 'quick',
        paneId: undefined,
        projectId: UNASSIGNED_PROJECT_ID,
        projectRoot: undefined,
        skillsRoot: undefined,
        launchCwd: '',
        quick: true,
        agentScope: { cwd: QUICK_CHAT_AGENT_CWD, projectId: null },
        surface: { kind: 'quick' },
        chatHostSelector: '[data-testid="quick-chat-main"]',
        dismiss: () => leaveQuickChatView('agentLauncher.dismiss'),
        warm: (configId) => {
          const store = useAcpStore.getState()
          store.setSelectedAgentConfigId(configId)
          void store.prewarmAgent(configId, QUICK_CHAT_AGENT_CWD)
        }
      }
    }
    const projectRoot = activeProjectId ? getDefaultCwdForProject(activeProjectId) : undefined
    return {
      key: paneId,
      paneId,
      projectId: activeProjectId,
      projectRoot,
      skillsRoot: activeProjectId ? getProjectRootPath(activeProjectId) : undefined,
      launchCwd: activeProjectId && projectRoot ? projectRoot : null,
      quick: false,
      agentScope: { cwd: projectRoot, projectId: activeProjectId || null },
      surface: { kind: 'pane', paneId },
      chatHostSelector: '[data-pane-content]',
      dismiss: () => useWorkspaceStore.getState().hideAgentLauncher(),
      warm:
        activeProjectId && projectRoot
          ? (configId) => {
              // Retarget the app-level warm pool to this agent+cwd: drains
              // stale pooled sessions for other agents (same cwd) and seeds a
              // warm session for this one. The pool owns the session
              // lifecycle, so a warm session stays ready for the next chat.
              const store = useAcpStore.getState()
              store.setSelectedAgentConfigId(configId)
              store.retargetWarmPool(configId, projectRoot, activeProjectId)
            }
          : null
    }
  }, [paneId, activeProjectId])
  return { ...place, project: paneId === undefined ? null : activeProject }
}
