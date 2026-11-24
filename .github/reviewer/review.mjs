// One-shot PR reviewer for GitHub Actions. Uses the Claude Agent SDK: the
// agent loop runs on the runner, reads the checked-out PR head, and the model
// calls bill to the Anthropic API key. Read-only tools, no shell.
//
// Env:
//   ANTHROPIC_API_KEY    Anthropic API key (secret).
//   ANTHROPIC_API_KEY_2  Fallback key (secret, optional). Used when the first key
//                        is missing or the provider refuses it before any work
//                        (bad key, billing, rate limit, outage).
//   GITHUB_TOKEN     Token with pull-requests: write (only needed to post).
//   REPO             owner/name.
//   PR_NUMBER        Pull request number.
//   HEAD_SHA         PR head commit (the checkout).
//   BASE_SHA         PR base commit. With full history in WORKSPACE, the diff is
//                    built locally with 15 lines of context; else from the API.
//   WORKSPACE        Checkout path. Default: cwd.
//   DRY_RUN=1        Print the review instead of posting it.
//   REVIEW_MODEL     Model id. Default claude-sonnet-5-5.
//   REVIEW_EFFORT    low | medium | high | xhigh | max. Default high.
//   REVIEW_TIMEOUT_MS  Hard cap for the whole review. Default 1_500_000 (25 min).
//   PROMPT_ONLY=1    Print the run section and prompt size, then exit.
//   TOOL_BUDGET      Exploration tool calls before a forced wrap-up. 0 = no budget (default).
//
// Exploration stops at TOOL_BUDGET tool calls or at 70% of the timeout, then
// a follow-up turn on the same session asks for post_findings with what it has.
// When the provider fails before any tool call or token on every key, the run
// prints a ::warning:: with the cause and exits 0.
import { execFileSync } from 'node:child_process'
import { realpathSync } from 'node:fs'
import { appendFile, readFile, writeFile } from 'node:fs/promises'
import { dirname, isAbsolute, join, relative, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { createSdkMcpServer, query, tool } from '@anthropic-ai/claude-agent-sdk'
import { z } from 'zod'
import { hasBotReview, REVIEWS_PER_PAGE, reviewOutcome, shouldTryNextKey } from './reviews.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const MAX_DIFF_CHARS = 300_000
const SEVERITIES = new Set(['high', 'medium', 'low'])
const POST_TOOL = 'mcp__thermo__post_findings'

const env = (name, fallback) => {
  const value = process.env[name]
  if (value === undefined || value === '') {
    if (fallback !== undefined) return fallback
    throw new Error(`missing env ${name}`)
  }
  return value
}

const apiKeys = ['ANTHROPIC_API_KEY', 'ANTHROPIC_API_KEY_2']
  .map((name) => ({ name, value: env(name, '') }))
  .filter((k) => k.value !== '')
if (apiKeys.length === 0) throw new Error('missing env ANTHROPIC_API_KEY')
const repo = env('REPO')
const prNumber = Number(env('PR_NUMBER'))
const headSha = env('HEAD_SHA')
const baseSha = process.env.BASE_SHA ?? ''
const workspace = env('WORKSPACE', process.cwd())
const root = realpathSync(workspace)
const dryRun = env('DRY_RUN', '0') === '1'
const model = env('REVIEW_MODEL', 'claude-sonnet-5-5')
const effort = env('REVIEW_EFFORT', 'high')
const timeoutMs = Number(env('REVIEW_TIMEOUT_MS', '1500000'))
const toolBudget = Number(env('TOOL_BUDGET', '0'))
const githubToken = process.env.GITHUB_TOKEN ?? ''
const marker = `<!-- termul-reviewer:${headSha} -->`

// Keep secrets out of the agent's environment and out of any output. The
// agent process gets a minimal env with only the key it runs with.
const secrets = [...apiKeys.map((k) => k.value), githubToken].filter((s) => s.length >= 8)
for (const name of [
  'ANTHROPIC_API_KEY',
  'ANTHROPIC_API_KEY_2',
  'ANTHROPIC_AUTH_TOKEN',
  'GITHUB_TOKEN',
  'GH_TOKEN'
]) {
  delete process.env[name]
}
const redact = (text) => secrets.reduce((t, s) => t.split(s).join('[redacted]'), text)

async function gh(path, init = {}) {
  const response = await fetch(`https://api.github.com${path}`, {
    ...init,
    headers: {
      accept: 'application/vnd.github+json',
      authorization: `Bearer ${githubToken}`,
      'x-github-api-version': '2022-11-28',
      ...init.headers
    }
  })
  if (!response.ok) {
    throw new Error(
      `GitHub ${init.method ?? 'GET'} ${path}: ${response.status} ${await response.text()}`
    )
  }
  return response
}

async function alreadyReviewed() {
  if (githubToken === '') return false
  const fetchPage = async (page) => {
    const path = `/repos/${repo}/pulls/${prNumber}/reviews?per_page=${REVIEWS_PER_PAGE}&page=${page}`
    return (await gh(path)).json()
  }
  return hasBotReview(fetchPage, marker)
}

async function loadDiff() {
  const response = await gh(`/repos/${repo}/pulls/${prNumber}`, {
    headers: { accept: 'application/vnd.github.diff' }
  })
  const text = await response.text()
  return text.length > MAX_DIFF_CHARS ? `${text.slice(0, MAX_DIFF_CHARS)}\n…[truncated]\n` : text
}

const git = (...args) =>
  execFileSync('git', args, { cwd: workspace, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 })

// Diff with wide context from the local checkout. Falls back to the API diff.
async function loadContextDiff() {
  if (baseSha !== '') {
    try {
      const mergeBase = git('merge-base', baseSha, headSha).trim()
      for (const unified of [15, 8, 3]) {
        const text = git('diff', `-U${unified}`, '--no-color', mergeBase, headSha)
        if (text.length <= MAX_DIFF_CHARS) return { text, unified }
      }
    } catch (error) {
      console.log(
        `local diff unavailable (${String(error.message).split('\n')[0]}); using API diff`
      )
    }
  }
  return { text: await loadDiff(), unified: 3 }
}

// Changed files, new-file line ranges of each hunk, and status.
function parseDiff(text) {
  const files = new Map()
  let current = null
  for (const line of text.split('\n')) {
    const header = /^diff --git a\/(.+?) b\/(.+)$/.exec(line)
    if (header) {
      current = { path: header[2], ranges: [], deleted: false }
      files.set(current.path, current)
      continue
    }
    if (current === null) continue
    if (line.startsWith('+++ /dev/null')) current.deleted = true
    const hunk = /^@@ -\d+(?:,\d+)? \+(\d+)(?:,(\d+))? @@/.exec(line)
    if (hunk) {
      const start = Number(hunk[1])
      const count = hunk[2] === undefined ? 1 : Number(hunk[2])
      current.ranges.push([start, start + Math.max(count, 1) - 1])
    }
  }
  return files
}

const SOURCE_RE = /\.(tsx?|jsx?|mjs|rs)$/
const TEST_RE = /(\.test\.|\.spec\.|__tests__|\/tests?\/)/

// Direct imports of each changed file and files that import it (via ripgrep).
function relatedFiles(files) {
  const changed = [...files.values()].filter(
    (f) => !f.deleted && SOURCE_RE.test(f.path) && !TEST_RE.test(f.path)
  )
  const lines = []
  for (const f of changed) {
    let imports = []
    try {
      const src = git('show', `${headSha}:${f.path}`)
      imports = [...src.matchAll(/from\s+['"]((?:@\/|\.)[^'"]+)['"]/g)].map((m) => m[1])
      imports = [...new Set(imports)].slice(0, 15)
    } catch {}
    const stem = f.path.replace(/^.*\//, '').replace(SOURCE_RE, '')
    let callers = []
    if (/\.(tsx?|jsx?|mjs)$/.test(f.path) && stem !== 'index') {
      try {
        // git grep is on every runner; ripgrep is not.
        callers = git('grep', '-l', '-E', `from ['"][^'"]*/${stem}['"]`, '--', 'src')
          .split('\n')
          .filter((p) => p !== '' && p !== f.path && !TEST_RE.test(p))
          .slice(0, 8)
      } catch {}
    }
    lines.push(
      `- ${f.path}` +
        (imports.length ? `\n  imports: ${imports.join(', ')}` : '') +
        (callers.length ? `\n  imported by: ${callers.join(', ')}` : '')
    )
  }
  return lines.join('\n')
}

const flatten = (v) => String(v).replace(/\s+/g, ' ').trim()
const skipReview = (message) => console.log(`::warning::${redact(flatten(message))}`)

function reviewBody(findings) {
  const verdict = findings.length === 0 ? 'CLEAN' : 'FINDINGS'
  const lines = ['## Thermo review', '', `**Verdict:** ${verdict}`]
  for (const f of findings) {
    lines.push(`- **${f.severity}** \`${flatten(f.file)}:${f.line}\` — ${flatten(f.title)}`)
    lines.push(`  ${flatten(f.scenario)} ${flatten(f.detail)}`)
  }
  lines.push('', marker)
  return redact(lines.join('\n'))
}

function validFinding(f) {
  return (
    f !== null &&
    typeof f === 'object' &&
    SEVERITIES.has(f.severity) &&
    typeof f.file === 'string' &&
    f.file.length > 0 &&
    f.file.length <= 240 &&
    Number.isInteger(f.line) &&
    f.line > 0 &&
    typeof f.title === 'string' &&
    f.title.length > 0 &&
    f.title.length <= 160 &&
    typeof f.scenario === 'string' &&
    f.scenario.length > 0 &&
    f.scenario.length <= 400 &&
    typeof f.detail === 'string' &&
    f.detail.length > 0 &&
    f.detail.length <= 280
  )
}

async function main() {
  if (!dryRun && (await alreadyReviewed())) {
    console.log(`Already reviewed ${repo}#${prNumber} at ${headSha}. Skipping.`)
    return
  }

  const instructions = await readFile(join(HERE, 'instructions.md'), 'utf8')
  const { text: diff, unified } = await loadContextDiff()
  const changedFiles = parseDiff(diff)
  const related = relatedFiles(changedFiles)
  let findings

  // A finding must sit inside a hunk of a changed file (new-file lines).
  const inChangedHunk = (f) => {
    const file = changedFiles.get(f.file.replace(/^\.\//, ''))
    return file?.ranges.some(([lo, hi]) => f.line >= lo && f.line <= hi) ?? false
  }

  const prompt = [
    instructions,
    '',
    '## This run',
    `Pull request ${repo}#${prNumber}. Your working directory is the repository checked out at the PR head (${headSha}).`,
    'Steps:',
    '1. Read the diff and the file map below. They are your main context.',
    '2. List the suspected bugs to yourself, riskiest first.',
    `3. Confirm each by reading only the lines you need in a changed file or a file from the map.${toolBudget > 0 ? ` Budget: about ${toolBudget} Read/Grep/Glob calls in total.` : ''} No repo-wide searches.`,
    '4. Call post_findings once with the confirmed findings only.',
    'No shell. Do not build, run, or test anything.',
    '',
    '## Changed files, their direct imports, and files that import them',
    related || '(none)',
    '',
    `<diff context_lines="${unified}">`,
    diff,
    '</diff>'
  ].join('\n')

  if (process.env.PROMPT_ONLY === '1') {
    console.log(prompt.slice(prompt.indexOf('## This run'), prompt.indexOf('<diff')))
    console.log(
      `prompt_chars=${prompt.length} diff_chars=${diff.length} context_lines=${unified} files=${changedFiles.size}`
    )
    return
  }
  const startedAt = Date.now()
  const deadline = startedAt + timeoutMs
  let tools = 0
  let sawOutput = false
  let lastResult

  // post_findings lives in an in-process MCP server. A fresh server per query,
  // since a server instance connects to one session only.
  const findingsServer = () =>
    createSdkMcpServer({
      name: 'thermo',
      version: '1.0.0',
      tools: [
        tool(
          'post_findings',
          'Record the review for this pull request. Call once. Empty findings = CLEAN.',
          {
            findings: z.array(
              z.object({
                severity: z.enum(['high', 'medium', 'low']),
                file: z.string(),
                line: z.number().int().min(1),
                title: z.string().max(160),
                scenario: z.string().max(400),
                detail: z.string().max(280)
              })
            )
          },
          async ({ findings: raw }) => {
            const reply = (text, isError = false) => ({
              content: [{ type: 'text', text }],
              isError
            })
            if (!Array.isArray(raw) || !raw.every(validFinding)) {
              return reply('Invalid findings. Check the schema and call again.', true)
            }
            const outside = raw.filter((f) => !inChangedHunk(f))
            if (outside.length > 0) {
              const where = outside.map((f) => `${f.file}:${f.line}`).join(', ')
              return reply(
                `These are not on lines this PR changed: ${where}. Point each finding at the changed line that causes it, or drop it, then call again.`,
                true
              )
            }
            findings = raw
            return reply('Recorded. Stop now.')
          }
        )
      ]
    })

  // Reads stay inside the checkout. Paths are resolved through symlinks, so a
  // PR cannot add a link that points at /proc or the runner's home.
  const insideRoot = (target) => {
    let real = resolve(root, target)
    try {
      real = realpathSync(real)
    } catch {}
    const rel = relative(root, real)
    return rel === '' || (!rel.startsWith('..') && !isAbsolute(rel))
  }
  const guardPaths = async (input) => {
    const args = input.tool_input ?? {}
    const target = input.tool_name === 'Read' ? args.file_path : args.path
    if (target === undefined || (typeof target === 'string' && insideRoot(target))) return {}
    console.log(`[deny] ${input.tool_name} outside the checkout`)
    return {
      hookSpecificOutput: {
        hookEventName: 'PreToolUse',
        permissionDecision: 'deny',
        permissionDecisionReason: 'Only files inside the repository checkout can be read.'
      }
    }
  }

  const options = (key) => ({
    cwd: root,
    model,
    effort,
    thinking: { type: 'adaptive' },
    tools: ['Read', 'Grep', 'Glob'],
    allowedTools: ['Read', 'Grep', 'Glob', POST_TOOL],
    permissionMode: 'dontAsk',
    settingSources: [],
    strictMcpConfig: true,
    mcpServers: { thermo: findingsServer() },
    hooks: {
      PreToolUse: [{ matcher: 'Read|Grep|Glob', hooks: [guardPaths], onFailure: 'block' }]
    },
    env: {
      PATH: process.env.PATH,
      HOME: process.env.HOME,
      TMPDIR: process.env.RUNNER_TEMP ?? process.env.TMPDIR,
      ANTHROPIC_API_KEY: key,
      CLAUDE_AGENT_SDK_CLIENT_APP: 'troupe-thermo-review/1.0',
      CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC: '1'
    },
    stderr: (data) => console.log(`[cli] ${redact(flatten(data))}`)
  })

  const textOf = (message) =>
    (message.message?.content ?? [])
      .filter((b) => b.type === 'text')
      .map((b) => b.text)
      .join(' ')

  // One query. Aborts when `stopAt` passes or, if `budget` is set, when the
  // turn has used that many exploration tool calls. `resume` continues a
  // session from an earlier turn.
  async function turn(message, { key, stopAt, budget, resume }) {
    const abortController = new AbortController()
    let stopped = null
    const stop = (why) => {
      if (stopped !== null) return
      stopped = why
      console.log(`[stop] ${why}`)
      abortController.abort()
    }
    const timer = setTimeout(() => stop('time'), Math.max(0, stopAt - Date.now()))
    let used = 0
    let error
    let result
    let sessionId = resume
    try {
      const run = query({ prompt: message, options: { ...options(key), abortController, resume } })
      for await (const msg of run) {
        if (msg.type === 'system' && msg.subtype === 'init') {
          sessionId = msg.session_id
          console.log(`[init] model=${msg.model} tools=${msg.tools.join(',')}`)
        } else if (msg.type === 'system' && msg.subtype === 'api_retry') {
          console.log(
            `[retry] attempt=${msg.attempt}/${msg.max_retries} status=${msg.error_status} error=${msg.error}`
          )
        } else if (msg.type === 'assistant' && msg.parent_tool_use_id === null) {
          if (msg.error !== undefined) {
            error = { code: msg.error, message: redact(flatten(textOf(msg) || msg.error)) }
            console.log(`[error] ${error.code}: ${error.message}`)
            continue
          }
          sawOutput = true
          for (const block of msg.message.content) {
            if (block.type !== 'tool_use') continue
            tools += 1
            used += 1
            console.log(`[tool] ${block.name}`)
            if (budget !== undefined && used >= budget && block.name !== POST_TOOL) {
              stop('tool budget')
            }
          }
        } else if (msg.type === 'result') {
          result = msg
          lastResult = msg
        }
      }
    } catch (thrown) {
      // An abort we asked for, or a CLI exit after it reported a provider
      // error, ends the turn. Anything else is a bug.
      if (stopped === null && error === undefined) throw thrown
      if (stopped === null) console.log(`[turn] query ended: ${redact(flatten(thrown.message))}`)
    } finally {
      clearTimeout(timer)
    }

    let status = 'finished'
    if (stopped !== null) status = 'cancelled'
    else if (error !== undefined) status = 'error'
    else if (result === undefined || result.is_error || result.subtype !== 'success') {
      status = 'error'
      if (result?.subtype !== undefined && result.subtype !== 'success') {
        error = { code: result.subtype, message: redact(flatten(result.errors.join('; '))) }
      } else {
        error = { code: 'unknown', message: redact(flatten(result?.result ?? 'no result')) }
      }
    }
    console.log(`[turn] status=${status}${error ? ` error=${error.code}: ${error.message}` : ''}`)
    return { status, error, sessionId }
  }

  const tokenTotals = () => {
    const totals = { inputTokens: 0, outputTokens: 0, cacheReadTokens: 0, cacheWriteTokens: 0 }
    for (const usage of Object.values(lastResult?.modelUsage ?? {})) {
      totals.inputTokens += usage.inputTokens ?? 0
      totals.outputTokens += usage.outputTokens ?? 0
      totals.cacheReadTokens += usage.cacheReadInputTokens ?? 0
      totals.cacheWriteTokens += usage.cacheCreationInputTokens ?? 0
    }
    return totals
  }

  // A full review with one key: explore, then a wrap-up turn if needed.
  async function reviewWith(key) {
    tools = 0
    sawOutput = false
    lastResult = undefined
    let result = await turn(prompt, {
      key,
      stopAt: startedAt + timeoutMs * 0.7,
      budget: toolBudget > 0 ? toolBudget : undefined
    })
    // Asking again after an error that came before any tool call only repeats it.
    if (
      findings === undefined &&
      result.sessionId !== undefined &&
      !(result.status === 'error' && tools === 0)
    ) {
      console.log('[wrap-up] asking for post_findings now')
      result = await turn(
        'Your tool budget is spent. Do not call Read, Grep, or Glob again. Call post_findings now with every bug you have concrete evidence for from the diff and the code you already read, each on its changed line with a concrete scenario. Drop only items you now think are wrong or are style.',
        { key, stopAt: deadline, resume: result.sessionId }
      )
    }
    const usage = tokenTotals()
    const tokens = usage.inputTokens + usage.outputTokens
    return {
      status: result.status,
      error: result.error,
      tools,
      tokens: tokens > 0 || !sawOutput ? tokens : 1,
      usage
    }
  }

  let run
  for (const [index, key] of apiKeys.entries()) {
    console.log(`[auth] using ${key.name}`)
    run = await reviewWith(key.value)
    if (index === apiKeys.length - 1 || !shouldTryNextKey({ ...run, findings })) break
    console.log(`[auth] ${key.name} failed (${run.error.code}); trying ${apiKeys[index + 1].name}`)
  }
  const elapsed = Date.now() - startedAt
  const { usage } = run

  console.log(`status=${run.status} elapsed_ms=${elapsed} tools=${run.tools} model=${model}`)
  console.log(
    `tokens in=${usage.inputTokens} out=${usage.outputTokens} cache_read=${usage.cacheReadTokens} cache_write=${usage.cacheWriteTokens}`
  )
  if (lastResult !== undefined) console.log(`cost estimated_usd=${lastResult.total_cost_usd}`)
  if (process.env.REVIEW_METRICS_FILE) {
    await writeFile(
      process.env.REVIEW_METRICS_FILE,
      JSON.stringify(
        { status: run.status, elapsedMs: elapsed, tools: run.tools, usage, findings },
        null,
        2
      )
    )
  }

  const outcome = reviewOutcome({
    status: run.status,
    error: run.error,
    tools: run.tools,
    tokens: run.tokens,
    findings
  })
  if (outcome.kind === 'skip') {
    skipReview(outcome.message)
    return
  }
  if (outcome.kind === 'fail') throw new Error(outcome.message)
  const body = reviewBody(findings)
  if (dryRun) {
    console.log('--- DRY RUN: review not posted ---')
    console.log(body)
    return
  }
  const response = await gh(`/repos/${repo}/pulls/${prNumber}/reviews`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ commit_id: headSha, body, event: 'COMMENT' })
  })
  const review = await response.json()
  console.log(`Posted review: ${review.html_url}`)
  if (process.env.GITHUB_STEP_SUMMARY) {
    await appendFile(process.env.GITHUB_STEP_SUMMARY, `${body}\n`)
  }
}

main().catch((error) => {
  console.error(redact(String(error?.stack ?? error)))
  process.exit(1)
})
