/**
 * The R9d GitHub lane. The seeded world has no GitHub remote and no `gh` login, so each page
 * answers `/github/*` from a recorded fixture (`page.route`); nothing reaches GitHub. In
 * alpha it opens the GitHub panel, opens pull request #7, reads its review threads and
 * checks, posts a comment, then opens and cancels the merge confirm. The lane fails unless
 * both clients show the same thread and timeline counts and sent the same writes: one
 * comment, and no merge.
 *
 *   bun tools/parity/lanes/github.ts --port 8193 --root /tmp/rune-r9d/lane \
 *     --server-bin <troupe-server> --dist <dist-web> [--phone]
 *
 * Writes `<client>-<step>.png` and `pair-<step>.png` under the root.
 */

import path from 'node:path'
import { parseArgs } from 'node:util'
import { type Browser, chromium, type Page } from 'playwright'
import { REFERENCE_ARGS } from '../drivers/chromium'
import { runeWebArgs } from '../drivers/rune-web'
import { REPO_DIR } from '../server'
import { createWorld } from '../world'
import { type Client, inReact, inRune, type LaneOptions, shot, writePairs } from './kit'

const { values } = parseArgs({
  args: Bun.argv.slice(2),
  options: {
    port: { type: 'string', default: '8193' },
    root: { type: 'string', default: '/tmp/troupe-parity/github' },
    'server-bin': { type: 'string' },
    dist: { type: 'string', default: path.join(REPO_DIR, 'dist-web') },
    only: { type: 'string' },
    phone: { type: 'boolean', default: false }
  }
})
const options: LaneOptions = {
  root: path.resolve(values.root as string),
  phone: values.phone as boolean,
  dist: path.resolve(values.dist as string),
  device: 'github'
}
const STEPS = ['list', 'item', 'threads', 'checks', 'commented', 'merge-confirm', 'menu']
const TITLE = 'Fix the login crash'
const COMMENT = 'Looks good to me'

const NOW = new Date(Date.now() - 3 * 3600_000).toISOString()
const REPO = { host: 'github.com', owner: 'acme', name: 'app' }
const label = (name: string, color: string) => ({ name, color })
const listItem = (number: number, title: string, over: Record<string, unknown> = {}) => ({
  kind: 'pr',
  number,
  title,
  state: 'open',
  author: 'ann',
  labels: [label('bug', 'd73a4a')],
  assignees: [],
  commentCount: 1,
  updatedAt: NOW,
  url: `https://github.com/acme/app/pull/${number}`,
  checks: 'failure',
  ...over
})
const comment = (id: string, author: string, body: string, isBot = false) => ({
  id,
  author,
  isBot,
  body,
  createdAt: NOW,
  url: `https://github.com/acme/app/pull/7#${id}`
})

/** The fixture host: what `/github/*` answers, and the writes each page sent. */
function fixture() {
  const comments = [comment('C1', 'bob', 'Can you add a test for the empty password case?')]
  const writes: Array<{ path: string; body: unknown }> = []
  const detail = () => ({
    repo: REPO,
    item: listItem(7, TITLE, { commentCount: comments.length }),
    body: 'Login crashed when the password field was empty.\n\n- adds a guard\n- adds a test',
    createdAt: NOW,
    comments: [
      ...comments,
      comment('B1', 'coderabbit', '<!-- summary -->**Walkthrough** The guard is fine.', true)
    ],
    timeline: [
      {
        id: 'E1',
        kind: 'labeled',
        actor: 'ann',
        createdAt: NOW,
        label: label('bug', 'd73a4a'),
        assignee: null
      },
      { id: 'E2', kind: 'assigned', actor: 'ann', createdAt: NOW, label: null, assignee: 'ann' }
    ],
    stateReason: null,
    pr: {
      headRefName: 'fix/login-crash',
      headSha: 'abc1234',
      baseRefName: 'main',
      isCrossRepository: false,
      maintainerCanModify: true,
      mergeable: 'mergeable',
      mergeStateStatus: 'CLEAN',
      autoMergeEnabled: false,
      additions: 24,
      deletions: 3,
      changedFiles: 2,
      reviewThreads: [
        {
          id: 'T1',
          path: 'src/login.ts',
          line: 42,
          isResolved: false,
          isOutdated: false,
          comments: [comment('R1', 'bob', 'Should this return early?')]
        },
        {
          id: 'T2',
          path: 'src/login.test.ts',
          line: null,
          isResolved: true,
          isOutdated: true,
          comments: [comment('R2', 'carol', 'Nit: rename the case.')]
        }
      ]
    },
    fetchedAt: NOW
  })
  const answers: Record<string, () => unknown> = {
    '/github/context': () => ({
      auth: {
        state: 'connected',
        host: 'github.com',
        account: 'ann',
        scopes: ['repo'],
        tokenSource: 'keyring',
        missingScopes: [],
        fixCommands: []
      },
      repo: { kind: 'resolved', origin: REPO, upstream: null }
    }),
    '/github/items': () => ({
      repo: REPO,
      items: [
        listItem(7, TITLE, { commentCount: comments.length }),
        listItem(5, 'Session expires too early', { kind: 'issue', checks: null, labels: [] })
      ],
      fetchedAt: NOW
    }),
    '/github/item': detail,
    '/github/checks': () => ({
      headSha: 'abc1234',
      rollup: 'failure',
      checks: [
        { name: 'test (ubuntu)', workflow: 'CI', status: 'failure', detailsUrl: null, jobId: 91 },
        { name: 'lint', workflow: 'CI', status: 'success', detailsUrl: null, jobId: null },
        { name: 'typecheck', workflow: 'CI', status: 'success', detailsUrl: null, jobId: null }
      ],
      fetchedAt: NOW
    }),
    '/github/repo-meta': () => ({
      repo: REPO,
      labels: [{ name: 'bug', color: 'd73a4a', description: "Something isn't working" }],
      assignableUsers: ['ann', 'bob'],
      viewer: 'ann',
      fetchedAt: NOW
    })
  }
  return {
    writes,
    async attach(page: Page) {
      await page.route('**/github/**', async (route) => {
        const url = new URL(route.request().url())
        const body = route.request().postDataJSON()
        const read = answers[url.pathname]
        let data: unknown
        if (read) data = read()
        else {
          writes.push({ path: url.pathname, body })
          if (url.pathname === '/github/comment') {
            const posted = comment(`P${comments.length}`, 'ann', body.body)
            comments.push(posted)
            data = { comment: posted }
          } else data = { number: 7 }
        }
        await route.fulfill({ json: data })
      })
    }
  }
}

interface Seen {
  seen: string[]
  threads: string
  writes: string
}

async function run(client: Client): Promise<Seen> {
  const host = fixture()
  await host.attach(client.page)
  const seen: string[] = []
  const step = async (name: string, check: () => Promise<boolean>) => {
    const ok = await check().catch(() => false)
    if (ok) seen.push(name)
    else console.log(`${client.name}: ${name} failed its check`)
    await shot(client, options.root, `${STEPS.indexOf(name) + 1}-${name}`)
  }
  const act = async (what: string, fn: () => Promise<void>) => {
    try {
      await fn()
    } catch (error) {
      console.log(`${client.name}: ${what} failed: ${error}`)
    }
  }
  const settle = (ms = 1200) => client.page.waitForTimeout(ms)
  await client.sees('What should we do in alpha?', 30_000)
  if (options.phone) {
    await act('drawer', () => client.click('Open projects and chats', false))
    await settle(800)
  }
  await act('open GitHub', () => client.click('GitHub'))
  await step('list', () => client.sees(TITLE, 20_000))
  await act('open item', () => client.click(TITLE, false))
  await settle(1500)
  await step('item', () => client.sees('Login crashed when the password field was empty.', 20_000))
  await act('threads', () => client.click('Threads', false))
  await settle()
  const threads = (await client.sees('2 unresolved of 2', 1500))
    ? '2/2'
    : (await client.sees('1 unresolved of 2', 15_000))
      ? '1/2'
      : 'none'
  await step('threads', async () => threads === '1/2')
  await act('checks', () => client.click('Checks', false))
  await settle()
  await step('checks', () => client.sees('test (ubuntu)', 15_000))
  await act('conversation', () => client.click('Conversation', false))
  await settle()
  await act('comment', async () => {
    if (client.name === 'react') await client.fill('Add a comment', COMMENT)
    else {
      await client.click('Add a comment')
      await client.page.keyboard.type(COMMENT, { delay: 15 })
    }
  })
  await act('post', () => client.click('Comment'))
  await settle(1500)
  await step('commented', () => client.sees(COMMENT, 15_000))
  if (!options.phone) {
    await act('merge', () => client.click('Merge'))
    await settle(800)
    await step('merge-confirm', () => client.sees('Merge pull request #7?', 10_000))
    await act('cancel', () => client.click('Cancel'))
    await settle(800)
  }
  await act('menu', () => client.click('Pull request actions'))
  await settle(800)
  await step('menu', () => client.sees('Copy link', 10_000))
  await client.page.keyboard.press('Escape')
  return {
    seen,
    threads,
    writes: JSON.stringify(host.writes)
  }
}

const world = await createWorld({
  port: Number(values.port),
  root: options.root,
  binary: values['server-bin']
})
let browser: Browser | undefined
try {
  browser = await chromium.launch({ args: runeWebArgs(REFERENCE_ARGS) })
  const results: Array<[string, Seen]> = []
  for (const name of ['react', 'rune'] as const) {
    if (values.only && values.only !== name) continue
    const runner = name === 'react' ? inReact : inRune
    results.push([name, await runner(browser, world, options, run)])
  }
  const want = STEPS.filter((s) => !(options.phone && s === 'merge-confirm'))
  let pass = true
  for (const [name, { seen, threads, writes }] of results) {
    const ok = want.every((s) => seen.includes(s))
    pass &&= ok
    console.log(`${name}: ${ok ? 'PASS' : 'FAIL'} ${JSON.stringify(seen)} threads=${threads}`)
    console.log(`${name} writes: ${writes}`)
  }
  if (results.length === 2) {
    const [a, b] = [results[0][1], results[1][1]]
    const same = a.threads === b.threads && a.writes === b.writes
    pass &&= same
    console.log(
      same
        ? 'PASS: both clients showed the same threads and sent the same writes'
        : 'FAIL: the clients differ'
    )
  }
  process.exitCode = pass ? 0 : 1
} finally {
  try {
    writePairs(options.root)
  } finally {
    try {
      await browser?.close()
    } finally {
      try {
        world.ws.close()
      } finally {
        await world.server.stop()
      }
    }
  }
}
