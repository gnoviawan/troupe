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

/** The word ends at the cursor and starts after unquoted, unescaped whitespace. */
function lastWordRange(draft: string, cursor: number): { from: number; to: number; quote: Quote } {
  const point = Math.max(0, Math.min(cursor, draft.length))
  let from = 0
  let quote: Quote = null
  for (let i = 0; i < point; i++) {
    const char = draft[i]
    if (quote === "'") {
      if (char === "'") quote = null
    } else if (char === '\\') {
      i += 1
    } else if (quote === '"') {
      if (char === '"') quote = null
    } else if (char === "'" || char === '"') {
      quote = char
    } else if (/\s/.test(char)) {
      from = i + 1
    }
  }
  return { from, to: point, quote }
}

/**
 * Candidates are plain words; spell them for the quoting open at the cursor.
 * A directory keeps its quote open so the path can be continued.
 */
function quoteCandidate(text: string, quote: Quote): string {
  const close = text.endsWith('/') ? '' : (quote ?? '')
  if (quote === "'") return `'${text.replace(/'/g, "'\\''")}${close}`
  if (quote === '"') return `"${text.replace(DOUBLE_QUOTED_SPECIAL, '\\$&')}${close}`
  // zsh compsys may hand over words it already escaped.
  if (/\\./.test(text)) return text
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
