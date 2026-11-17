import { expect, it } from 'vitest'
import { parseCompletionOutput, wordContinuesPastCursor } from './completion-output'

function hexLine(text: string): string {
  return Array.from(new TextEncoder().encode(text), (byte) =>
    byte.toString(16).padStart(2, '0')
  ).join('')
}

it('completion output becomes rows', () => {
  const draft = 'git sta'
  const output = [hexLine('status'), 'not-hex', hexLine('stash'), hexLine('status')].join('\n')

  const rows = parseCompletionOutput(draft, 7, output)

  expect(rows).toEqual([
    { text: 'status', from: 4, to: 7 },
    { text: 'stash', from: 4, to: 7 }
  ])
  expect(draft).toBe('git sta')

  expect(parseCompletionOutput(draft, 7, '')).toEqual([])
  expect(parseCompletionOutput('git ', 4, hexLine('status'))).toEqual([
    { text: 'status', from: 4, to: 4 }
  ])
})

it('drops completion candidates that contain ESC or CR', () => {
  const draft = 'git sta'
  const output = [
    hexLine('status\x1b[201~'),
    hexLine('stash\rinjected'),
    hexLine('stash'),
    hexLine('status\tok'),
    hexLine('hidden\u200bname'),
    hexLine('bidi\u202ename'),
    hexLine('isolated\u2066name')
  ].join('\n')

  const rows = parseCompletionOutput(draft, 7, output)

  expect(rows.map((row) => row.text)).toEqual(['stash'])
})

it('candidates take the shell spelling of the word being completed (#1879)', () => {
  const output = [hexLine('dir with space/'), hexLine('file one.txt'), hexLine('plain')].join('\n')
  const spell = (draft: string) =>
    parseCompletionOutput(draft, draft.length, output).map((row) => row.insert ?? row.text)

  expect(spell('cat ')).toEqual(['dir\\ with\\ space/', 'file\\ one.txt', 'plain'])
  expect(spell('cat "di')).toEqual(['"dir with space/', '"file one.txt"', '"plain"'])
  expect(spell("cat 'di")).toEqual(["'dir with space/", "'file one.txt'", "'plain'"])
  expect(spell('cat "dir"\\ w')).toEqual(['dir\\ with\\ space/', 'file\\ one.txt', 'plain'])
  expect(parseCompletionOutput('cat "plain', 10, hexLine('plain'))[0]).toEqual({
    text: 'plain',
    insert: '"plain"',
    from: 4,
    to: 10
  })
})

it('the word being completed may contain quoted or escaped spaces (#1879)', () => {
  const output = hexLine('dir with space')
  expect(parseCompletionOutput('ls dir\\ w', 9, output)[0]).toMatchObject({ from: 3, to: 9 })
  expect(parseCompletionOutput('cd "a b" "dir w', 15, output)[0]).toMatchObject({
    from: 9,
    to: 15
  })
  expect(parseCompletionOutput("ls 'x y' dir", 12, output)[0]).toMatchObject({ from: 9, to: 12 })
})

it('shell syntax in candidates is escaped', () => {
  const rows = parseCompletionOutput(
    'x ',
    2,
    [hexLine("it's"), hexLine('a&b'), hexLine('#tag'), hexLine('--color=auto')].join('\n')
  )
  expect(rows.map((row) => row.insert ?? row.text)).toEqual([
    "it\\'s",
    'a\\&b',
    '\\#tag',
    '--color=auto'
  ])
  const quoted = parseCompletionOutput('x "', 3, [hexLine('a"$b'), hexLine("it's")].join('\n'))
  expect(quoted.map((row) => row.insert)).toEqual(['"a\\"\\$b"', '"it\'s"'])
  const single = parseCompletionOutput("x '", 3, hexLine("it's"))
  expect(single[0]?.insert).toBe("'it'\\''s'")
})

// Both host scripts send raw words (bash compgen; zsh capture unquotes -Q matches).
it('a backslash in a candidate is a literal character and is escaped', () => {
  const rows = parseCompletionOutput(
    'cat ',
    4,
    [hexLine('a\\b.txt'), hexLine('my file\\b.txt'), hexLine('a\\ b')].join('\n')
  )
  expect(rows.map((row) => row.insert)).toEqual(['a\\\\b.txt', 'my\\ file\\\\b.txt', 'a\\\\\\ b'])
  expect(parseCompletionOutput('cat "', 5, hexLine('a\\b.txt'))[0]?.insert).toBe('"a\\\\b.txt"')
})

// Bash history-expands `!` inside double quotes; a backslash there stays literal.
it('a double-quoted candidate breaks out of the quotes for !', () => {
  const spell = (output: string) => parseCompletionOutput('cat "hel', 8, hexLine(output))[0]?.insert
  expect(spell('hello!world.txt')).toBe('"hello"\\!"world.txt"')
  expect(spell('hello!/')).toBe('"hello"\\!"/')
  expect(parseCompletionOutput('cat hel', 7, hexLine('hello!world.txt'))[0]?.insert).toBe(
    'hello\\!world.txt'
  )
  expect(parseCompletionOutput("cat 'hel", 8, hexLine('hello!world.txt'))[0]?.insert).toBe(
    "'hello!world.txt'"
  )
})

it('braces in an unquoted candidate are escaped against brace expansion', () => {
  const rows = parseCompletionOutput(
    'cat ',
    4,
    [hexLine('report{a,b}.txt'), hexLine('{x}')].join('\n')
  )
  expect(rows.map((row) => row.insert)).toEqual(['report\\{a,b\\}.txt', '\\{x\\}'])
})

// A leading `~` is a tilde prefix only when the user is completing one.
it('a literal leading tilde is escaped, a tilde prefix being completed is not', () => {
  const spell = (draft: string, text: string) =>
    parseCompletionOutput(draft, draft.length, hexLine(text))[0]?.insert ?? text
  expect(spell('cat ', '~')).toBe('\\~')
  expect(spell('cat ', '~backup')).toBe('\\~backup')
  expect(spell('cat \\~b', '~backup')).toBe('\\~backup')
  expect(spell('cat ~', '~')).toBe('\\~')
  expect(spell('cat ~', '~backup')).toBe('~backup')
  expect(spell('cd ~/Doc', '~/Documents/')).toBe('~/Documents/')
  expect(spell('cd ~ro', '~root/')).toBe('~root/')
  expect(spell('cat a', 'a~b')).toBe('a~b')
  expect(spell("cat '", '~')).toBe("'~'")
})

// In $'...' a backslash escapes the next character, including a quote.
it('ansi-c quotes keep an escaped quote and the space after it in the word', () => {
  const draft = "cat $'a\\' b'"
  expect(wordContinuesPastCursor(draft, draft.indexOf(' b'))).toBe(true)
  expect(wordContinuesPastCursor("cat $'a' b", 8)).toBe(false)
  expect(wordContinuesPastCursor("cat \\$'a\\' b", 10)).toBe(false)
  const open = "cat $'a\\' b"
  expect(parseCompletionOutput(open, open.length, hexLine("a' bc"))[0]).toEqual({
    text: "a' bc",
    insert: "$'a\\' bc'",
    from: 4,
    to: open.length
  })
  expect(parseCompletionOutput("cat $'d", 7, hexLine('d\\ir/'))[0]?.insert).toBe("$'d\\\\ir/")
})
