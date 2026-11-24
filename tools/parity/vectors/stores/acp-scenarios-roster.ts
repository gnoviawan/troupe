// The ACP store's roster watch and agent browser tab scenarios for `acp.ts`.
// Each restates a case of `agent-roster-watch.test.ts` or
// `agent-browser-pane.test.ts`.
import type { Scenario } from '../record'
import { AGENT, ev, S, seeded, watermark } from './acp-fixtures'

const capabilities = { loadSession: true }

const delta = (event: unknown, payload?: unknown) =>
  ev('agentsWatchDelta', payload === undefined ? { event } : { event, payload })

const roster = (...summaries: unknown[]) => ev('agentsWatchState', summaries)

const agent = (id: string) => ({ id, capabilities: {}, authMethods: [] })

/** Agents `kept` (no capabilities yet) and `gone` connected, `dead` errored. */
const held = {
  agents: { kept: { id: 'kept', authMethods: [] }, gone: agent('gone'), dead: agent('dead') },
  agentStatus: { kept: 'connected', gone: 'connected', dead: 'error' }
}

const OPEN = { action: 'open', tabId: 'agent-tab-1', sessionId: S, url: 'https://example.com/a' }

export const rosterScenarios: Scenario[] = [
  {
    name: 'agents_watch_routes_each_roster_delta_to_its_reducer',
    initial: seeded(),
    steps: [
      delta('agent_spawned', { agentId: AGENT, capabilities }),
      delta('agent_crashed', { agentId: AGENT, sessionId: S, message: 'died' }),
      delta('agent_error', { agentId: AGENT, message: 'oops' }),
      delta('agent_disconnected', { agentId: AGENT, reason: 'killed' })
    ]
  },
  {
    name: 'agents_watch_toasts_a_default_for_a_failure_without_text',
    initial: { agents: { [AGENT]: agent(AGENT) }, agentStatus: { [AGENT]: 'connected' } },
    steps: [
      delta('agent_error', { agentId: AGENT, message: '' }),
      delta('agent_crashed', { agentId: AGENT, message: '' })
    ]
  },
  {
    name: 'agents_watch_routes_browser_open_and_sign_in_deltas',
    initial: { agents: { [AGENT]: agent(AGENT) }, agentStatus: { [AGENT]: 'connected' } },
    steps: [
      delta('browser_open_request', { agentId: AGENT, url: 'https://auth.example/x' }),
      delta('agent_sign_in_request', {
        agentId: AGENT,
        requestId: 'signin-1',
        url: 'https://auth.example/device',
        message: 'Enter this code: ABCD-1234'
      }),
      delta('agent_sign_in_resolved', { agentId: AGENT, requestId: 'signin-1' })
    ]
  },
  {
    name: 'agents_watch_hands_policy_and_automation_hints_on',
    steps: [
      delta('browser_policy_changed', { scope: 'project', projectId: 'p1', revision: 3 }),
      delta('automation_changed', { project_id: 'p1', automation_id: 'auto-1' }),
      delta('automation_resync', { missed: 3 }),
      delta('automation_resync')
    ]
  },
  {
    name: 'agents_watch_reconciles_the_full_state',
    initial: held,
    steps: [
      roster(
        { id: 'kept', name: 'Kept', capabilities },
        { id: 'new', name: 'New', capabilities },
        { name: 'No id', capabilities }
      ),
      roster()
    ]
  },
  {
    name: 'agents_watch_resync_without_a_list_only_asks_for_rereads',
    initial: held,
    steps: [ev('agentsWatchState', null)]
  },
  {
    name: 'agents_watch_ignores_a_delta_without_an_agent_id_or_of_an_unknown_kind',
    steps: [
      delta('agent_spawned', {}),
      ev('agentsWatchDelta', null),
      delta('agent_spawned', { agentId: 7 }),
      delta('agent_teleported', { agentId: 'a1' })
    ]
  },
  {
    name: 'agent_browser_tab_on_the_web_only_tracks_its_stop_owner',
    initial: seeded(),
    steps: [
      ev('_onBrowserAgentTab', OPEN),
      ev('_onBrowserAgentTab', { action: 'open', tabId: 'agent-tab-2' }),
      ev('_onBrowserAgentTab', { action: 'open', tabId: '', sessionId: S }),
      ev('_onBrowserAgentTab', { action: 'close', tabId: 'agent-tab-1', sessionId: S }),
      ev('_onBrowserTabClosed', { tabId: 'agent-tab-2' }),
      ev('_onBrowserTabClosed', { tabId: '' })
    ]
  },
  {
    name: 'agent_browser_tab_on_the_desktop_opens_and_closes_the_tab',
    context: { desktop: true },
    initial: seeded(),
    steps: [
      ev('_onBrowserAgentTab', OPEN),
      ev('_onBrowserAgentTab', {
        action: 'open',
        tabId: 'agent-tab-2',
        sessionId: S,
        url: 'javascript:alert(1)'
      }),
      ev('_onBrowserAgentTab', { action: 'open', tabId: 'agent-tab-3', url: 'HTTP://x.test' }),
      ev('_onBrowserAgentTab', { action: 'close', tabId: 'agent-tab-1', sessionId: S }),
      ev('_onBrowserAgentTab', { action: 'focus', tabId: 'agent-tab-2', sessionId: S }),
      ev('_onBrowserTabClosed', { tabId: 'agent-tab-2' })
    ]
  },
  {
    name: 'agent_browser_tab_open_the_history_covers_is_skipped',
    context: { desktop: true },
    initial: seeded(),
    steps: [
      watermark(40),
      ev('_onBrowserAgentTab', OPEN, 12),
      ev('_onBrowserAgentTab', OPEN, 41),
      ev('noteLiveTurnReplayFloor', S, 30),
      ev('_onBrowserAgentTab', OPEN, 35),
      ev('_onBrowserAgentTab', OPEN, 25),
      ev('_onBrowserAgentTab', { ...OPEN, action: 'close' }, 12)
    ]
  }
]
