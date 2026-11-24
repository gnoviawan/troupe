import { rememberNewChatAgent } from '@/components/chat/new-chat/new-chat-agent'
import { openNewChat, openNewQuickChat } from '@/components/chat/new-chat/open-new-chat'
import { writeAppSettingsNow } from '@/hooks/use-app-settings'
import { saveSupportedAgentConfig } from '@/lib/agents/agent-setup'
import type { SupportedAcpAgentEntry } from '@/lib/agents/supported-acp-agents'
import { logFrontendError } from '@/lib/log-api'

export interface OnboardingChoice {
  /** null: "Set up later" at the agents step. */
  agent: SupportedAcpAgentEntry | null
  /** null: "Skip" at the project step. */
  projectId: string | null
}

export type OnboardingLanding =
  /** The new-chat launcher in the project's empty pane. */
  | { kind: 'launcher'; projectId: string }
  /** A quick chat on the chosen agent. */
  | { kind: 'quick-chat' }
  /** Nothing to start: the empty workspace shows its own actions. */
  | { kind: 'workspace' }

export function onboardingLanding(choice: OnboardingChoice): OnboardingLanding {
  if (choice.projectId) return { kind: 'launcher', projectId: choice.projectId }
  return choice.agent ? { kind: 'quick-chat' } : { kind: 'workspace' }
}

function log(level: 'info' | 'warn', message: string): void {
  void logFrontendError({ level, source: 'onboarding', message })
}

/**
 * Saves and remembers the chosen agent before anything can mount the launcher
 * (it reads the remembered agent once), records completion with an immediate
 * write, leaves onboarding, then opens the landing. A failed save or write is
 * logged and does not trap the user: the gate stays closed for this session,
 * and a failed completion write shows first run again on the next launch.
 */
export async function landOnboarding(choice: OnboardingChoice, close: () => void): Promise<void> {
  if (choice.agent) {
    try {
      rememberNewChatAgent(await saveSupportedAgentConfig(choice.agent))
    } catch (err) {
      log('warn', `Saving the chosen agent ${choice.agent.id} failed: ${String(err)}`)
    }
  }
  const written = await writeAppSettingsNow({ onboardingCompletedAt: new Date().toISOString() })
  if (!written.success) log('warn', `Recording completion failed (${written.code})`)
  close()
  const landing = onboardingLanding(choice)
  log('info', `Completed: landing=${landing.kind} agent=${choice.agent?.id ?? 'none'}`)
  if (landing.kind === 'launcher') {
    await openNewChat({ projectId: landing.projectId, reason: 'onboarding' })
  } else if (landing.kind === 'quick-chat') {
    openNewQuickChat('onboarding')
  }
}
