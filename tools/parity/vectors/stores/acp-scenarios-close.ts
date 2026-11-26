// The ACP store's close and delete scenarios for `acp.ts`. Each restates a
// case of `lifecycle.test.ts`, `closed-transcript-eviction.test.ts` or
// `history-session.test.ts`.
import type { Scenario } from '../record'
import { AGENT, ev, message, S, seeded, text } from './acp-fixtures'

/** s1 live with a reply, prompts waiting on it; s2 another live chat; s3 closed. */
function chats(): Record<string, unknown> {
  const base = seeded({ activeTurn: true, openTurnId: 'turn:t1' })
  const one = (base.sessions as Record<string, Record<string, unknown>>)[S]
  return {
    sessions: {
      [S]: one,
      s2: { ...one, id: 's2', activeTurn: false, openTurnId: null },
      s3: { ...one, id: 's3', status: 'closed', activeTurn: false, openTurnId: null }
    },
    messages: {
      [S]: [message('u1', 'user', 'go'), message('a1', 'agent', 'done')],
      s2: [message('u2', 'user', 'hi')]
    },
    agents: {
      [AGENT]: {
        id: AGENT,
        capabilities: { sessionCapabilities: { delete: {} } },
        authMethods: []
      },
      'agent-2': { id: 'agent-2', capabilities: { loadSession: true }, authMethods: [] }
    },
    sessionIndex: [{ id: S }, { id: 's2' }, { id: 's3' }, { id: 's8', agentId: 'agent-gone' }],
    preparedSessions: { 'prep-1': S, 'prep-2': 's2' },
    promptQueues: { [S]: [{ id: 'q1', blocks: [text('later')] }] },
    suppressQueueFlush: { [S]: true },
    pendingPermissions: {
      p1: { requestId: 'p1', agentId: AGENT, sessionId: S, options: [], toolCall: null },
      p2: { requestId: 'p2', agentId: AGENT, sessionId: 's2', options: [], toolCall: null }
    },
    pendingQuestions: {
      q1: { questionId: 'q1', agentId: AGENT, sessionId: S, question: 'q', options: [] }
    }
  }
}

export const closeScenarios: Scenario[] = [
  {
    name: 'close_session_ends_the_agent_session_then_frees_the_chat',
    context: { agentReplies: [null, { reject: 'gone' }] },
    initial: chats(),
    steps: [
      ev('closeSession', S),
      ev('closeSession', 's2'),
      ev('closeSession', 's3'),
      ev('closeSession', 's9')
    ]
  },
  {
    name: 'close_session_keeps_the_transcript_an_open_tab_shows',
    context: { agentReplies: [null], openChatTabs: [S] },
    initial: chats(),
    steps: [ev('closeSession', S)]
  },
  {
    name: 'delete_history_session_asks_the_agent_that_can_delete',
    context: { agentReplies: [null, null, { reject: 'unsupported' }, null] },
    initial: chats(),
    steps: [ev('deleteHistorySession', S), ev('deleteHistorySession', 's8')]
  },
  {
    name: 'close_session_answered_after_the_chat_was_deleted',
    context: { agentReplies: [{ hold: true, release: null }, null, null] },
    initial: chats(),
    steps: [ev('closeSession', S), ev('deleteHistorySession', S), ev('releaseHeld')]
  },
  {
    name: 'delete_history_session_answered_after_the_chat_was_closed',
    context: { agentReplies: [{ hold: true, release: { reject: 'gone' } }, null, null] },
    initial: chats(),
    steps: [ev('deleteHistorySession', S), ev('closeSession', S), ev('releaseHeld')]
  },
  {
    name: 'delete_history_session_keeps_the_chat_when_the_history_stays',
    context: { agentReplies: [{ reject: 'disk full' }, null] },
    initial: {
      ...chats(),
      agents: {
        [AGENT]: { id: AGENT, capabilities: { loadSession: true }, authMethods: [] }
      }
    },
    steps: [ev('deleteHistorySession', 's2'), ev('deleteHistorySession', 's7')]
  }
]
