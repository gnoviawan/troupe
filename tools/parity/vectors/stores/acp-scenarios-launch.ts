// The ACP store's launch placeholder scenarios for `acp.ts`. Each restates a
// case of `lifecycle.test.ts` or `prompting.test.ts`.
import type { Scenario } from '../record'
import { ev, message, S, seeded, text } from './acp-fixtures'

const launch = (extra: Record<string, unknown> = {}) =>
  ev('createLaunchPlaceholder', { configId: 'cfg-1', cwd: '/work', projectId: 'p1', ...extra })

export const launchScenarios: Scenario[] = [
  {
    name: 'launch_placeholder_shows_the_chat_before_its_session',
    steps: [
      launch(),
      launch({
        initialUserBlocks: [text('hello')],
        models: { currentModelId: 'm1', availableModels: [] },
        modes: null,
        configOptions: [{ id: 'effort' }],
        worktreePath: '/wt',
        worktreeBranch: 'b1',
        worktreeProgressId: 'wp-1',
        kind: 'quick'
      }),
      launch({ kind: 'chat', initialUserBlocks: [] })
    ]
  },
  {
    name: 'launch_placeholder_is_discarded_with_its_queue',
    initial: {
      ...seeded(),
      activeSessionId: S,
      launchingSessionIds: { [S]: true, s2: true },
      promptQueues: { [S]: [{ id: 'q1', blocks: [text('later')] }] }
    },
    steps: [
      ev('discardLaunchPlaceholder', S),
      ev('discardLaunchPlaceholder', 's2'),
      ev('discardLaunchPlaceholder', 's3')
    ]
  },
  {
    name: 'seed_launch_user_message_opens_the_turn_once',
    initial: {
      sessions: {
        [S]: {
          id: S,
          agentId: 'agent-1',
          status: 'initializing',
          title: null,
          activeTurn: false,
          openTurnId: null,
          lastError: 'old',
          lastErrorCode: 5,
          lastErrorData: { x: 1 }
        },
        s2: {
          id: 's2',
          agentId: 'agent-1',
          status: 'active',
          title: null,
          activeTurn: false,
          openTurnId: 'turn:open',
          lastError: null
        },
        s3: {
          id: 's3',
          agentId: 'agent-1',
          status: 'closed',
          title: null,
          activeTurn: false,
          openTurnId: null,
          lastError: null
        },
        s5: {
          id: 's5',
          agentId: 'agent-1',
          status: 'active',
          title: null,
          activeTurn: false,
          openTurnId: 'turn:old',
          lastError: null
        },
        s6: {
          id: 's6',
          agentId: 'agent-1',
          status: 'active',
          title: null,
          activeTurn: false,
          openTurnId: null,
          lastError: null
        },
        s4: {
          id: 's4',
          agentId: 'agent-1',
          status: 'active',
          title: null,
          activeTurn: false,
          openTurnId: null,
          lastError: null
        }
      },
      messages: {
        [S]: [],
        s2: [message('u0', 'user', 'first')],
        s4: [message('u4', 'user', 'first')],
        s6: []
      }
    },
    steps: [
      ev('seedLaunchUserMessage', S, [text('hello')]),
      ev('seedLaunchUserMessage', S, [text('again')]),
      ev('seedLaunchUserMessage', 's2', [text('more')]),
      ev('seedLaunchUserMessage', 's3', [text('closed')]),
      ev('seedLaunchUserMessage', 's4', [text('reopen')]),
      ev('seedLaunchUserMessage', 's9', [text('unknown')]),
      ev('seedLaunchUserMessage', 's5', [text('fresh')]),
      ev('seedLaunchUserMessage', 's6', []),
      ev('seedLaunchUserMessage', S, []),
      ev('clearLaunchingSession', S),
      ev('clearLaunchingSession', S)
    ]
  }
]
