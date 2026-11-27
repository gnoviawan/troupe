import { buildIssueMarkdown, buildIssueUrl } from '../../../../src/renderer/lib/bug-report-api'
import type { BugReportContext } from '../../../../src/shared/types/bug-report.types'
import type { Scenario, StoreRecorder, VectorEvent } from '../record'

/**
 * The Report a Bug dialog's report: the markdown it previews and sends, and the prefilled
 * `issues/new` URL of its fallback. `tests/bug_report` replays each as a read against
 * `model::bug_report`.
 */
export const bugReportRecorder: StoreRecorder = {
  store: 'bug-report',
  sources: ['src/renderer/lib/bug-report-api.ts'],
  reset() {},
  seed() {},
  snapshot() {
    return null
  },
  dispatch({ action, args }) {
    if (action === 'markdown')
      return buildIssueMarkdown(
        args[0] as BugReportContext,
        args[1] as string,
        args[2] as string,
        args[3] as number
      )
    if (action === 'issueUrl')
      return buildIssueUrl(
        args[0] as BugReportContext,
        args[1] as string,
        args[2] as string,
        args[3] as boolean
      )
    throw new Error(`bug-report has no action ${action}`)
  }
}

const read = (action: string, ...args: unknown[]): VectorEvent & { query: true } => ({
  action,
  args,
  query: true
})

const CONTEXT: BugReportContext = {
  appVersion: '0.6.0',
  os: 'linux',
  arch: 'x86_64',
  channel: 'release',
  runId: 'run-1',
  collectedAt: '2026-11-26T10:00:00Z',
  logAvailable: true,
  logTruncated: false,
  logTail: 'INFO started\nWARN [redacted] token\n'
}

const NO_LOG: BugReportContext = { ...CONTEXT, logAvailable: false, logTail: '' }
const FENCED: BugReportContext = {
  ...CONTEXT,
  logTruncated: true,
  logTail: 'a ```` b\n~~~ c ``'
}

export const bugReportScenarios: Scenario[] = [
  {
    name: 'the_markdown_is_what_the_dialog_previews_and_sends',
    steps: [
      read('markdown', CONTEXT, '  Crash on open ', ' Steps:\n1. open\n', 0),
      read('markdown', NO_LOG, '', '', 1),
      read('markdown', FENCED, 'Fence', 'x', 3)
    ]
  },
  {
    name: 'the_fallback_url_carries_the_body_or_the_short_one',
    steps: [
      read('issueUrl', CONTEXT, 'Crash on open', 'short body & more', true),
      read('issueUrl', CONTEXT, 'Huge', 'y'.repeat(9000), true),
      read('issueUrl', CONTEXT, 'Huge', 'y'.repeat(9000), false),
      read('issueUrl', CONTEXT, 'é'.repeat(3000), 'body', true)
    ]
  }
]
