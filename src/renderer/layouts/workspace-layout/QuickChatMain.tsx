import { AnimatePresence, motion, useReducedMotion } from 'framer-motion'
import { AgentLauncher } from '@/components/agents/AgentLauncher'
import { launcherExitTransition } from '@/components/agents/launcher/launcher-motion'
import { QUICK_LAUNCHER_TARGET } from '@/components/agents/launcher/launcher-target'
import { AgentChatPanel } from '@/components/chat/AgentChatPanel'
import { type ClosedQuickChatView, quickChatPanelKey } from '@/stores/quick-chat-store'

/**
 * The quick chat view in the main area: a quick (or older) chat, or the agent
 * launcher of a new quick chat. Send in the launcher opens the chat here and
 * the launcher plays its exit over it, as in an empty pane.
 */
export function QuickChatMain({ view }: { view: ClosedQuickChatView }): React.JSX.Element {
  const reducedMotion = useReducedMotion() ?? false
  return (
    <div className="relative flex h-full min-h-0 flex-1 flex-col" data-testid="quick-chat-main">
      {/* Another chat remounts the panel, so no draft or panel state carries
          over. A placeholder that became its session keeps it. */}
      {view.mode === 'chat' ? (
        <AgentChatPanel key={quickChatPanelKey(view)} sessionId={view.sessionId} />
      ) : null}
      {/* The wrapper stays pointer-events-none (the launcher root re-enables
          hits while present), so the new chat is interactive mid-exit. */}
      <AnimatePresence initial={false}>
        {view.mode === 'launcher' ? (
          <motion.div
            key="quick-chat-launcher"
            className="pointer-events-none absolute inset-0"
            initial={false}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={launcherExitTransition(reducedMotion)}
          >
            <AgentLauncher target={QUICK_LAUNCHER_TARGET} />
          </motion.div>
        ) : null}
      </AnimatePresence>
    </div>
  )
}
