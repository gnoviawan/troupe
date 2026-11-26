/**
 * Command editor key vectors: a CodeMirror view with the command editor's keymap
 * (`editor-keys.ts`) and history, driven by recorded keys. `model::terminal::line` replays
 * them (`tests/terminal/keys.rs`). Text is typed through a transaction, as the browser's
 * input would land; the arrows and Backspace that CodeMirror leaves to the browser are not
 * recorded, the caret moves with `select` instead.
 */

import { history } from '@codemirror/commands'
import { EditorSelection, EditorState } from '@codemirror/state'
import { EditorView, runScopeHandlers } from '@codemirror/view'
import {
  editorKeymap,
  sanitizeEditorPaste
} from '../../../src/renderer/components/terminal/command-editor/editor-keys'

export const keySources = ['src/renderer/components/terminal/command-editor/editor-keys.ts']

export type KeyStep =
  | { type: string }
  | { paste: string }
  | { select: [number, number] }
  | { key: string }
  | { selection: string }

export interface KeyCase {
  name: string
  doc: string
  history: string[]
  steps: KeyStep[]
}

const HISTORY = ['ls', 'git status', 'git stash', 'echo hi']

export const keyCases: KeyCase[] = [
  {
    name: 'multi_line_command',
    doc: '',
    history: [],
    steps: [
      { type: 'echo "a' },
      { key: 'Shift-Enter' },
      { type: 'b"' },
      { key: 'Shift-Enter' },
      { type: '&& ls' },
      { key: 'Enter' }
    ]
  },
  {
    name: 'recall_walks_back_and_forward',
    doc: '',
    history: HISTORY,
    steps: [
      { key: 'ArrowUp' },
      { key: 'ArrowUp' },
      { key: 'ArrowUp' },
      { key: 'ArrowUp' },
      { key: 'ArrowUp' },
      { key: 'ArrowUp' },
      { key: 'ArrowDown' },
      { key: 'ArrowDown' },
      { key: 'ArrowDown' },
      { key: 'ArrowDown' },
      { key: 'ArrowDown' },
      { key: 'ArrowDown' },
      { key: 'ArrowDown' }
    ]
  },
  {
    name: 'recall_by_prefix_keeps_the_draft',
    doc: 'git',
    history: HISTORY,
    steps: [
      { key: 'ArrowUp' },
      { key: 'ArrowUp' },
      { key: 'ArrowUp' },
      { key: 'ArrowDown' },
      { key: 'ArrowDown' },
      { key: 'ArrowDown' }
    ]
  },
  {
    name: 'an_edit_ends_the_recall',
    doc: '',
    history: HISTORY,
    steps: [
      { key: 'ArrowUp' },
      { key: 'ArrowUp' },
      { type: ' -u' },
      { key: 'ArrowUp' },
      { key: 'ArrowDown' },
      { key: 'Enter' }
    ]
  },
  {
    // jsdom has no layout, so Arrow Up and Down are pressed only on a draft's first or last line.
    name: 'recall_of_a_multi_line_entry',
    doc: '',
    history: ['ls', 'echo "a\nb"'],
    steps: [
      { key: 'ArrowUp' },
      { select: [3, 3] },
      { key: 'ArrowUp' },
      { key: 'ArrowDown' },
      { key: 'ArrowDown' },
      { key: 'ArrowDown' }
    ]
  },
  {
    name: 'readline_keys',
    doc: 'git commit -m "msg here"',
    history: [],
    steps: [
      { select: [10, 10] },
      { key: 'Ctrl-a' },
      { key: 'Ctrl-e' },
      { select: [14, 14] },
      { key: 'Ctrl-k' },
      { key: 'Ctrl-w' },
      { key: 'Ctrl-w' },
      { key: 'Ctrl-u' },
      { type: 'echo one\ntwo three' },
      { select: [9, 9] },
      { key: 'Ctrl-u' },
      { key: 'Ctrl-u' },
      { key: 'Ctrl-e' },
      { key: 'Ctrl-k' },
      { key: 'Ctrl-k' }
    ]
  },
  {
    name: 'ctrl_c_copies_a_selection_else_clears',
    doc: 'echo secret',
    history: [],
    steps: [
      { select: [5, 11] },
      { key: 'Ctrl-c' },
      { select: [11, 11] },
      { selection: 'terminal text' },
      { key: 'Ctrl-c' },
      { selection: '' },
      { key: 'Ctrl-c' },
      { key: 'Ctrl-c' }
    ]
  },
  {
    name: 'ctrl_d_is_eof_on_an_empty_line',
    doc: '',
    history: [],
    steps: [
      { key: 'Ctrl-d' },
      { type: 'abc' },
      { key: 'Ctrl-d' },
      { select: [1, 1] },
      { key: 'Ctrl-d' },
      { key: 'Ctrl-d' },
      { key: 'Ctrl-d' }
    ]
  },
  {
    name: 'paste_is_cleaned',
    doc: 'before old after',
    history: [],
    steps: [
      { select: [7, 10] },
      { paste: '\x1b[200~echo\x1b hi\x00\x01\x7f\n\tthere\r\nnext\rlast\x1b[201~' },
      { paste: 'safe‮ café‍' },
      { key: 'Enter' }
    ]
  },
  {
    name: 'unicode_words',
    doc: 'echo café 😀 naïve',
    history: [],
    steps: [
      { key: 'Ctrl-w' },
      { key: 'Ctrl-w' },
      { key: 'Ctrl-w' },
      { select: [5, 5] },
      { key: 'Ctrl-k' },
      { key: 'Enter' }
    ]
  }
]

function keyEvent(spec: string): KeyboardEvent {
  const parts = spec.split('-')
  const key = parts.at(-1) as string
  const mods = new Set(parts.slice(0, -1))
  return new KeyboardEvent('keydown', {
    key: key === 'Enter' || key.startsWith('Arrow') ? key : key.toLowerCase(),
    ctrlKey: mods.has('Ctrl'),
    shiftKey: mods.has('Shift'),
    bubbles: true,
    cancelable: true
  })
}

/** jsdom has no layout; CodeMirror still asks the Range geometry API. */
function withGeometry<T>(run: () => T): T {
  const names = ['getClientRects', 'getBoundingClientRect'] as const
  const saved = names.map((name) => Object.getOwnPropertyDescriptor(Range.prototype, name))
  Object.defineProperty(Range.prototype, 'getClientRects', { configurable: true, value: () => [] })
  Object.defineProperty(Range.prototype, 'getBoundingClientRect', {
    configurable: true,
    value: () => new DOMRect()
  })
  try {
    return run()
  } finally {
    names.forEach((name, i) => {
      const original = saved[i]
      if (original) Object.defineProperty(Range.prototype, name, original)
      else delete (Range.prototype as unknown as Record<string, unknown>)[name]
    })
  }
}

export function recordKeys() {
  return withGeometry(recordCases)
}

function recordCases() {
  return keyCases.map((c) => {
    const submitted: string[] = []
    const copied: string[] = []
    let eof = 0
    let terminalSelection = ''
    const view = new EditorView({
      parent: document.body,
      state: EditorState.create({
        doc: c.doc,
        selection: { anchor: c.doc.length },
        extensions: [
          history(),
          editorKeymap({
            history: () => c.history,
            submit: (text) => submitted.push(text),
            sendEof: () => {
              eof += 1
            },
            copy: (text) => copied.push(text),
            selection: () => terminalSelection,
            isSubmitting: () => false,
            composition: () => ({ open: false, text: '' })
          })
        ]
      })
    })
    try {
      const steps = c.steps.map((step) => {
        let handled: boolean | null = null
        if ('type' in step) {
          view.dispatch({ ...view.state.replaceSelection(step.type), userEvent: 'input.type' })
        } else if ('paste' in step) {
          view.dispatch({
            ...view.state.replaceSelection(sanitizeEditorPaste(step.paste)),
            userEvent: 'input.paste'
          })
        } else if ('select' in step) {
          view.dispatch({ selection: EditorSelection.single(step.select[0], step.select[1]) })
        } else if ('selection' in step) {
          terminalSelection = step.selection
        } else {
          handled = runScopeHandlers(view, keyEvent(step.key), 'editor')
        }
        const { anchor, head } = view.state.selection.main
        return {
          ...step,
          handled,
          doc: view.state.doc.toString(),
          anchor,
          head,
          submitted: [...submitted],
          copied: [...copied],
          eof
        }
      })
      return { name: c.name, doc: c.doc, history: c.history, steps }
    } finally {
      view.destroy()
    }
  })
}
