import { UNASSIGNED_PROJECT_ID } from '@shared/types/web-projects.types'
import {
  emptyPendingLauncherOptions,
  hasPendingLauncherOptions,
  optionsToPending,
  overlayPendingLauncherOptions,
  type PendingLauncherOptions,
  restoredLauncherOptions
} from '@/components/agents/pending-launcher-options'
import {
  pendingForModelPick,
  type SwitchModelPick
} from '@/components/chat/agent-model-selector/use-agent-switch'
import { newChatKind } from '@/components/shell/open-new-chat'
import type { SessionId } from '@/lib/acp-api'
import { focusComposerWhenReady } from '@/lib/focus-composer'
import { logFrontendError } from '@/lib/log-api'
import {
  adoptQuickChat,
  focusQuickLauncherComposer,
  leaveQuickChatView,
  showQuickChatLauncher
} from '@/lib/no-project-chat'
import { committedPaneProject } from '@/lib/pane-target'
import { whenProjectSwitchSettled } from '@/lib/project-switch/status-store'
import {
  type AgentOptionsCacheEntry,
  ChatLaunchCancelledError,
  useAcpStore
} from '@/stores/acp-store'
import {
  agentWideValues,
  isScopedOption,
  readComposerOptions
} from '@/stores/acp-store/composer-options'
import { useChatStartPickStore } from '@/stores/chat-start-pick-store'
import { useProjectStore } from '@/stores/project-store'
import { useQuickChatStore } from '@/stores/quick-chat-store'
import { useWorkspaceStore } from '@/stores/workspace-store'
import { chatNotStarted } from './chat-not-started'
import { rememberNewChatAgent } from './new-chat-agent'

const SOURCE = 'chat.openNewChat'

/** Why a new chat was asked for (logged with every start). */
export type NewChatReason =
  | 'rail'
  | 'project-row'
  | 'shortcut'
  | 'menu'
  | 'new-tab-menu'
  | 'command-palette'
  | 'stopped-chat'
  | 'github-handoff'
  | 'worktree-failed'
  | 'quick-chats-header'
  | 'command-palette-quick'
  | 'mobile-drawer-quick'
  | 'onboarding'

export interface OpenNewChatRequest {
  /** The pane the launcher opens in; absent means the focused pane. */
  paneId?: string | null
  /** The project the chat belongs to; absent means the active project. */
  projectId?: string | null
  reason: NewChatReason
}

export type OpenNewChatResult =
  /** The launcher opened (or got focus) in the project's pane or the quick chat view. */
  { kind: 'launcher' } | { kind: 'skipped' }

function log(level: 'info' | 'warn' | 'error', message: string): void {
  void logFrontendError({ level, source: SOURCE, message })
}

/**
 * A chat that has not started: no messages, nothing queued for the agent, no
 * turn, still usable. A message sent before the agent is ready waits in the
 * queue, so a chat with a queued message is not empty.
 */
export function isEmptyStartChat(sessionId: SessionId): boolean {
  return chatNotStarted(useAcpStore.getState(), sessionId)
}

/**
 * True when `paneId` is a pane of the tree on screen and that tree is
 * `projectId`'s (not the previous project's tree during a slow switch). Logs
 * the skip otherwise.
 */
function paneStillTargets(paneId: string, projectId: string, reason: NewChatReason): boolean {
  if (committedPaneProject(paneId) === projectId) return true
  log('info', `New chat (reason=${reason}) skipped the launcher: its pane or project changed`)
  return false
}

/**
 * Forget an empty chat that will not be used: it has no messages, so nothing
 * is lost. A launch still in flight is called off (the deletion tombstone),
 * a live session is closed first.
 */
export async function discardEmptyChat(sessionId: SessionId): Promise<void> {
  useChatStartPickStore.getState().clearPick(sessionId)
  const store = useAcpStore.getState()
  try {
    if (!store.launchingSessionIds[sessionId]) await store.closeSession(sessionId)
    await useAcpStore.getState().deleteHistorySession(sessionId)
  } catch (err) {
    log(
      'warn',
      `Could not discard the empty chat ${sessionId}: ${err instanceof Error ? err.message : String(err)}`
    )
  }
}

/** The saved composer picks of an agent, checked against its cached options. */
async function savedPicks(configId: string): Promise<PendingLauncherOptions> {
  const saved = await readComposerOptions(configId).catch((err: unknown) => {
    log(
      'warn',
      `New chat on ${configId}: could not read the saved options: ${err instanceof Error ? err.message : String(err)}`
    )
    return null
  })
  if (!saved) return emptyPendingLauncherOptions()
  const cache = useAcpStore.getState().agentOptionsCache[configId]
  return restoredLauncherOptions({
    saved,
    pick: null,
    models: cache?.models,
    modes: cache?.modes,
    configOptions: cache?.configOptions ?? [],
    onDrop: (what) => log('warn', `New chat on ${configId}: dropped the saved ${what}`)
  })
}

/**
 * The options the new chat shows before its session starts: the agent's
 * cached options with the saved picks on top. The cache holds the options of
 * the agent's last session, which may run another model. Its scoped values
 * (effort, Fast) belong to that model, so for another picked model they are
 * dropped unless the saved record has that model's own value; the session
 * then shows the model's defaults (the launcher's rule, `launchPending`).
 */
export function newChatPlaceholderOptions(
  cache: Pick<AgentOptionsCacheEntry, 'models' | 'modes' | 'configOptions'> | undefined,
  picks: PendingLauncherOptions
): ReturnType<typeof overlayPendingLauncherOptions> {
  const cached = {
    models: cache?.models,
    modes: cache?.modes,
    configOptions: cache?.configOptions ?? []
  }
  const cachedModel = optionsToPending(cached).modelId
  const otherModel = picks.modelId != null && picks.modelId !== cachedModel
  const configOptions = otherModel
    ? cached.configOptions.filter(
        (option) => !isScopedOption(option) || picks.configValues[option.id] != null
      )
    : cached.configOptions
  return overlayPendingLauncherOptions({ ...cached, configOptions, pending: picks })
}

/**
 * A new quick chat: the agent launcher in the quick chat view, its composer
 * focused, with no session. Send starts the chat with its first message on
 * the picked agent (the same launch as a project New chat). The Quick chats
 * `+` and the palette call it directly; New chat calls it only when no
 * project exists. A launcher already on screen keeps its draft and gets focus.
 */
export function openNewQuickChat(reason: NewChatReason): OpenNewChatResult {
  if (useQuickChatStore.getState().view.mode === 'launcher') {
    log('info', `New chat (reason=${reason}) focused the quick chat launcher`)
  } else {
    showQuickChatLauncher(SOURCE)
    log('info', `New chat (reason=${reason}) opened the quick chat launcher`)
  }
  // A project launcher overlay stays hidden under the quick view; drop it.
  const workspace = useWorkspaceStore.getState()
  if (workspace.agentLauncherPaneId) workspace.hideAgentLauncher()
  focusQuickLauncherComposer()
  return { kind: 'launcher' }
}

/**
 * New chat for a project: the launcher in the pane, its composer at once. No
 * session starts here: the launcher warms one in the background and Send
 * claims it. A chat with no project is a quick chat (`openNewQuickChat`).
 */
export async function openNewChat(request: OpenNewChatRequest): Promise<OpenNewChatResult> {
  const { reason } = request
  const projectId =
    request.projectId !== undefined ? request.projectId : useProjectStore.getState().activeProjectId
  if (newChatKind(projectId ?? '') === 'quick') return openNewQuickChat(reason)
  if (!projectId) {
    log('warn', `New chat (reason=${reason}) skipped: no active project`)
    return { kind: 'skipped' }
  }
  // The launcher goes in the project tree. A quick chat on top of it would hide it.
  leaveQuickChatView(SOURCE)
  // A project row's New chat switches project first: its pane is in the tree
  // that the switch puts on screen.
  await whenProjectSwitchSettled(projectId)
  const paneId = request.paneId ?? useWorkspaceStore.getState().activePaneId
  if (!paneId) {
    log('warn', `New chat (reason=${reason}) skipped: no pane`)
    return { kind: 'skipped' }
  }
  if (!paneStillTargets(paneId, projectId, reason)) return { kind: 'skipped' }
  useWorkspaceStore.getState().showAgentLauncher(paneId)
  // The launcher of a pane with no tabs is already mounted, and an overlay
  // mounts read-only until its agent resolves: its own autofocus misses both.
  focusComposerWhenReady(
    `[data-pane-content="${paneId}"] [aria-label="Agent prompt"][contenteditable="true"]`
  )
  log('info', `New chat (reason=${reason}) opened the launcher in pane ${paneId} of ${projectId}`)
  return { kind: 'launcher' }
}

/**
 * Move an empty chat to another agent: a new empty chat on `configId` with
 * the picked model takes the place of the old one (same tab, or the quick
 * chat view), and the old chat is discarded. An empty chat has nothing to
 * hand off, so no switch is armed and no switch record is written.
 */
export async function replaceEmptyChat(
  sessionId: SessionId,
  configId: string,
  pick: SwitchModelPick
): Promise<void> {
  const old = useAcpStore.getState().sessions[sessionId]
  if (!old) return
  const saved = await savedPicks(configId)
  const model = await pendingForModelPick(configId, pick)
  // The awaits above can outlive the empty chat: a message sent meanwhile
  // starts it, so it is no longer ours to replace.
  if (!isEmptyStartChat(sessionId)) {
    log('info', `Empty chat ${sessionId} started before it moved to ${configId}: kept it`)
    return
  }
  const store = useAcpStore.getState()
  const cache = store.agentOptionsCache[configId]
  const picks: PendingLauncherOptions = {
    modelId: model.modelId,
    modeId: saved.modeId,
    configValues: {
      ...agentWideValues(saved.configValues, cache?.configOptions ?? []),
      ...model.configValues
    }
  }
  const shown = newChatPlaceholderOptions(cache, picks)
  const quick = old.kind === 'quick'
  // A quick chat's folder belongs to the host (`startQuickChat`).
  const cwd = quick ? '' : old.cwd
  const projectId = quick ? UNASSIGNED_PROJECT_ID : old.projectId
  const placeholderId = store.createLaunchPlaceholder({
    configId,
    cwd,
    projectId,
    models: shown.models,
    modes: shown.modes,
    configOptions: shown.configOptions,
    worktreePath: old.worktreePath,
    worktreeBranch: old.worktreeBranch,
    ...(quick ? { kind: 'quick' as const } : {})
  })
  if (quick) adoptQuickChat(sessionId, placeholderId, SOURCE)
  else useWorkspaceStore.getState().remapAgentChatSession(sessionId, placeholderId)
  rememberNewChatAgent(configId)
  void discardEmptyChat(sessionId)
  log('info', `Empty chat ${sessionId} moved to ${configId} as ${placeholderId}`)
  try {
    const started = await useAcpStore.getState().finalizeChatLaunch({
      placeholderId,
      configId,
      cwd,
      projectId,
      pending: hasPendingLauncherOptions(picks) ? picks : null,
      initialText: null,
      initialBlocks: null,
      adoptSession: (from, to) => {
        if (quick) adoptQuickChat(from, to, SOURCE)
        else useWorkspaceStore.getState().remapAgentChatSession(from, to)
      },
      worktreePath: old.worktreePath,
      worktreeBranch: old.worktreeBranch
    })
    log('info', `Chat ${placeholderId} on ${configId} started as ${started}`)
  } catch (err) {
    if (err instanceof ChatLaunchCancelledError) return
    // The chat shows the start error and Retry.
    log(
      'warn',
      `Chat ${placeholderId} on ${configId} did not start: ${err instanceof Error ? err.message : String(err)}`
    )
  }
}
