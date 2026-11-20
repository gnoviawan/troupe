import {
  type MutableRefObject,
  type ReactNode,
  type RefObject,
  useEffect,
  useLayoutEffect,
  useRef,
  useState
} from 'react'
import { ChevronLeft } from '@/components/icons'
import { Button } from '@/components/ui/button'
import { logFrontendError } from '@/lib/log-api'
import { cn } from '@/lib/utils'
import { type ProviderSetup, useProviderSetup } from './provider-setup'
import { ROW, TabGlyph } from './SelectorRows'
import { Busy, Note, SignInActions } from './SignInActions'
import type { AgentTab } from './selector-model'
import { pickerRowHeight } from './selector-tokens'
import type { SelectorPanelState } from './use-selector-panel-state'

const LIST_ROW = '[data-selector-row]'
const LOG_SOURCE = 'composer.selector.setup'

export interface PanelSetupRoute {
  /** The agent whose setup the list shows, or null. */
  tab: AgentTab | null
  /** Show a "Needs setup" agent's setup in the list, one level into "More". */
  open: (tab: AgentTab) => void
  /** Back to the "More" list, focus on the agent's row. */
  back: () => void
  /** Its models are known: show them. */
  ready: () => void
  /** Drop the setup view for another view (a tab, or search). */
  leave: () => void
}

/**
 * Which "Needs setup" agent the desktop panel is setting up (issue #1949).
 * It sits under "More", like the phone sheet's setup screen sits under
 * Providers: Escape and Back return to that list, a tab or a search leaves it.
 */
export function usePanelAgentSetup(
  state: Pick<SelectorPanelState, 'searching' | 'setQuery' | 'showMore' | 'openAgent' | 'tabOf'>,
  escapeRef: MutableRefObject<(() => boolean) | null>,
  refs: { list: RefObject<HTMLElement | null>; search: RefObject<HTMLElement | null> }
): PanelSetupRoute {
  const [configId, setConfigId] = useState<string | null>(null)
  const tab = configId && !state.searching ? state.tabOf(configId) : null
  /** Where focus goes once the view closes: an agent row's id, or '' for the list. */
  const returnTo = useRef<string | null>(null)

  const open = (next: AgentTab): void => {
    void logFrontendError({
      level: 'info',
      source: LOG_SOURCE,
      message: `Opened the setup of ${next.configId} in the model picker`
    })
    state.setQuery('')
    state.showMore()
    setConfigId(next.configId)
  }
  const leave = (): void => setConfigId(null)
  const back = (): void => {
    if (configId === null) return
    returnTo.current = configId
    setConfigId(null)
  }
  const ready = (): void => {
    if (!tab) return
    void logFrontendError({
      level: 'info',
      source: LOG_SOURCE,
      message: `Set up ${tab.configId} from the model picker; showing its models`
    })
    returnTo.current = ''
    setConfigId(null)
    state.openAgent(tab)
  }

  // The selector state sets its own Escape each render; the setup view steps back first.
  const stateEscape = escapeRef.current
  escapeRef.current = () => {
    if (tab) {
      back()
      return true
    }
    return stateEscape?.() ?? false
  }

  // The view left with focus inside it: before paint, so focus never stays on body.
  useLayoutEffect(() => {
    const target = returnTo.current
    if (configId !== null || target === null) return
    returnTo.current = null
    const active = document.activeElement
    if (active && active !== document.body) return
    const rows = Array.from(refs.list.current?.querySelectorAll<HTMLElement>(LIST_ROW) ?? [])
    const row = target
      ? rows.find((el) => el.getAttribute('data-testid') === `selector-agent-${target}`)
      : rows.find((el) => !el.hasAttribute('disabled'))
    ;(row ?? refs.search.current)?.focus()
  }, [configId, refs])

  return { tab, open, back, ready, leave }
}

function Hero({
  tab,
  title,
  detail
}: {
  tab: AgentTab
  title: string
  detail?: string
}): React.JSX.Element {
  return (
    <div role="status" className="flex flex-col items-center gap-1 px-3 pt-1 text-center">
      <span className="mb-1 flex size-10 items-center justify-center rounded-xl bg-foreground/[0.06]">
        <TabGlyph tab={tab} />
      </span>
      <h3 className="text-sm font-medium text-foreground">{title}</h3>
      {detail ? (
        <p className="line-clamp-4 break-words text-xs text-muted-foreground">{detail}</p>
      ) : null}
    </div>
  )
}

/**
 * A "Needs setup" agent set up in the desktop panel's list: save its config,
 * start it, and sign in when it asks (`useProviderSetup`, shared with the
 * phone sheet). Once its models load, the panel shows them.
 */
export function PanelAgentSetup({
  tab,
  workspace,
  disabled,
  touch,
  onBack,
  onReady
}: {
  tab: AgentTab
  workspace: { cwd: string; projectId: string }
  disabled: boolean
  touch: boolean
  onBack: () => void
  onReady: () => void
}): React.JSX.Element {
  const setup = useProviderSetup(tab, workspace)
  const ready = setup.phase.kind === 'ready'
  const left = useRef(false)
  useEffect(() => {
    if (!ready || left.current) return
    left.current = true
    onReady()
  }, [ready, onReady])
  return (
    <PanelSetupView tab={tab} setup={setup} disabled={disabled} touch={touch} onBack={onBack} />
  )
}

function PanelSetupView({
  tab,
  setup,
  disabled,
  touch,
  onBack
}: {
  tab: AgentTab
  setup: ProviderSetup
  disabled: boolean
  touch: boolean
  onBack: () => void
}): React.JSX.Element {
  const { phase } = setup
  const rootRef = useRef<HTMLDivElement>(null)
  const actionsRef = useRef<HTMLDivElement>(null)
  // The row that opened it, or the button a phase replaced, is gone: focus
  // the phase's first action (else Back), never body. Focus parked on Back
  // while a phase had no action moves on to the next phase's action, so
  // Enter signs in or retries instead of leaving.
  // biome-ignore lint/correctness/useExhaustiveDependencies: runs when the phase changes the buttons
  useLayoutEffect(() => {
    const active = document.activeElement
    const back = rootRef.current?.querySelector<HTMLElement>(LIST_ROW) ?? null
    const onBack = active !== null && active === back
    if (active && active !== document.body && !onBack) return
    const action = Array.from(
      actionsRef.current?.querySelectorAll<HTMLElement>('button, input') ?? []
    ).find((el) => !el.hasAttribute('disabled'))
    ;(action ?? back)?.focus()
  }, [phase.kind])

  const size = touch ? 'touch' : 'sm'
  let hero: ReactNode
  let actions: ReactNode
  switch (phase.kind) {
    case 'confirm':
      hero = (
        <Hero
          tab={tab}
          title={`Set up ${tab.name}`}
          detail="Add it to your agents, then pick a model."
        />
      )
      actions = (
        <>
          <Button
            type="button"
            size={size}
            className="w-full"
            disabled={disabled || !setup.config}
            onClick={setup.start}
          >
            {`Set up ${tab.name}`}
          </Button>
          {setup.config ? <Note>{`Runs with ${setup.config.command}.`}</Note> : null}
        </>
      )
      break
    case 'saving':
      hero = <Hero tab={tab} title={`Set up ${tab.name}`} />
      actions = <Busy label="Setting up…" touch={touch} />
      break
    case 'starting':
    case 'ready':
      hero = <Hero tab={tab} title={`Starting ${tab.name}`} detail="Reading its models." />
      actions = <Busy label="Starting…" touch={touch} />
      break
    case 'sign-in':
      hero = <Hero tab={tab} title={`Sign in to ${tab.name}`} detail={phase.error.detail} />
      actions = (
        <SignInActions
          tab={tab}
          signIn={setup.signIn}
          retry={setup.retry}
          disabled={disabled}
          touch={touch}
        />
      )
      break
    case 'failed':
      hero = <Hero tab={tab} title={phase.title} detail={phase.detail} />
      actions = (
        <Button
          type="button"
          size={size}
          className="w-full"
          disabled={disabled}
          onClick={setup.retry}
        >
          Try again
        </Button>
      )
      break
  }
  return (
    <div
      ref={rootRef}
      data-testid="panel-agent-setup"
      data-phase={phase.kind}
      className="flex flex-col gap-2 pb-2"
    >
      <button
        type="button"
        data-selector-row=""
        onClick={onBack}
        className={cn(ROW, pickerRowHeight(touch), 'text-muted-foreground')}
      >
        <ChevronLeft size={14} aria-hidden="true" className="shrink-0" />
        <span className="fade-end min-w-0 flex-1">Back to agents</span>
      </button>
      {hero}
      <div ref={actionsRef} className="flex flex-col gap-2 px-2">
        {actions}
      </div>
    </div>
  )
}
