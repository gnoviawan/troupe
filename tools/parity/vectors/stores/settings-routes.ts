import { WsAcpTransport } from '../../../../src/renderer/lib/acp-transport/ws-transport'
import type { Scenario, StoreRecorder, VectorEvent } from '../record'
import { type Api, routesRecorder } from './git-routes'

/**
 * The HTTP request the web transport sends for each settings call: method, path, JSON body
 * and whether it carries `X-Command-Id`. `tests/settings/mod.rs` checks `Request::route`.
 */
export function settingsRoutesRecorder(): StoreRecorder {
  const transport = new WsAcpTransport({ url: 'ws://capture.invalid/ws' })
  const api: Api = {
    getAgentIdleWindow: () => transport.getAgentIdleWindow(),
    setAgentIdleWindow: (secs) => transport.setAgentIdleWindow(secs as number | null)
  }
  return routesRecorder(
    'settings-routes',
    ['src/renderer/lib/acp-transport/ws-transport.ts', 'src/renderer/lib/ipc/http.ts'],
    api
  )
}

const ev = (action: string, ...args: unknown[]): VectorEvent => ({ action, args })

export const settingsRoutesScenarios: Scenario[] = [
  {
    name: 'idle_window',
    steps: [
      ev('getAgentIdleWindow'),
      ev('setAgentIdleWindow', 3600),
      ev('setAgentIdleWindow', null)
    ]
  }
]
