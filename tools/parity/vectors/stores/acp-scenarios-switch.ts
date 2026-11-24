// The ACP store's agent-switch marker scenarios for `acp.ts`:
// `_onAgentSwitch`, `cancelAgentSwitch` and the continuation edges
// (`slices/switch.ts`, `stampSwitchContinuation`). Each restates a case of
// `agent-switch.test.ts`.
import type { Scenario } from '../record'
import { AGENT, ev, idle, S, seeded } from './acp-fixtures'

const switched = (extra: Record<string, unknown> = {}, seq?: number) =>
  ev(
    '_onAgentSwitch',
    {
      agentId: AGENT,
      sessionId: S,
      fromConfigId: 'claude',
      toConfigId: 'codex',
      newSessionId: 's2',
      summaryText: 'Handoff summary',
      ...extra
    },
    ...(seq === undefined ? [] : [seq])
  )
const index = (s2: Record<string, unknown> = {}) => ({
  sessionIndex: [
    { id: S, projectId: 'p1' },
    { id: 's2', projectId: 'p1', ...s2 }
  ]
})
const installed = {
  id: 'switch:seq-3',
  fromConfigId: 'claude',
  toConfigId: 'codex',
  newSessionId: 's2',
  summaryText: 'Handoff summary',
  timestamp: 1,
  seq: 3
}

export const switchScenarios: Scenario[] = [
  {
    name: 'agent_switch_adds_a_marker_and_stamps_the_continuation',
    initial: seeded(idle, index()),
    steps: [switched({}, 7)]
  },
  {
    name: 'agent_switch_without_a_seq_takes_the_next_one',
    initial: seeded(idle, index()),
    steps: [switched()]
  },
  {
    name: 'agent_switch_updates_the_installed_marker_with_the_same_content',
    initial: seeded(idle, { ...index(), agentSwitches: { [S]: [installed] } }),
    steps: [switched({ fromConfigId: 'gemini' }, 9)]
  },
  {
    name: 'agent_switch_updates_the_marker_with_the_same_id',
    initial: seeded(idle, { ...index(), agentSwitches: { [S]: [installed] } }),
    steps: [switched({ summaryText: 'Newer summary' }, 3)]
  },
  {
    name: 'agent_switch_with_another_summary_adds_a_marker',
    initial: seeded(idle, { ...index(), agentSwitches: { [S]: [installed] } }),
    steps: [switched({ summaryText: 'Second handoff' }, 9)]
  },
  {
    name: 'cancel_agent_switch_without_an_armed_switch_changes_nothing',
    initial: seeded(idle),
    steps: [ev('cancelAgentSwitch', S)]
  },
  {
    name: 'agent_switch_on_a_closed_chat_is_dropped',
    initial: seeded({ ...idle, status: 'closed' }, index()),
    steps: [switched({}, 7)]
  },
  {
    name: 'agent_switch_covered_by_history_is_dropped',
    initial: seeded(idle, index()),
    steps: [
      ev('noteHistoryWatermark', S, { metadata: { id: S, lastSeq: 8 }, messages: [] }),
      switched({}, 7)
    ]
  },
  {
    name: 'agent_switch_keeps_an_existing_continuation',
    initial: seeded(idle, index({ continuedFrom: 's0' })),
    steps: [switched({}, 7)]
  },
  {
    name: 'agent_switch_never_stamps_across_projects',
    initial: seeded(idle, index({ projectId: 'p2' })),
    steps: [switched({}, 7)]
  },
  {
    name: 'cancel_agent_switch_clears_the_armed_switch',
    initial: seeded({ ...idle, switching: { toConfigId: 'codex', status: 'pending' } }),
    steps: [ev('cancelAgentSwitch', S), ev('cancelAgentSwitch', 'missing')]
  },
  {
    name: 'history_watermark_counts_switch_markers',
    initial: seeded(idle),
    steps: [
      ev('noteHistoryWatermark', S, {
        metadata: { id: S, lastSeq: 2 },
        messages: [],
        switches: [{ ...installed, seq: 12 }, { seq: 40 }]
      })
    ]
  }
]
