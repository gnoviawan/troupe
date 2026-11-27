import { accountStatus } from '../../../../src/renderer/components/settings/provider/account-status'
import { antigravityWindowLabel } from '../../../../src/renderer/lib/antigravity-usage-format'
import {
  formatUsageChecked,
  formatUsageReset,
  planLabel,
  primaryUsageWindow,
  usageTone,
  usageWindowLabel
} from '../../../../src/renderer/lib/claude-usage-format'
import { codexWindowLabel } from '../../../../src/renderer/lib/codex-usage-format'
import type {
  ClaudeAccountUsage,
  ClaudeAccountView,
  ClaudeUsageWindow
} from '../../../../src/shared/types/claude-accounts.types'
import type { Scenario, StoreRecorder, VectorEvent } from '../record'

/** The fixed "now" every read uses (epoch ms). */
const NOW = 1_800_000_000_000

/**
 * The Accounts page's words: usage tones, window labels, reset and checked times, plans, and
 * a Claude row's status. `tests/settings` replays each as a read against
 * `model::settings::accounts`.
 */
export const accountsRecorder: StoreRecorder = {
  store: 'accounts',
  sources: [
    'src/renderer/lib/claude-usage-format.ts',
    'src/renderer/lib/codex-usage-format.ts',
    'src/renderer/lib/antigravity-usage-format.ts',
    'src/renderer/components/settings/provider/account-status.tsx'
  ],
  reset() {},
  seed() {},
  snapshot() {
    return null
  },
  dispatch({ action, args }) {
    switch (action) {
      case 'tone':
        return usageTone(args[0] as number)
      case 'label':
        return usageWindowLabel(args[0] as ClaudeUsageWindow)
      case 'reset':
        return formatUsageReset(args[0] as number, NOW)
      case 'checked':
        return formatUsageChecked(args[0] as number, NOW)
      case 'plan':
        return planLabel(args[0] as string | null)
      case 'primary':
        return primaryUsageWindow(args[0] as ClaudeAccountUsage)?.percent ?? null
      case 'status': {
        const status = accountStatus(
          args[0] as ClaudeAccountView,
          (args[1] ?? undefined) as ClaudeAccountUsage | undefined,
          {
            signingIn: args[2] as boolean,
            projectCount: args[3] as number
          }
        )
        return { tone: status.tone, meta: status.meta }
      }
      case 'codexLabel':
        return codexWindowLabel(args[0] as Parameters<typeof codexWindowLabel>[0])
      case 'antigravityLabel':
        return antigravityWindowLabel(args[0] as Parameters<typeof antigravityWindowLabel>[0])
      default:
        throw new Error(`accounts has no action ${action}`)
    }
  }
}

const read = (action: string, ...args: unknown[]): VectorEvent & { query: true } => ({
  action,
  args,
  query: true
})
const win = (kind: string, percent: number, scope: string | null = null): ClaudeUsageWindow => ({
  kind,
  group: null,
  scope,
  percent,
  resetsAt: null,
  severity: null
})
const usage = (status: string, windows: ClaudeUsageWindow[], plan: string | null = 'max') => ({
  accountId: 'a',
  status,
  subscriptionType: plan,
  windows,
  checkedAt: NOW,
  message: null
})
const account = (signedIn: boolean | null) => ({ id: 'a', name: 'Work', builtin: false, signedIn })
const minute = 60

export const accountsScenarios: Scenario[] = [
  {
    name: 'usage_words',
    steps: [
      ...[0, 79.9, 80, 99.5, 100, 140].map((p) => read('tone', p)),
      ...[
        win('session', 1),
        win('weekly_all', 1),
        win('weekly_scoped', 1, 'Opus'),
        win('weekly_scoped', 1, null),
        win('extra_usage', 1)
      ].map((w) => read('label', w)),
      ...[0.2, 45, 134, 120, 70 * 60, 30 * 60 * 24].map((m) =>
        read('reset', NOW / 1000 + m * minute)
      ),
      read('reset', NOW / 1000 - 600),
      ...[30_000, 4 * 60_000, 130 * 60_000].map((ago) => read('checked', NOW - ago)),
      ...['max', 'pro', '', null].map((p) => read('plan', p)),
      read('codexLabel', { kind: 'session', percent: 1, resetsAt: null, windowDurationMins: 300 }),
      read('codexLabel', { kind: 'weekly', percent: 1, resetsAt: null, windowDurationMins: null }),
      read('antigravityLabel', {
        kind: 'model',
        label: '',
        modelId: 'gemini-3-pro',
        percent: 1,
        resetsAt: null
      }),
      read('antigravityLabel', {
        kind: 'model',
        label: 'Gemini 3 Pro',
        modelId: 'g',
        percent: 1,
        resetsAt: null
      })
    ]
  },
  {
    name: 'claude_rows_say_their_state',
    steps: [
      read('primary', usage('ok', [win('session', 42), win('weekly_scoped', 85, 'Opus')])),
      read('primary', usage('error', [win('session', 42)])),
      read(
        'status',
        account(true),
        usage('ok', [win('session', 42), win('weekly_scoped', 85, 'Opus')]),
        false,
        0
      ),
      read('status', account(true), usage('ok', [win('session', 100)]), false, 0),
      read('status', account(true), usage('ok', [win('session', 10)]), false, 2),
      read('status', account(true), usage('ok', [win('session', 10)], null), false, 1),
      read('status', account(true), null, false, 0),
      read('status', account(true), usage('ok', [], 'pro'), true, 0),
      read('status', account(false), null, false, 3),
      read('status', account(true), usage('signed_out', []), false, 0),
      read('status', account(null), null, false, 0)
    ]
  }
]
