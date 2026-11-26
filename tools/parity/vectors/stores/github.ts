import { useGithubStore } from '../../../../src/renderer/stores/github-store'
import type { Scenario, StoreRecorder, VectorEvent } from '../record'
import { routesRecorder } from './git-routes'
import { type GithubReply, githubMock } from './github-mock'

const settle = () => new Promise((resolve) => setTimeout(resolve, 0))

const ev = (action: string, ...args: unknown[]): VectorEvent => ({ action, args })
const ok = (data: unknown): GithubReply => ({ success: true, data })
const fail = (code: string, error = code.toLowerCase()): GithubReply => ({
  success: false,
  code,
  error
})
const HOLD: GithubReply = { hold: true }

export const githubRecorder: StoreRecorder = {
  store: 'github',
  sources: ['src/renderer/stores/github-store.ts'],
  reset(context) {
    useGithubStore.setState(useGithubStore.getInitialState(), true)
    githubMock.replies = [...((context.github as GithubReply[] | undefined) ?? [])]
    githubMock.held = []
  },
  seed(initial) {
    useGithubStore.setState(initial)
  },
  snapshot() {
    const { contexts, lists, items, checks, logs, repoMeta, filters } = useGithubStore.getState()
    return { contexts, lists, items, checks, logs, repoMeta, filters }
  },
  async dispatch({ action, args }) {
    if (action === 'releaseHeld') {
      const release = githubMock.held.shift()
      if (!release) throw new Error('releaseHeld with nothing held')
      release(args[0] as GithubReply)
      await settle()
      return undefined
    }
    const state = useGithubStore.getState() as unknown as Record<
      string,
      (...a: unknown[]) => unknown
    >
    let outcome: { value: unknown } | null = null
    Promise.resolve(state[action](...args)).then((value) => {
      outcome = { value }
    })
    await settle()
    const settled = outcome as { value: unknown } | null
    return settled === null ? { pending: true } : settled.value
  }
}

/** The request the web client's real `githubApi` sends for each call. */
export function githubRoutesRecorder(
  api: Record<string, (...args: unknown[]) => unknown>
): StoreRecorder {
  return routesRecorder('github-routes', ['src/renderer/lib/github-api.ts'], api)
}

const CWD = '/repo'
const REPO = { host: 'github.com', owner: 'acme', name: 'app' }
const UPSTREAM = { host: 'github.com', owner: 'upstream', name: 'app' }
const AT = '2026-11-20T10:00:00Z'
const PR = { cwd: CWD, repo: 'origin', kind: 'pr', number: 7 }
const ISSUE = { cwd: CWD, repo: 'origin', kind: 'issue', number: 3 }

const connected = {
  auth: {
    state: 'connected',
    host: 'github.com',
    account: 'ann',
    scopes: ['repo'],
    tokenSource: 'keyring',
    missingScopes: [],
    fixCommands: []
  },
  repo: { kind: 'resolved', origin: REPO, upstream: UPSTREAM }
}

const signedOut = {
  auth: {
    state: 'unauthenticated',
    host: 'github.com',
    loginCommand: 'gh auth login --hostname github.com'
  },
  repo: { kind: 'resolved', origin: REPO, upstream: null }
}

const listItem = (number: number, over: Record<string, unknown> = {}) => ({
  kind: 'pr',
  number,
  title: `Item ${number}`,
  state: 'open',
  author: 'ann',
  labels: [{ name: 'bug', color: 'd73a4a' }],
  assignees: [],
  commentCount: 1,
  updatedAt: AT,
  url: `https://github.com/acme/app/pull/${number}`,
  checks: 'pending',
  ...over
})

const list = (...numbers: number[]) => ({
  repo: REPO,
  items: numbers.map((n) => listItem(n)),
  fetchedAt: AT
})

const comment = (id: string, body: string, over: Record<string, unknown> = {}) => ({
  id,
  author: 'bob',
  isBot: false,
  body,
  createdAt: AT,
  url: `https://github.com/acme/app/pull/7#${id}`,
  ...over
})

const pull = (over: Record<string, unknown> = {}) => ({
  headRefName: 'feature/x',
  headSha: 'abc123',
  baseRefName: 'main',
  isCrossRepository: false,
  maintainerCanModify: true,
  mergeable: 'mergeable',
  mergeStateStatus: 'CLEAN',
  autoMergeEnabled: false,
  additions: 12,
  deletions: 3,
  changedFiles: 2,
  reviewThreads: [
    {
      id: 'T1',
      path: 'src/a.ts',
      line: 4,
      isResolved: false,
      isOutdated: false,
      comments: [comment('R1', 'Why?')]
    }
  ],
  ...over
})

const detail = (over: Record<string, unknown> = {}) => ({
  repo: REPO,
  item: listItem(7),
  body: 'Fixes the thing.',
  createdAt: AT,
  comments: [comment('C1', 'Looks good')],
  timeline: [
    {
      id: 'E1',
      kind: 'labeled',
      actor: 'ann',
      createdAt: AT,
      label: { name: 'bug', color: 'd73a4a' },
      assignee: null
    }
  ],
  stateReason: null,
  pr: pull(),
  fetchedAt: AT,
  ...over
})

const issueDetail = detail({
  item: listItem(3, { kind: 'issue', checks: null }),
  pr: null,
  comments: [],
  timeline: []
})

const checks = {
  headSha: 'abc123',
  rollup: 'failure',
  checks: [
    { name: 'test', workflow: 'CI', status: 'failure', detailsUrl: 'https://x/1', jobId: 11 },
    { name: 'lint', workflow: 'CI', status: 'success', detailsUrl: null, jobId: null }
  ],
  fetchedAt: AT
}

const meta = {
  repo: REPO,
  labels: [{ name: 'bug', color: 'd73a4a', description: null }],
  assignableUsers: ['ann', 'bob'],
  viewer: 'ann',
  fetchedAt: AT
}

export const githubScenarios: Scenario[] = [
  {
    name: 'context, filter and list',
    context: {
      github: [
        ok(connected),
        ok(list(7, 8)),
        ok(list(7)),
        fail('GH_RATE_LIMITED', 'API rate limit exceeded'),
        fail('GH_UNAUTHENTICATED', 'not logged in'),
        ok(signedOut)
      ]
    },
    steps: [
      ev('loadContext', CWD),
      ev('loadList', CWD),
      ev('setFilter', CWD, { kind: 'pr', search: '  crash ' }),
      ev('loadList', CWD, { refresh: true }),
      ev('loadList', CWD)
    ]
  },
  {
    name: 'every filter field keys its own list',
    context: { github: [ok(list(1)), ok(list(2)), ok(list(3)), ok(list(4)), ok(list(5))] },
    steps: [
      ev('setFilter', CWD, { labels: ['bug', 'p1'], labelMode: 'exclude' }),
      ev('setFilter', CWD, { assignee: { kind: 'user', login: 'bob' } }),
      ev('setFilter', CWD, { assignee: { kind: 'anyone' }, reviewRequested: true }),
      ev('setFilter', CWD, { state: 'all', sort: 'comments' }),
      ev('setFilter', CWD, { repo: 'upstream' })
    ]
  },
  {
    name: 'a slower older read never overwrites a newer one',
    context: { github: [HOLD, ok(detail({ body: 'new' }))] },
    steps: [
      ev('loadItem', PR),
      ev('loadItem', PR, { refresh: true }),
      ev('releaseHeld', ok(detail({ body: 'old' })))
    ]
  },
  {
    name: 'item, checks, log and repository meta',
    context: {
      github: [
        ok(detail()),
        ok(checks),
        ok({ jobId: 11, tail: '##[error] boom', truncated: true }),
        ok(meta),
        fail('GH_TIMEOUT', 'timed out'),
        fail('GH_NOT_FOUND', 'no such issue'),
        fail('GH_NOT_GITHUB_REPO', 'remote moved'),
        ok({ auth: connected.auth, repo: { kind: 'not_github' } })
      ]
    },
    steps: [
      ev('loadItem', PR),
      ev('loadChecks', PR),
      ev('loadCheckLog', CWD, 'origin', 11),
      ev('loadRepoMeta', CWD, 'origin'),
      ev('loadChecks', PR, { refresh: true }),
      ev('loadItem', ISSUE),
      ev('loadRepoMeta', CWD, 'origin', { refresh: true })
    ]
  },
  {
    name: 'merge re-reads checks, list and item',
    context: {
      github: [
        ok({ number: 7 }),
        ok(checks),
        ok(list(8)),
        ok(detail({ item: listItem(7, { state: 'merged' }) })),
        fail('GH_NOT_MERGEABLE', 'Pull request is not mergeable'),
        ok(checks),
        ok(list(8)),
        HOLD
      ]
    },
    steps: [
      ev('merge', PR, { kind: 'merge', method: 'squash' }),
      ev('merge', PR, { kind: 'enable_auto_merge', method: 'rebase' }),
      ev('releaseHeld', fail('GH_FAILED', 'gh exited 1'))
    ]
  },
  {
    name: 'comments show before the re-read',
    context: {
      github: [
        ok(detail()),
        ok({ comment: comment('C2', 'Thanks') }),
        HOLD,
        ok({ comment: comment('R2', 'Because', { isBot: true }) }),
        HOLD,
        fail('GH_EMPTY_COMMENT', 'empty'),
        ok({ comment: comment('C9', 'unseen') }),
        ok(issueDetail)
      ]
    },
    steps: [
      ev('loadItem', PR),
      ev('comment', PR, { kind: 'conversation' }, 'Thanks'),
      ev('comment', PR, { kind: 'review_thread', threadId: 'T1' }, 'Because'),
      ev('comment', PR, { kind: 'conversation' }, '   '),
      ev('comment', ISSUE, { kind: 'conversation' }, 'unseen')
    ]
  },
  {
    name: 'label create, edit and state writes',
    context: {
      github: [
        ok({ label: { name: 'p1', color: 'b60205', description: 'urgent' } }),
        ok({
          ...meta,
          labels: [...meta.labels, { name: 'p1', color: 'b60205', description: 'urgent' }]
        }),
        ok({ number: 3 }),
        ok(list(3)),
        ok(issueDetail),
        fail('GH_FAILED', 'no permission'),
        ok(list(3)),
        ok(issueDetail),
        ok({ number: 3 }),
        ok(list()),
        ok(
          detail({
            item: listItem(3, { kind: 'issue', state: 'closed' }),
            stateReason: 'not_planned',
            pr: null
          })
        )
      ]
    },
    steps: [
      ev('createLabel', CWD, 'origin', { name: 'p1', color: 'b60205', description: 'urgent' }),
      ev('editItem', ISSUE, { addLabels: ['p1'], removeLabels: ['bug'] }),
      ev('editItem', ISSUE, { addAssignees: ['bob'] }),
      ev('setItemState', ISSUE, { kind: 'close', reason: 'not_planned' })
    ]
  }
]

const ROUTE_CWD = 'C:\\Users\\a b\\repo'
const ROUTE_REF = { cwd: ROUTE_CWD, repo: 'upstream', kind: 'pr', number: 12 }

export const githubRoutesScenarios: Scenario[] = [
  {
    name: 'reads',
    steps: [
      ev('getContext', ROUTE_CWD),
      ev('listItems', {
        cwd: ROUTE_CWD,
        filter: {
          repo: 'origin',
          kind: 'all',
          state: 'open',
          search: '',
          labels: [],
          labelMode: 'any',
          assignee: { kind: 'anyone' },
          reviewRequested: false,
          sort: 'updated'
        },
        refresh: true
      }),
      ev('getItem', { ...ROUTE_REF, refresh: true }),
      ev('getChecks', { cwd: ROUTE_CWD, repo: 'upstream', number: 12 }),
      ev('getCheckLog', { cwd: ROUTE_CWD, repo: 'origin', jobId: 99 }),
      ev('getRepoMeta', { cwd: ROUTE_CWD, repo: 'origin' })
    ]
  },
  {
    name: 'writes',
    steps: [
      ev('merge', {
        cwd: ROUTE_CWD,
        repo: 'upstream',
        number: 12,
        action: { kind: 'disable_auto_merge' }
      }),
      ev('comment', {
        ...ROUTE_REF,
        target: { kind: 'review_thread', threadId: 'T1' },
        body: 'ok'
      }),
      ev('createLabel', {
        cwd: ROUTE_CWD,
        repo: 'origin',
        name: 'p1',
        color: 'b60205',
        description: null
      }),
      ev('editItem', { ...ROUTE_REF, addLabels: ['x'], removeAssignees: ['y'] }),
      ev('setItemState', { ...ROUTE_REF, action: { kind: 'reopen' } })
    ]
  }
]
