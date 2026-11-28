import { useBrowserControlStore } from '../../../../src/renderer/stores/browser-control-store'
import type { Scenario, StoreRecorder, VectorEvent } from '../record'

const ev = (action: string, ...args: unknown[]): VectorEvent => ({ action, args })

export const browserControlScenarios: Scenario[] = [
  {
    name: 'track_stop_and_forget_sessions',
    steps: [
      ev('trackAgentTab', 'tab-1', 'sess-1'),
      ev('trackAgentTab', 'tab-1', 'sess-1'),
      ev('trackAgentTab', 'tab-2', 'sess-1'),
      ev('trackAgentTab', 'tab-3', 'sess-2'),
      ev('setStopped', 'sess-1', true),
      ev('setStopped', 'sess-1', true),
      ev('untrackAgentTab', 'tab-1'),
      ev('untrackAgentTab', 'tab-missing'),
      ev('setStopped', 'sess-1', false),
      ev('setStopped', 'sess-2', true),
      ev('forgetSession', 'sess-1'),
      ev('forgetSession', 'sess-2')
    ]
  }
]

export const browserControlRecorder: StoreRecorder = {
  store: 'browser-control',
  sources: ['src/renderer/stores/browser-control-store.ts'],
  reset() {
    useBrowserControlStore.setState({ tabSessions: {}, stoppedSessions: {} })
  },
  seed(initial) {
    useBrowserControlStore.setState(structuredClone(initial))
  },
  snapshot() {
    const { tabSessions, stoppedSessions } = useBrowserControlStore.getState()
    return { tabSessions, stoppedSessions }
  },
  dispatch({ action, args }) {
    const store = useBrowserControlStore.getState()
    switch (action) {
      case 'trackAgentTab':
        store.trackAgentTab(args[0] as string, args[1] as string)
        return undefined
      case 'untrackAgentTab':
        store.untrackAgentTab(args[0] as string)
        return undefined
      case 'setStopped':
        store.setStopped(args[0] as string, args[1] as boolean)
        return undefined
      case 'forgetSession':
        store.forgetSession(args[0] as string)
        return undefined
      default:
        throw new Error(`browser-control store has no action ${action}`)
    }
  }
}
