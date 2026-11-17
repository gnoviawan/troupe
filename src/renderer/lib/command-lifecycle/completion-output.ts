export interface CompletionOutputRow {
  text: string
  from: number
  to: number
  /** Shell spelling that replaces `from..to`, when it differs from `text`. */
  insert?: string
}

type Quote = "'" | '"' | null

// Unquoted characters the shell would split, expand or treat as syntax.
const UNQUOTED_SPECIAL = /[\s'"\\$`&;|<>()*?[!]/g
const DOUBLE_QUOTED_SPECIAL = /["\\$`]/g

export function parseCompletionOutput(
  draft: string,
  cursor: number,
  output: string
): CompletionOutputRow[] {
  const { from, to, quote } = lastWordRange(draft, cursor)
  const seen = new Set<string>()
  const rows: CompletionOutputRow[] = []
  for (const raw of output.split('\n')) {
    const line = raw.endsWith('\r') ? raw.slice(0, -1) : raw
    if (!line) continue
    const text = decodeHexLine(line)
    if (text === null || containsDisallowedControls(text) || seen.has(text)) continue
    seen.add(text)
    const insert = quoteCandidate(text, quote)
    rows.push(insert === text ? { text, from, to } : { text, from, to, insert })
  }
  return rows
}

interface WordAtCursor {
  from: number
  /** Quote still open at the cursor. */
  quote: Quote
  /** The cursor sits right after an unquoted backslash, splitting an escape. */
  escaped: boolean
}

/** The word ends at the cursor and starts after unquoted, unescaped whitespace. */
function wordAtCursor(draft: string, point: number): WordAtCursor {
  let from = 0
  let quote: Quote = null
  let escaped = false
  for (let i = 0; i < point; i++) {
    const char = draft[i]
    if (quote === "'") {
      if (char === "'") quote = null
    } else if (char === '\\') {
      escaped = i + 1 >= point
      i += 1
    } else if (quote === '"') {
      if (char === '"') quote = null
    } else if (char === "'" || char === '"') {
      quote = char
    } else if (/\s/.test(char)) {
      from = i + 1
    }
  }
  return { from, quote, escaped }
}

function lastWordRange(draft: string, cursor: number): { from: number; to: number; quote: Quote } {
  const point = Math.max(0, Math.min(cursor, draft.length))
  const { from, quote } = wordAtCursor(draft, point)
  return { from, to: point, quote }
}

/**
 * True when the shell word at the cursor goes on past it: the next character is
 * not whitespace, or it is whitespace that an open quote or a backslash keeps
 * inside the word. A candidate would replace only the part before the cursor.
 */
export function wordContinuesPastCursor(draft: string, cursor: number): boolean {
  const next = draft[cursor]
  if (next === undefined || cursor < 0) return false
  if (!/\s/.test(next)) return true
  const { quote, escaped } = wordAtCursor(draft, cursor)
  return quote !== null || escaped
}

/**
 * Candidates are raw words (both host scripts unquote what their completers
 * return); spell them for the quoting open at the cursor. A directory keeps its
 * quote open so the path can be continued.
 */
function quoteCandidate(text: string, quote: Quote): string {
  const close = text.endsWith('/') ? '' : (quote ?? '')
  if (quote === "'") return `'${text.replace(/'/g, "'\\''")}${close}`
  if (quote === '"') {
    // History expansion sees `!` inside double quotes, and a backslash before
    // it stays in the word there, so step out of the quotes to escape it.
    const body = text.replace(DOUBLE_QUOTED_SPECIAL, '\\$&').replace(/!/g, '"\\!"')
    return `"${body}${close}`
  }
  return text.replace(UNQUOTED_SPECIAL, '\\$&').replace(/^#/, '\\#')
}

export function containsDisallowedControls(text: string): boolean {
  if (/\p{Cf}/u.test(text)) return true
  for (const char of text) {
    const code = char.codePointAt(0) ?? 0
    if (code <= 0x1f || code === 0x7f || (code >= 0x80 && code <= 0x9f)) return true
  }
  return false
}

function decodeHexLine(line: string): string | null {
  if (!/^(?:[0-9a-fA-F]{2})+$/.test(line)) return null
  const bytes = new Uint8Array(line.length / 2)
  for (let i = 0; i < bytes.length; i++) {
    bytes[i] = Number.parseInt(line.slice(i * 2, i * 2 + 2), 16)
  }
  return new TextDecoder('utf-8').decode(bytes)
}
