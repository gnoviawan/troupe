/** Short constructors for writing surface steps. */

import type { Action, Check, Step, Target } from '../types'

export const id = (testId: string): Target => ({ testId })
export const txt = (text: string, exact = true): Target => ({ text, exact })
export const label = (name: string, role?: Extract<Target, { label: string }>['role']): Target => ({
  label: name,
  role
})
export const css = (selector: string): Target => ({ css: selector })

export const click = (target: Target): Action => ({ do: 'click', target })
export const rclick = (target: Target): Action => ({ do: 'click', target, button: 'right' })
export const dblclick = (target: Target): Action => ({ do: 'dblclick', target })
export const hover = (target: Target): Action => ({ do: 'hover', target })
export const press = (keys: string): Action => ({ do: 'press', keys })
export const type = (text: string): Action => ({ do: 'type', text })
export const wait = (ms: number): Action => ({ do: 'wait', ms })
export const drag = (from: Target, to: Target): Action => ({ do: 'drag', from, to })
export const wheel = (target: Target, deltaY: number): Action => ({ do: 'wheel', target, deltaY })
export const choose = (target: Target, file: string): Action => ({ do: 'choose', target, file })

export const see = (...texts: string[]): Check[] => texts.map((text) => ({ text }))
export const gone = (text: string): Check => ({ text, gone: true })

export const step = (name: string, actions: Action[], expect: Check[] = []): Step => ({
  name,
  actions,
  expect
})

/** The launcher has picked the scripted agent: its first paint can show another installed agent. */
export const AGENT_PICKED: Check = {
  text: 'Chat Scenario Agent',
  within: 'agent-model-selector-trigger'
}

/** The dashboard as it boots, with checks that the seeded state is there. */
export const DASHBOARD = step(
  'a-dashboard',
  [],
  [...see('What should we do in alpha?'), AGENT_PICKED]
)

/** The seeded chat's title, as its card shows it. */
export const CHAT_TITLE = 'How does the web client decide which routes rend'
export const openChat = click(txt(CHAT_TITLE, false))

/** The seeded chat is open and its transcript rests at its end (the scroller's button is gone). */
export const CHAT_SETTLED: Check[] = [
  ...see('Where else is the chat route parsed?'),
  { text: 'Scroll to latest', gone: true, timeoutMs: 15_000 }
]
