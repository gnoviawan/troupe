import { expect, it } from 'vitest'
import { parseCompletionOutput } from './completion-output'

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

it('shell syntax in candidates is escaped, already escaped words are kept', () => {
  const rows = parseCompletionOutput(
    'x ',
    2,
    [
      hexLine("it's"),
      hexLine('a&b'),
      hexLine('#tag'),
      hexLine('--color=auto'),
      hexLine('a\\ b')
    ].join('\n')
  )
  expect(rows.map((row) => row.insert ?? row.text)).toEqual([
    "it\\'s",
    'a\\&b',
    '\\#tag',
    '--color=auto',
    'a\\ b'
  ])
  const quoted = parseCompletionOutput('x "', 3, [hexLine('a"$b'), hexLine("it's")].join('\n'))
  expect(quoted.map((row) => row.insert)).toEqual(['"a\\"\\$b"', '"it\'s"'])
  const single = parseCompletionOutput("x '", 3, hexLine("it's"))
  expect(single[0]?.insert).toBe("'it'\\''s'")
})
