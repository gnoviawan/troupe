import {
  resetGitStatusStoreControlForTests,
  useGitStatusStore
} from '../../../../src/renderer/stores/git-status-store'
import type { Scenario, StoreRecorder, VectorEvent } from '../record'
import { type GitReply, gitMock } from './git-mock'

const unwatch = new Map<string, () => void>()

const settle = () => new Promise((resolve) => setTimeout(resolve, 0))

const ev = (action: string, ...args: unknown[]): VectorEvent => ({ action, args })
const ok = (value: unknown = null): GitReply => ({ ok: value })
const fail = (message: string, code?: string): GitReply =>
  code ? { reject: message, code } : { reject: message }
const HOLD: GitReply = { hold: true }

export const gitStatusRecorder: StoreRecorder = {
  store: 'git-status',
  sources: ['src/renderer/stores/git-status-store.ts'],
  reset(context) {
    resetGitStatusStoreControlForTests()
    useGitStatusStore.setState(useGitStatusStore.getInitialState(), true)
    gitMock.replies = [...((context.git as GitReply[] | undefined) ?? [])]
    gitMock.held = []
    gitMock.active = true
    unwatch.clear()
  },
  seed(initial) {
    useGitStatusStore.setState(initial)
  },
  snapshot() {
    const {
      paths,
      statuses,
      diffs,
      diffErrors,
      commitContexts,
      diffStats,
      errors,
      stashes,
      branches,
      selectedFile,
      isFetchingStatus,
      statusFetchCount
    } = useGitStatusStore.getState()
    return {
      paths,
      statuses,
      diffs,
      diffErrors,
      commitContexts,
      diffStats,
      errors,
      stashes,
      branches,
      selectedFile,
      isFetchingStatus,
      statusFetchCount
    }
  },
  async dispatch({ action, args }) {
    if (action === 'releaseHeld') {
      const release = gitMock.held.shift()
      if (!release)
        throw new Error(`releaseHeld with nothing held (${gitMock.replies.length} replies left)`)
      release(args[0] as GitReply)
      await settle()
      return undefined
    }
    if (action === 'unwatchDiffStat') {
      unwatch.get(args[0] as string)?.()
      return undefined
    }
    const state = useGitStatusStore.getState() as unknown as Record<
      string,
      (...a: unknown[]) => unknown
    >
    let outcome: { value: unknown } | { reject: string } | null = null
    const returned = state[action](...args)
    if (typeof returned === 'function') {
      unwatch.set(args[0] as string, returned as () => void)
      return undefined
    }
    Promise.resolve(returned).then(
      (value) => {
        outcome = { value }
      },
      (error: unknown) => {
        outcome = { reject: error instanceof Error ? error.message : String(error) }
      }
    )
    await settle()
    const settled = outcome as { value: unknown } | { reject: string } | null
    if (settled === null) return { pending: true }
    return 'reject' in settled ? settled : settled.value
  }
}

const CWD = '/repo'

const context = (over: Record<string, unknown> = {}) => ({
  branch: 'main',
  hasUpstream: true,
  ahead: 1,
  behind: 0,
  stagedCount: 1,
  hasHead: true,
  lastSubject: 'last subject',
  lastBody: '',
  ...over
})

const head = (hash: string) => ok([{ hash }])

const STATUS = [
  { path: 'a.ts', status: 'modified', staged: true },
  { path: 'a.ts', status: 'modified', staged: false },
  { path: 'b.ts', status: 'untracked', staged: false }
]

const STASHES = [{ index: 0, name: 'stash@{0}', message: 'WIP on main: abc wip' }]

/** `refreshAfterWrite` answered: status, context, HEAD (diff stat unwatched). */
const afterWrite = (status: unknown = STATUS, ctx = context(), hash = 'h2'): GitReply[] => [
  ok(status),
  ok(ctx),
  head(hash)
]

/** `refreshAfterMutation` answered: `afterWrite`, then stashes and branches. */
const afterMutation = (branches = ['main', 'feature']): GitReply[] => [
  ...afterWrite(),
  ok(STASHES),
  ok(branches)
]

/** Mirrors the cases of `stores/git-status-store.test.ts` (the `refreshTracked` pool and
 * the unresolved-worktree write block are not ported). */
export const gitStatusScenarios: Scenario[] = [
  {
    name: 'fetch_commit_context_stores_it_with_its_head',
    context: { git: [ok(context()), head('h1'), ok(context({ hasHead: false, branch: 'dev' }))] },
    steps: [ev('fetchCommitContext', CWD), ev('fetchCommitContext', '/other')]
  },
  {
    name: 'fetch_commit_context_failure_drops_the_stale_context',
    context: {
      git: [ok(context()), head('h1'), fail('not a repository'), ok(context()), head('h1')]
    },
    steps: [
      ev('fetchCommitContext', CWD),
      ev('fetchCommitContext', CWD),
      ev('fetchCommitContext', CWD)
    ]
  },
  {
    name: 'a_stale_commit_context_is_ignored',
    context: {
      git: [HOLD, ok(context({ branch: 'new' })), head('h-new')]
    },
    steps: [
      ev('fetchCommitContext', CWD),
      ev('fetchCommitContext', CWD),
      ev('releaseHeld', ok(context({ branch: 'old' })))
    ]
  },
  {
    name: 'a_superseded_context_records_no_head',
    context: {
      git: [
        ok(context()),
        HOLD,
        ok(context({ branch: 'second' })),
        head('h-new'),
        ok(),
        ...afterMutation()
      ]
    },
    steps: [
      ev('fetchCommitContext', CWD),
      ev('fetchCommitContext', CWD),
      ev('releaseHeld', head('h-old')),
      ev('commit', CWD, 'summary', '', false)
    ]
  },
  {
    name: 'a_stale_status_is_ignored',
    context: { git: [HOLD, ok(STATUS)] },
    steps: [ev('refreshStatus', CWD), ev('refreshStatus', CWD), ev('releaseHeld', ok([]))]
  },
  {
    name: 'a_status_failure_keeps_the_previous_status_and_clears_on_success',
    context: { git: [ok(STATUS), fail('index.lock exists'), ok([])] },
    steps: [ev('refreshStatus', CWD), ev('refreshStatus', CWD), ev('refreshStatus', CWD)]
  },
  {
    name: 'commit_sends_the_head_last_shown_and_refreshes',
    context: { git: [ok(context()), head('h1'), ok(), ...afterMutation()] },
    steps: [ev('fetchCommitContext', CWD), ev('commit', CWD, 'fix: thing', 'body', false)]
  },
  {
    name: 'commit_on_an_unborn_branch_expects_the_empty_head',
    context: {
      git: [ok(context({ hasHead: false, branch: 'main' })), ok(), ...afterMutation()]
    },
    steps: [ev('fetchCommitContext', CWD), ev('commit', CWD, 'first', '', true)]
  },
  {
    name: 'a_commit_before_any_context_reads_the_head_first',
    context: { git: [ok(context()), head('h0'), ok(), ...afterMutation()] },
    steps: [ev('commit', CWD, 'summary', '', false)]
  },
  {
    name: 'a_commit_refused_because_head_moved_refreshes_and_rejects',
    context: {
      git: [
        ok(context()),
        head('h1'),
        fail('HEAD moved', 'precondition_failed'),
        ...afterWrite(STATUS, context(), 'h-moved'),
        ok(),
        ...afterMutation()
      ]
    },
    steps: [
      ev('fetchCommitContext', CWD),
      ev('commit', CWD, 'summary', '', false),
      ev('commit', CWD, 'summary', '', false)
    ]
  },
  {
    name: 'a_head_that_cannot_be_read_refuses_the_commit',
    context: { git: [ok(context()), fail('log failed')] },
    steps: [ev('fetchCommitContext', CWD), ev('commit', CWD, 'summary', '', false)]
  },
  {
    name: 'a_failed_context_forgets_the_head_and_the_commit_refetches_it',
    context: {
      git: [ok(context()), head('h1'), fail('gone'), fail('still gone')]
    },
    steps: [
      ev('fetchCommitContext', CWD),
      ev('fetchCommitContext', CWD),
      ev('commit', CWD, 'summary', '', false)
    ]
  },
  {
    name: 'commit_errors_reject_without_a_refresh',
    context: { git: [ok(context()), head('h1'), fail('hook rejected')] },
    steps: [ev('fetchCommitContext', CWD), ev('commit', CWD, 'summary', '', false)]
  },
  {
    name: 'a_commit_resolves_when_its_refresh_fails',
    context: {
      git: [
        ok(context()),
        head('h1'),
        ok(),
        fail('status down'),
        fail('context down'),
        fail('stash down'),
        fail('branches down')
      ]
    },
    steps: [ev('fetchCommitContext', CWD), ev('commit', CWD, 'summary', '', false)]
  },
  {
    name: 'push_refreshes_and_a_failed_push_rejects',
    context: { git: [ok(), ...afterMutation(), fail('no upstream')] },
    steps: [ev('push', CWD), ev('push', CWD)]
  },
  {
    name: 'stage_files_stages_each_then_refreshes_once',
    context: {
      git: [ok('diff a'), ok('diff a staged'), ok(), ok(), ...afterWrite()]
    },
    steps: [
      ev('fetchDiff', CWD, 'a.ts', false),
      ev('fetchDiff', CWD, 'a.ts', true),
      ev('stageFiles', CWD, ['a.ts', 'b.ts'])
    ]
  },
  {
    name: 'a_mid_batch_stage_failure_still_refreshes_and_rejects',
    context: { git: [ok(), fail('b.ts is locked'), ...afterWrite()] },
    steps: [ev('stageFiles', CWD, ['a.ts', 'b.ts', 'c.ts'])]
  },
  {
    name: 'single_file_writes_and_an_empty_batch',
    context: { git: [ok(), ...afterWrite(), ok(), ...afterWrite(), ok(), ...afterWrite([])] },
    steps: [
      ev('stageFile', CWD, 'a.ts'),
      ev('unstageFile', CWD, 'a.ts'),
      ev('discardFile', CWD, 'b.ts'),
      ev('unstageFiles', CWD, []),
      ev('discardFiles', CWD, [])
    ]
  },
  {
    name: 'hunk_writes_invalidate_the_file_and_refresh',
    context: {
      git: [ok('diff'), ok(), ...afterWrite(), fail('patch does not apply'), ...afterWrite()]
    },
    steps: [
      ev('fetchDiff', CWD, 'a.ts', false),
      ev('stageHunk', CWD, 'a.ts', '@@ -1 +1 @@\n-a\n+b\n'),
      ev('unstageHunk', CWD, 'a.ts', '@@ -1 +1 @@\n-a\n+b\n')
    ]
  },
  {
    name: 'stash_and_branch_lists_and_writes',
    context: {
      git: [
        ok(STASHES),
        ok(['main']),
        fail('no stashes'),
        ok(),
        ...afterMutation(),
        ok(),
        ...afterMutation(),
        ok(),
        ...afterMutation(),
        ok(),
        ok([]),
        fail('stash has conflicts')
      ]
    },
    steps: [
      ev('fetchStashes', CWD),
      ev('fetchBranches', CWD),
      ev('fetchStashes', CWD),
      ev('stashSave', CWD, 'wip', true),
      ev('stashApply', CWD, 0),
      ev('stashPop', CWD, 0),
      ev('stashDrop', CWD, 0),
      ev('stashApply', CWD, 3)
    ]
  },
  {
    name: 'branch_switch_and_create_clear_the_selection_and_refresh',
    context: {
      git: [
        ok(),
        ...afterMutation(),
        ok(),
        ...afterMutation(['main', 'feature', 'new']),
        fail('exists')
      ]
    },
    steps: [
      ev('setSelectedFile', 'a.ts'),
      ev('branchSwitch', CWD, 'feature'),
      ev('setSelectedFile', 'b.ts'),
      ev('branchCreate', CWD, 'new'),
      ev('branchCreate', CWD, 'new')
    ]
  },
  {
    name: 'a_failed_diff_notifies_by_default_and_clears_on_success',
    context: { git: [fail('binary file'), fail('gone', 'command_conflict'), ok('diff')] },
    steps: [
      ev('fetchDiff', CWD, 'a.ts', false),
      ev('fetchDiff', CWD, 'b.ts', true),
      ev('fetchDiff', CWD, 'a.ts', false)
    ]
  },
  {
    name: 'a_quiet_diff_failure_keeps_the_cache_empty_and_logs',
    context: { git: [fail('binary file'), ok('diff')] },
    steps: [
      ev('fetchDiff', CWD, 'a.ts', false, { notify: false }),
      ev('fetchDiff', CWD, 'a.ts', false, { notify: false })
    ]
  },
  {
    name: 'a_diff_invalidated_in_flight_is_discarded',
    context: { git: [HOLD, ok(), ...afterWrite()] },
    steps: [
      ev('fetchDiff', CWD, 'a.ts', false),
      ev('stageFile', CWD, 'a.ts'),
      ev('releaseHeld', ok('stale'))
    ]
  },
  {
    name: 'a_watched_diff_stat_reads_and_an_unwatched_one_does_not',
    context: {
      git: [
        ok(STATUS),
        ok(context()),
        ok({ added: 3, removed: 1 }),
        head('h1'),
        ok({ added: 1, removed: 0 }),
        ok(STATUS),
        ok(context()),
        head('h1')
      ]
    },
    steps: [
      ev('refreshDiffStat', CWD),
      ev('watchDiffStat', CWD),
      ev('refreshWorkingCopy', CWD),
      ev('refreshDiffStat', '/other'),
      ev('watchDiffStat', '/other'),
      ev('refreshDiffStat', '/other'),
      ev('unwatchDiffStat', CWD),
      ev('refreshWorkingCopy', CWD)
    ]
  },
  {
    name: 'a_diff_stat_failure_is_stored_and_logged',
    context: { git: [fail('stat failed')] },
    steps: [ev('watchDiffStat', CWD), ev('refreshDiffStat', CWD)]
  },
  {
    name: 'refresh_working_copy_never_rejects',
    context: { git: [fail('status down'), fail('context down'), fail('stat down')] },
    steps: [ev('watchDiffStat', CWD), ev('refreshWorkingCopy', CWD)]
  },
  {
    name: 'two_spellings_of_a_windows_path_share_one_entry',
    context: { git: [ok(STATUS), ok(['main']), ok([])] },
    steps: [
      ev('refreshStatus', 'C:\\Repo'),
      ev('fetchBranches', 'c:/repo/'),
      ev('refreshStatus', 'c:/repo/')
    ]
  }
]
