/**
 * The rest of the terminal vectors (`terminal.ts` has the walks): how completion output is
 * spelled, which text the editor and the PTY refuse, the PTY guard, the command editor's
 * session calls on `/terminal/ws`, and the command history.
 */

import {
  containsEditorDisallowedControls,
  sanitizeEditorPaste
} from '../../../src/renderer/components/terminal/command-editor/editor-keys'
import { createWebCommandEditorTransport } from '../../../src/renderer/lib/command-editor-transport'
import {
  containsDisallowedControls,
  parseCompletionOutput,
  wordContinuesPastCursor
} from '../../../src/renderer/lib/command-lifecycle/completion-output'
import { canWriteToPty } from '../../../src/renderer/lib/command-lifecycle/input-guard'
import { trimListToCap } from '../../../src/renderer/lib/persistence-trim'
import { useCommandHistoryStore } from '../../../src/renderer/stores/command-history-store'
import { drainLogs, MODES, PHASES } from './terminal'

// ---- words, controls and the PTY guard ----------------------------------------------------

const hexLine = (text: string) =>
  Array.from(new TextEncoder().encode(text), (b) => b.toString(16).padStart(2, '0')).join('')

export const WORD_DRAFTS = [
  'git sta',
  'git ',
  'cat ',
  'cat "di',
  "cat 'di",
  'cat "dir"\\ w',
  'cat "plain',
  'ls dir\\ w',
  'cd "a b" "dir w',
  "ls 'x y' dir",
  'x ',
  "cat ~/$'my f",
  'cat ~"/Doc',
  'cat "~/Doc',
  "cat $'a' b",
  "cat \\$'a\\' b",
  "cat $'d",
  "cat 'a'b",
  'cat report{',
  'cat {a,',
  'cat a#',
  'cat "a\\$',
  "cat $'a\\\\",
  'cat a$',
  'cat "a$',
  'cat a\\',
  'cat hello!',
  'cat "hello!',
  'cat a&',
  'cat a*',
  'cat a$b',
  'cat {a}',
  'cat =ec',
  'cat \\=ec',
  "cat '=ec",
  'cat a',
  "cat $'dir\\x20w",
  "cat $'dir\\040w",
  "cat $'dir\\u0020w",
  "cat $'caf\\U000000e9",
  "cat $'caf\\xc3\\xa9",
  "cat $'caf\\303\\251'\\ m",
  "cat $'\\41",
  "cat $'\\u3b1",
  "cat $'a\\q",
  "cat $'a\\x",
  "cat $'caf\\xc3",
  'cat $"a b"',
  'cat $"x',
  'echo café ~/Doc',
  'ls 😀 dir',
  'a\nb c'
]

export const CANDIDATES = [
  'status',
  'stash',
  'dir with space/',
  'file one.txt',
  'plain',
  "it's",
  'a&b',
  '#tag',
  '--color=auto',
  "~/my file's",
  '~/Documents/',
  "a' bc",
  'd\\ir/',
  'ab c',
  'report{a,b}.txt',
  '{a,b}',
  'a#b c',
  'a$b c',
  'a\\b c',
  'a$b.txt',
  'a$&b',
  'a b',
  'hello!world.txt',
  'a*b',
  'a$bc',
  '{a}b',
  '=echo',
  'a=b',
  'dir with space',
  'café menu',
  '!7',
  'αb',
  'αz',
  'a\\qb',
  'a\\xb',
  'café',
  'y z',
  '😀 dir'
]

const OUTPUT = [
  ...CANDIDATES.map(hexLine),
  'not-hex',
  'abc',
  `efbbbf${hexLine('bom')}`,
  hexLine('status\x1b[201~'),
  hexLine('stash\rinjected'),
  hexLine('hidden​name'),
  hexLine('bidi‮name'),
  hexLine('status'),
  `${hexLine('crlf')}\r`,
  'ff',
  ''
].join('\n')

const CONTROL_SAMPLES = [
  'plain',
  'a\nb',
  'a\tb',
  'a\r\nb',
  'a\rb',
  '\x1b[200~pasted\x1b[201~',
  '\x03',
  '\x7f',
  '\u0085',
  'zwnj‌',
  'zwj‍',
  'zw​sp',
  'bidi‮',
  'iso⁦',
  '﻿bom',
  'tag\u{e0041}',
  'emoji 😀',
  'café',
  '­',
  '؜',
  '᠎',
  '\u{110bd}'
]

export function recordWords() {
  const parse = []
  for (const draft of WORD_DRAFTS) {
    const units = draft.length
    const cursors = [
      ...new Set([units, Math.floor(units / 2), ...(units % 4 === 0 ? [units + 3] : [])])
    ]
    for (const cursor of cursors) {
      parse.push({ draft, cursor, rows: parseCompletionOutput(draft, cursor, OUTPUT) })
    }
  }
  const continues = WORD_DRAFTS.map((draft) => ({
    draft,
    continues: Array.from({ length: draft.length + 1 }, (_, cursor) =>
      wordContinuesPastCursor(draft, cursor)
    )
  }))
  const controls = CONTROL_SAMPLES.map((text) => ({
    text,
    pty: containsDisallowedControls(text),
    editor: containsEditorDisallowedControls(text),
    paste: sanitizeEditorPaste(text)
  }))
  const guard = []
  for (const mode of MODES) {
    for (const phase of PHASES) {
      for (const data of ['\x03', 'a', '', '\x04']) {
        guard.push({ mode, phase, data, write: canWriteToPty({ mode, phase }, data) })
      }
    }
  }
  return { output: OUTPUT, parse, continues, controls, guard }
}

// ---- the command editor's session calls on `/terminal/ws` ---------------------------------

interface Reply {
  success: boolean
  data?: unknown
  code?: string
}

type RouteCall =
  | { call: 'submit'; terminalId: string; draft: string }
  | {
      call: 'requestCompletion'
      terminalId: string
      request: {
        requestId: string
        draft: string
        bufferRevision: number
        cursor: number
        promptGeneration: number
        cwd?: string
      }
    }
  | { call: 'cancel'; terminalId: string; requestId: string }
  | { call: 'restore'; terminalId: string }

const ROUTE_CALLS: RouteCall[] = [
  { call: 'submit', terminalId: 'pty-1', draft: 'git status' },
  { call: 'submit', terminalId: 'pty-1', draft: 'echo "a\nb" \\\n  && ls' },
  { call: 'submit', terminalId: 'pty-1', draft: 'echo café 😀' },
  {
    call: 'requestCompletion',
    terminalId: 'pty-1',
    request: {
      requestId: 'completion-1',
      draft: 'git st',
      bufferRevision: 3,
      cursor: 6,
      promptGeneration: 2,
      cwd: '/repo'
    }
  },
  {
    call: 'requestCompletion',
    terminalId: 'pty-1',
    request: {
      requestId: 'completion-2',
      draft: 'ls',
      bufferRevision: 0,
      cursor: 2,
      promptGeneration: 0
    }
  },
  {
    call: 'requestCompletion',
    terminalId: 'pty-1',
    request: {
      requestId: 'completion-3',
      draft: 'ls',
      bufferRevision: 2 ** 53,
      cursor: 2,
      promptGeneration: 0,
      cwd: '/'
    }
  },
  { call: 'cancel', terminalId: 'pty-1', requestId: 'completion-1' },
  { call: 'restore', terminalId: 'pty-1' }
]

const ROUTE_REPLIES: Array<Reply | 'throw' | 'silent'> = [
  { success: true, data: 'Accepted' },
  { success: true, data: 'NotStarted' },
  { success: true, data: 'Bogus' },
  { success: true, data: { result: 'Accepted' } },
  { success: true, data: { result: 'Accepted', outcome: 'TimedOut' } },
  { success: true, data: { result: 'Accepted', outcome: 'Cancelled', completion: null } },
  {
    success: true,
    data: {
      result: 'Accepted',
      outcome: 'Completed',
      completion: {
        request_id: 'completion-1',
        buffer_revision: 3,
        cursor: 6,
        prompt_generation: 2,
        cwd: '/repo',
        output: '6c73'
      }
    }
  },
  {
    success: true,
    data: {
      result: 'Accepted',
      outcome: 'Completed',
      completion: {
        request_id: 'completion-1',
        buffer_revision: -1,
        cursor: 6,
        prompt_generation: 2,
        cwd: '/repo',
        output: ''
      }
    }
  },
  { success: false, code: 'NETWORK_ERROR' },
  { success: false, code: 'INVALID_REQUEST' },
  'throw',
  'silent'
]

export async function recordRoutes() {
  const out = []
  for (const call of ROUTE_CALLS) {
    for (const reply of ROUTE_REPLIES) {
      const frames: unknown[] = []
      const pending = new Map<
        string,
        { timer: ReturnType<typeof setTimeout>; resolve: (r: unknown) => void }
      >()
      const socket = {
        send(text: string) {
          if (reply === 'throw') throw new Error('socket closed')
          frames.push(JSON.parse(text))
          if (reply === 'silent') return
          queueMicrotask(() => {
            const entry = pending.get('req-1')
            if (!entry) return
            clearTimeout(entry.timer)
            pending.delete('req-1')
            entry.resolve({ id: 'req-1', ...reply })
          })
        }
      }
      const transport = createWebCommandEditorTransport({
        getSessionRequest: () => ({ socket, id: 'req-1', pending, timeoutMs: 5 }) as never
      })
      let result: unknown
      switch (call.call) {
        case 'submit':
          result = await transport.submitCommand(call.terminalId, call.draft)
          break
        case 'requestCompletion':
          result = await transport.requestCompletion(call.terminalId, call.request)
          break
        case 'cancel':
          result = await transport.cancelCompletion(call.terminalId, call.requestId)
          break
        case 'restore':
          result = await transport.restorePrompt(call.terminalId)
          break
      }
      const logs = drainLogs().map(({ level, source, message }) => ({ level, source, message }))
      out.push({ ...call, reply, frames, result, logs })
    }
  }
  return out
}

// ---- command history ----------------------------------------------------------------------
// `useAddCommand` is a hook, so it cannot be called here; this repeats it around the real
// store and `trimListToCap`.

/** `useAddCommand` and `useCommandHistory`'s recall list, around the real store. */
export function recordHistory() {
  const store = useCommandHistoryStore
  store.setState({ entries: [] })
  let clock = 1_700_000_000_000
  const realNow = Date.now
  const realRandom = Math.random
  Date.now = () => clock
  Math.random = () => 0.5
  const steps = []
  const adds: Array<[string, string]> = [
    ['p1', 'git status'],
    ['p1', '  ls -la  '],
    ['p2', 'cargo test'],
    ['p1', ''],
    ['p1', '   '],
    ['p1', 'git status'],
    ['p1', 'echo "a\nb"'],
    ['p2', 'git stash'],
    ['p1', 'git stash pop'],
    ['p1', 'x'.repeat(300)]
  ]
  try {
    for (const [projectId, command] of adds) {
      clock += 1000
      const trimmed = command.trim()
      let write = null
      if (trimmed) {
        store.getState().addCommand({
          command: trimmed,
          terminalName: 'zsh',
          terminalId: 't1',
          projectId,
          timestamp: Date.now()
        })
        const projectEntries = store.getState().entries.filter((e) => e.projectId === projectId)
        write = {
          key: `projects/${projectId}/command-history`,
          value: trimListToCap(projectEntries, (kept) => kept, 'head').items
        }
      }
      const recall = (id: string) =>
        store
          .getState()
          .entries.filter((entry) => entry.projectId === id)
          .map((entry) => entry.command)
          .reverse()
      steps.push({ projectId, command, write, recall: { p1: recall('p1'), p2: recall('p2') } })
    }
    // A long project history is cut to 500 entries and then to the host's value cap.
    store.setState({ entries: [] })
    const from = clock
    const long = 'y'.repeat(700)
    for (let i = 0; i < 520; i += 1) {
      clock += 1
      store.getState().addCommand({
        command: `${long}${i}`,
        terminalName: 'zsh',
        terminalId: 't1',
        projectId: 'p3',
        timestamp: Date.now()
      })
    }
    const entries = store.getState().entries
    const kept = trimListToCap(entries, (k) => k, 'head').items
    return {
      steps,
      cap: {
        from,
        stored: entries.length,
        written: kept.length,
        first: kept[0]?.command.slice(700),
        last: kept.at(-1)?.command.slice(700)
      }
    }
  } finally {
    Date.now = realNow
    Math.random = realRandom
    store.setState({ entries: [] })
  }
}
