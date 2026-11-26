import type { Scenario, StoreRecorder, VectorEvent } from '../record'

/**
 * The HTTP request the web client's real `gitApi` sends for each call: method, path, JSON
 * body and whether it carries `X-Command-Id` (a ledgered route). `fetch` is captured and
 * never answers, so nothing reaches a server. `tests/vectors.rs` checks `Call::route` and
 * `Call::ledger` against each one.
 */

export type Api = Record<string, (...args: unknown[]) => unknown>

let captured: Array<Record<string, unknown>> = []

async function capture(api: Api, op: string, args: unknown[]): Promise<void> {
  const real = globalThis.fetch
  globalThis.fetch = ((input: string | URL, init?: RequestInit) => {
    const url = new URL(String(input), 'http://capture.invalid')
    const headers = (init?.headers ?? {}) as Record<string, string>
    captured.push({
      kind: 'http',
      method: init?.method ?? 'GET',
      path: `${url.pathname}${url.search}`,
      body: typeof init?.body === 'string' ? JSON.parse(init.body) : null,
      commandId: Object.keys(headers).some((name) => name.toLowerCase() === 'x-command-id')
    })
    return new Promise<Response>(() => {})
  }) as typeof fetch
  try {
    void Promise.resolve(api[op](...args)).catch(() => {})
    for (let tick = 0; tick < 20 && captured.length === 0; tick += 1) {
      await new Promise((resolve) => setTimeout(resolve, 0))
    }
  } finally {
    globalThis.fetch = real
  }
}

/** A recorder of the request each `api` call sends, under `store`. */
export function routesRecorder(store: string, sources: string[], api: Api): StoreRecorder {
  return {
    store,
    sources,
    reset() {
      captured = []
    },
    seed() {},
    snapshot: () => null,
    async dispatch({ action, args }) {
      captured = []
      await capture(api, action, args)
      const [request] = captured
      if (!request) throw new Error(`${store}: ${action} sent no request`)
      // The effects are what the step reports; the recorder's sink only sees `emit`.
      return request
    }
  }
}

export function gitRoutesRecorder(api: Api): StoreRecorder {
  return routesRecorder(
    'git-routes',
    [
      'src/renderer/lib/git-api.ts',
      'src/renderer/lib/web-server-api.ts',
      'src/renderer/lib/ipc/http.ts'
    ],
    api
  )
}

const ev = (action: string, ...args: unknown[]): VectorEvent => ({ action, args })
const CWD = 'C:\\Users\\a b\\repo'

export const gitRoutesScenarios: Scenario[] = [
  {
    name: 'reads',
    steps: [
      ev('getStatus', CWD),
      ev('getDiffStat', CWD),
      ev('getDiff', CWD, 'src/a.ts', true),
      ev('getCommitContext', CWD),
      ev('getLog', CWD, 1),
      ev('stashList', CWD),
      ev('branchList', CWD)
    ]
  },
  {
    name: 'writes',
    steps: [
      ev('stage', CWD, 'a.ts'),
      ev('unstage', CWD, 'a.ts'),
      ev('discard', CWD, 'a.ts'),
      ev('stageHunk', CWD, 'a.ts', '@@ -1 +1 @@\n-a\n+b\n'),
      ev('unstageHunk', CWD, 'a.ts', '@@ -1 +1 @@\n-a\n+b\n'),
      ev('commit', CWD, 'fix: x', 'body', false, 'abc123'),
      ev('commit', CWD, 'first', '', true, ''),
      ev('push', CWD),
      ev('stashSave', CWD, 'wip', true),
      ev('stashSave', CWD),
      ev('stashApply', CWD, 0),
      ev('stashPop', CWD, 1),
      ev('stashDrop', CWD, 2),
      ev('branchSwitch', CWD, 'feature/x'),
      ev('createBranch', CWD, 'feature/y')
    ]
  }
]
