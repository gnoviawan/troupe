import { motion, type Transition, useReducedMotion } from 'framer-motion'
import { Bug, FileText, ListChecks, Sparkles } from '@/components/icons'
import { TermulMark } from '@/components/TermulMark'
import { cn } from '@/lib/utils'
import { CHAT_SPRING } from './chat-motion'

/**
 * The chat start screen ("Empty chat · V1 Composer in the middle"), shared by
 * the agent launcher (before a session exists) and an empty chat (a session
 * with no messages yet): the Troupe mark, one project question, the composer
 * in the middle (each surface keeps its own composer), and quiet starter
 * chips under it. One source of truth for this copy, these starters, and
 * this spacing.
 */

export interface ChatStarter {
  icon: React.ComponentType<{ className?: string }>
  label: string
  /** Text put in the composer. A pick never sends. */
  prompt: string
}

export const CHAT_STARTERS: ChatStarter[] = [
  {
    icon: Sparkles,
    label: 'Explain this project',
    prompt: 'Give me a high-level overview of this codebase and how it is structured.'
  },
  {
    icon: Bug,
    label: 'Find a bug',
    prompt: 'Look for potential bugs or edge cases in the code I currently have open.'
  },
  {
    icon: ListChecks,
    label: 'Write tests',
    prompt: 'Write unit tests for the file I am currently working on.'
  },
  {
    icon: FileText,
    label: 'Summarize changes',
    prompt: 'Summarize my recent uncommitted git changes.'
  }
]

/** Starters for a chat with no project. They do not mention a folder or a checkout. */
export const QUICK_CHAT_STARTERS: ChatStarter[] = [
  {
    icon: Sparkles,
    label: 'Ask a question',
    prompt: 'Help me think this through. Start by asking what I want to figure out.'
  },
  {
    icon: Bug,
    label: 'Think something through',
    prompt: 'Help me reason about a problem. Ask one clarifying question first.'
  },
  {
    icon: ListChecks,
    label: 'Make a list',
    prompt: 'Help me turn this into a short checklist.'
  },
  {
    icon: FileText,
    label: 'Draft a note',
    prompt: 'Help me draft a short note from the points I give you.'
  }
]

/**
 * The mark + project question. On the mobile web shell it shrinks so the
 * bottom-anchored composer stays above the fold; during a launcher exit it
 * dissolves upward.
 */
export function ChatStartHero({
  projectLabel,
  headingLevel = 1,
  isMobileShell = false,
  isExiting = false,
  reducedMotion = false,
  quick = false
}: {
  projectLabel: string
  /** 1 in the launcher (the pane's main heading); 2 inside a chat tab. */
  headingLevel?: 1 | 2
  isMobileShell?: boolean
  isExiting?: boolean
  reducedMotion?: boolean
  /** A chat with no project. The question does not name a folder. */
  quick?: boolean
}): React.JSX.Element {
  const Heading = headingLevel === 1 ? 'h1' : 'h2'
  return (
    <div
      className={cn(
        'mb-8 flex w-full flex-col items-center gap-4 text-center transition-[opacity,translate,filter] duration-200 ease-out motion-reduce:transition-none',
        isMobileShell && 'mb-4 gap-2',
        isExiting && !reducedMotion && '-translate-y-2 opacity-0 blur-[2px]'
      )}
    >
      <TermulMark size={isMobileShell ? 32 : 48} className="text-foreground" />
      <Heading
        className={cn(
          'break-words text-3xl font-medium tracking-tight text-foreground md:text-4xl',
          isMobileShell && 'text-xl'
        )}
      >
        {quick ? 'What should we do?' : `What should we do in ${projectLabel}?`}
      </Heading>
    </div>
  )
}

/**
 * Starter chips under the composer: text with an icon, no card. A pick fills
 * the composer and does not send, so the user can edit first. They enter with
 * a 40ms stagger and fade out with the launcher's exit.
 */
export function ChatStarters({
  onPick,
  isExiting = false,
  className,
  starters = CHAT_STARTERS
}: {
  onPick: (prompt: string) => void
  isExiting?: boolean
  className?: string
  starters?: readonly ChatStarter[]
}): React.JSX.Element {
  const reduced = useReducedMotion() ?? false
  const enter = (i: number): Transition =>
    reduced ? { duration: 0 } : { ...CHAT_SPRING, delay: 0.04 * i }

  return (
    // biome-ignore lint/a11y/useSemanticElements: a fieldset here would carry form semantics for plain shortcuts
    <div
      role="group"
      aria-label="Starters"
      className={cn(
        'flex flex-wrap items-center justify-center gap-1 transition-opacity duration-150 ease-out motion-reduce:transition-none',
        isExiting && 'opacity-0',
        className
      )}
    >
      {starters.map((starter, i) => (
        <motion.button
          key={starter.label}
          type="button"
          onClick={() => onPick(starter.prompt)}
          initial={reduced ? false : { opacity: 0, y: 4 }}
          animate={{ opacity: 1, y: 0 }}
          transition={enter(i)}
          data-press-feedback="off"
          className="inline-flex h-8 items-center gap-1.5 rounded-full px-3 text-sm text-muted-foreground transition-[background-color,color,transform] duration-150 ease-out hover:bg-foreground/10 hover:text-foreground focus-visible:bg-foreground/10 focus-visible:text-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-focus-ring active:scale-[0.96] motion-reduce:active:scale-100"
        >
          <starter.icon className="size-3.5 shrink-0" />
          {starter.label}
        </motion.button>
      ))}
    </div>
  )
}
