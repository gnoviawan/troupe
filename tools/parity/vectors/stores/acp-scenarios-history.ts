// The ACP store's history-open scenarios for `acp.ts`: `openHistorySession`
// (`slices/session.ts`) on the desktop path, without the host's owner
// listing. Each restates a case of `history-session.test.ts`,
// `dead-turn-reopen.test.ts`, `turn-error-reopen.test.ts` or
// `stale-cache-reopen.test.ts`. A step that ends a reopen is followed by a
// `tick` step: the replay window closes on a timer the step does not wait for.
import type { Scenario } from '../record'
import { AGENT, ev, message } from './acp-fixtures'

const NUL = '\u0000'
const GEMINI = { id: 'gemini', name: 'Gemini', command: 'gemini-acp' }
const tick = ev('clearPendingSignIn', 'none')
const open = (...args: unknown[]) => ev('openHistorySession', 's1', ...args)
const REQUIRED = { reject: 'ACP_AUTH_REQUIRED: sign in first' }

const user = (id: string, text: string, seq: number) => message(id, 'user', text, { seq })
const agent = (id: string, text: string, seq: number) => message(id, 'agent', text, { seq })

function payload(
  meta: Record<string, unknown> = {},
  extra: Record<string, unknown> = {}
): Record<string, unknown> {
  return {
    metadata: {
      id: 's1',
      agentId: AGENT,
      agentConfigId: 'gemini',
      cwd: '/work',
      projectId: 'p1',
      title: 'Fix the build',
      status: 'closed',
      createdAt: 5,
      lastSeq: 4,
      ...meta
    },
    messages: [user('turn:t1', 'hi', 1), agent('m2', 'hello', 2)],
    toolCalls: [{ toolCallId: 'tc1', title: 'Read', status: 'completed', seq: 3 }],
    ...extra
  }
}

/** `AGENT` connected for `gemini` in `/work`, with `capabilities`. */
function live(capabilities: unknown, extra: Record<string, unknown> = {}) {
  return {
    agentConfigs: [GEMINI],
    agents: { [AGENT]: { id: AGENT, capabilities, authMethods: [] } },
    agentStatus: { [AGENT]: 'connected' },
    configToLiveAgent: { [`gemini${NUL}/work${NUL}project${NUL}p1`]: AGENT },
    sessionIndex: [{ id: 's1', title: 'Fix the build' }],
    ...extra
  }
}

const LOADS = { loadSession: true }
const RESUMES = { sessionCapabilities: { resume: {} } }
const PROJECTS = [{ id: 'p1', name: 'Work', path: '/work' }]

export const historyScenarios: Scenario[] = [
  {
    name: 'history_open_loads_on_an_agent_that_can_load',
    context: { payloads: { s1: payload() }, agentReplies: [{}], projects: PROJECTS },
    initial: live(LOADS),
    steps: [open(), tick]
  },
  {
    name: 'history_open_resumes_and_takes_the_agent_controls',
    context: {
      payloads: { s1: payload() },
      agentReplies: [{ modes: { currentModeId: 'ask', availableModes: [{ id: 'ask' }] } }],
      projects: PROJECTS
    },
    initial: live(RESUMES),
    steps: [open(), tick]
  },
  {
    name: 'history_open_stays_local_without_load_or_resume',
    context: { payloads: { s1: payload() }, projects: PROJECTS },
    initial: live({}),
    steps: [open()]
  },
  {
    name: 'history_open_of_a_live_chat_answers_at_once',
    initial: live(LOADS, {
      sessions: {
        s1: {
          id: 's1',
          agentId: AGENT,
          status: 'active',
          title: null,
          activeTurn: false,
          openTurnId: null,
          cwd: '/work',
          createdAt: 1
        }
      },
      messages: { s1: [] }
    }),
    steps: [open()]
  },
  {
    name: 'history_open_without_history_fails',
    initial: live(LOADS),
    steps: [open()]
  },
  {
    name: 'history_open_spawns_an_agent_for_the_chat',
    context: {
      payloads: { s1: payload() },
      projects: PROJECTS,
      agentReplies: [{ agentId: 'agent-9', capabilities: LOADS, authMethods: [] }, {}]
    },
    initial: { agentConfigs: [GEMINI], sessionIndex: [{ id: 's1' }] },
    steps: [open(), tick]
  },
  {
    name: 'history_open_load_failure_shows_the_resume_error',
    context: { payloads: { s1: payload() }, agentReplies: [{ reject: 'load blew up' }] },
    initial: live(LOADS),
    steps: [open()]
  },
  {
    name: 'history_open_resume_falls_back_to_load_on_a_missing_session',
    context: {
      payloads: { s1: payload() },
      agentReplies: [{ reject: 'Session not found: s1' }, {}]
    },
    initial: live({ loadSession: true, sessionCapabilities: { resume: {} } }),
    steps: [open(), tick]
  },
  {
    name: 'history_open_resume_failure_is_final_without_load',
    context: { payloads: { s1: payload() }, agentReplies: [{ reject: 'Session not found: s1' }] },
    initial: live(RESUMES),
    steps: [open()]
  },
  {
    name: 'history_open_attaches_to_a_turn_its_agent_still_runs',
    context: {
      payloads: {
        s1: payload(
          { status: 'active', turnActive: true, lastSeq: 6 },
          {
            messages: [user('turn:t1', 'hi', 1), agent('m2', 'hello', 2), user('turn:t2', 'go', 5)]
          }
        )
      }
    },
    initial: live(LOADS),
    steps: [open(), tick]
  },
  {
    name: 'history_open_clears_a_turn_no_agent_runs',
    context: {
      payloads: {
        s1: payload(
          { status: 'active', turnActive: true, agentId: 'agent-gone' },
          { messages: [user('turn:t1', 'hi', 1)] }
        )
      },
      agentReplies: [{}]
    },
    initial: live(LOADS),
    steps: [open(), tick]
  },
  {
    name: 'history_open_derives_an_open_turn_from_an_unanswered_prompt',
    context: {
      payloads: {
        s1: payload(
          { status: 'active' },
          { messages: [agent('m0', 'hey', 1), user('u2', 'go', 2)] }
        )
      }
    },
    initial: live(LOADS),
    steps: [open(), tick]
  },
  {
    name: 'history_open_attaches_when_the_host_says_the_turn_runs',
    context: {
      payloads: { s1: payload() },
      agentReplies: [{ reject: 'ACP_REOPEN_TURN_ACTIVE: a turn is running' }]
    },
    initial: live(LOADS),
    steps: [open(), tick]
  },
  {
    name: 'history_open_keeps_the_last_turn_error',
    context: {
      payloads: {
        s1: payload({
          lastTurnError: {
            message: 'quota',
            code: 429,
            providerCode: 'provider_quota_exceeded'
          }
        })
      },
      agentReplies: [{}]
    },
    initial: live(LOADS),
    steps: [open(), tick]
  },
  {
    name: 'history_open_shows_a_stop_reason_note',
    context: {
      payloads: { s1: payload({ lastTurnError: { stopReason: 'max_tokens' } }) }
    },
    initial: live({}),
    steps: [open()]
  },
  {
    name: 'history_open_restores_the_plan_fence_settled',
    context: {
      payloads: {
        s1: payload(
          {},
          {
            messages: [
              user('turn:t1', 'plan it', 1),
              agent(
                'm2',
                '```termul-plan\n[{"content":"a","status":"in_progress"},{"content":"b","status":"pending","priority":"high"},{"content":1}]\n```',
                2
              )
            ]
          }
        )
      }
    },
    initial: live({}),
    steps: [open()]
  },
  {
    name: 'history_open_logs_a_malformed_plan_fence',
    context: {
      payloads: {
        s1: payload(
          {},
          {
            messages: [user('turn:t1', 'plan it', 1), agent('m2', '```termul-plan\n{oops\n```', 2)]
          }
        )
      }
    },
    initial: live({}),
    steps: [open()]
  },
  {
    name: 'history_open_drops_hidden_turns_and_their_cards',
    context: {
      payloads: {
        s1: payload(
          {},
          {
            messages: [
              user('u0', '  ', 1),
              agent('m1', 'greeting', 2),
              user('turn:t1', 'real', 4),
              agent('m4', 'answer', 5)
            ],
            toolCalls: [
              { toolCallId: 'hidden', title: 'Greet', status: 'completed', seq: 3 },
              { toolCallId: 'shown', title: 'Work', status: 'completed', seq: 6 }
            ]
          }
        )
      }
    },
    initial: live({}),
    steps: [open()]
  },
  {
    name: 'history_open_takes_the_index_title_over_a_placeholder',
    context: {
      payloads: {
        s1: payload(
          { title: 'Untitled Chat' },
          {
            commands: [{ name: 'compact' }],
            commandsSeq: 7,
            modes: { currentModeId: 'ask', availableModes: [{ id: 'ask' }] }
          }
        )
      }
    },
    initial: live({}, { sessionIndex: [{ id: 's1', title: 'Named elsewhere' }] }),
    steps: [open()]
  },
  {
    name: 'history_open_shares_an_open_in_flight',
    context: {
      payloads: { s1: payload() },
      agentReplies: [{ hold: true, release: {} }]
    },
    initial: live(LOADS),
    steps: [open(), open(), ev('releaseHeld'), tick]
  },
  {
    name: 'history_open_stops_when_a_new_session_takes_the_chat',
    context: {
      payloads: { s1: payload() },
      agentReplies: [
        { hold: true, release: { modes: { currentModeId: 'x', availableModes: [] } } },
        { sessionId: 's1' }
      ]
    },
    initial: live(LOADS),
    steps: [open(), ev('createSession', AGENT, '/work', []), ev('releaseHeld'), tick]
  },
  {
    name: 'history_open_reopens_a_chat_in_error',
    context: { payloads: { s1: payload() }, agentReplies: [{}] },
    initial: live(LOADS, {
      sessions: {
        s1: {
          id: 's1',
          agentId: 'agent-old',
          status: 'error',
          title: null,
          activeTurn: false,
          openTurnId: null,
          cwd: '/work',
          createdAt: 1
        }
      },
      messages: { s1: [] }
    }),
    steps: [open(), tick]
  },
  {
    name: 'history_open_after_a_superseded_open_starts_again',
    context: {
      payloads: { s1: payload() },
      agentReplies: [{ hold: true }, { sessionId: 's1' }, { hold: true }]
    },
    initial: live(LOADS),
    steps: [
      open(),
      ev('createSession', AGENT, '/work', []),
      ev('_onSessionClosed', { agentId: AGENT, sessionId: 's1' }),
      open()
    ]
  },
  {
    name: 'history_open_drops_a_payload_read_it_no_longer_owns',
    context: {
      payloads: { s1: { hold: true, release: payload() } },
      agentReplies: [{ sessionId: 's1' }]
    },
    initial: live(LOADS),
    steps: [open(), ev('createSession', AGENT, '/work', []), ev('releaseHeld')]
  },
  {
    name: 'history_open_reload_does_not_attach_to_the_running_turn',
    context: {
      payloads: {
        s1: payload(
          { status: 'active', turnActive: true },
          { messages: [user('turn:t1', 'hi', 1)] }
        )
      },
      agentReplies: [{}]
    },
    initial: live(LOADS),
    steps: [open({ forceNewAgent: true }), tick]
  },
  {
    name: 'history_open_takes_reports_that_landed_while_the_agent_spawned',
    context: {
      payloads: { s1: payload() },
      projects: PROJECTS,
      agentReplies: [
        { hold: true, release: { agentId: 'agent-9', capabilities: RESUMES, authMethods: [] } },
        {}
      ]
    },
    initial: { agentConfigs: [GEMINI], sessionIndex: [{ id: 's1' }] },
    steps: [
      open(),
      ev('_onAgentWorking', { agentId: 'agent-9', sessionId: 's1', working: true }),
      ev('_onBackgroundCommands', {
        agentId: 'agent-9',
        sessionId: 's1',
        revision: 2,
        commands: [{ id: 'c1', command: 'npm run dev' }]
      }),
      ev('releaseHeld'),
      tick
    ]
  },
  {
    name: 'history_open_keeps_controls_an_event_changed_during_the_load',
    context: {
      payloads: { s1: payload() },
      agentReplies: [
        {
          hold: true,
          release: {
            modes: { currentModeId: 'ask', availableModes: [{ id: 'ask' }] },
            models: { currentModelId: 'm1', availableModels: [{ modelId: 'm1', name: 'M1' }] }
          }
        }
      ]
    },
    initial: live(LOADS),
    steps: [
      open(),
      ev('_onModeUpdate', { agentId: AGENT, sessionId: 's1', currentModeId: 'code' }),
      ev('releaseHeld'),
      tick
    ]
  },
  {
    name: 'history_open_resume_failure_other_than_a_missing_session_is_final',
    context: { payloads: { s1: payload() }, agentReplies: [{ reject: 'resume exploded' }] },
    initial: live({ loadSession: true, sessionCapabilities: { resume: {} } }),
    steps: [open()]
  },
  {
    name: 'history_open_failed_load_puts_the_payload_transcript_back',
    context: {
      payloads: { s1: payload() },
      agentReplies: [{ hold: true, release: { reject: 'load died midway' } }]
    },
    initial: live(LOADS),
    steps: [
      open(),
      ev('_onMessageChunk', {
        agentId: AGENT,
        sessionId: 's1',
        role: 'agent',
        content: { type: 'text', text: 'replayed' }
      }),
      ev('releaseHeld')
    ]
  },
  {
    name: 'history_open_untitled_resume_does_not_persist_at_replay_end',
    context: { payloads: { s1: payload({ title: null }) }, agentReplies: [{}] },
    initial: live(RESUMES),
    steps: [open(), tick]
  },
  {
    name: 'history_open_shows_a_signed_out_account',
    context: {
      payloads: { s1: payload() },
      agentReplies: [{ reject: 'Sign in to Claude again.', code: 'claude_account_signed_out' }]
    },
    initial: live(LOADS),
    steps: [open()]
  },
  {
    name: 'history_open_host_turn_flag_wins_over_a_reply',
    context: {
      payloads: {
        s1: payload({ status: 'active', turnActive: true, lastSeq: 2 })
      }
    },
    initial: live(LOADS),
    steps: [open(), tick]
  },
  {
    name: 'history_open_normalizes_user_bubbles_and_restores_payload_models',
    context: {
      payloads: {
        s1: payload(
          {},
          {
            messages: [
              user(
                'turn:t1',
                '<<<TERMUL_READ_ONLY_GUARD>>>read only<<<END_TERMUL_READ_ONLY_GUARD>>> look around',
                1
              ),
              agent('m2', 'done', 2)
            ],
            models: { availableModels: [{ modelId: 'm1', name: 'M1' }] }
          }
        )
      }
    },
    initial: live({}),
    steps: [open()]
  },
  {
    name: 'create_session_lookup_opens_the_session_from_history',
    context: {
      payloads: { s5: payload({ id: 's5' }) },
      agentReplies: [{ sessionId: 's5', lookup: true }, {}]
    },
    initial: live(LOADS),
    steps: [ev('createSession', AGENT, '/work', [], 'p1'), tick]
  },
  {
    name: 'create_session_lookup_fails_without_history',
    context: { agentReplies: [{ sessionId: 's5', lookup: true }] },
    initial: live(LOADS),
    steps: [ev('createSession', AGENT, '/work', [], 'p1')]
  },
  {
    name: 'history_open_signs_in_when_the_load_asks',
    context: { payloads: { s1: payload() }, agentReplies: [REQUIRED, null, {}] },
    initial: live(LOADS, {
      agents: {
        [AGENT]: { id: AGENT, capabilities: LOADS, authMethods: [{ id: 'oauth', name: 'oauth' }] }
      }
    }),
    steps: [open(), tick]
  },
  {
    name: 'history_open_reload_needs_a_new_agent',
    context: {
      payloads: { s1: payload() },
      agentReplies: [{ reject: 'spawn failed: no binary' }]
    },
    initial: { agentConfigs: [GEMINI], sessionIndex: [{ id: 's1' }] },
    steps: [open({ forceNewAgent: true })]
  },
  {
    name: 'history_open_with_explicit_servers_and_roots',
    context: {
      payloads: { s1: payload() },
      agentReplies: [{}],
      projects: [{ id: 'p1', name: 'Work', path: '/work', worktrees: [{ path: '/wt/x' }] }]
    },
    initial: live({ loadSession: true, sessionCapabilities: { additionalDirectories: {} } }),
    steps: [
      open({ mcpServers: [{ type: 'stdio', name: 'x', command: 'x', args: [], env: [] }] }),
      tick
    ]
  }
]
