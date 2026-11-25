import { beforeEach, describe, expect, it, vi } from 'vitest'

const BIG_ID = '18446744073709551615'

type Reply = { ok: true; result: string } | { ok: false; error: { code: string; message: string } }

const state: {
  phase: string
  calls: Array<{ method: string; params: string }>
  answer: (method: string, params: string) => Reply
} = {
  phase: 'ready',
  calls: [],
  answer: () => ({ ok: true, result: '{}' })
}

vi.mock('@/stores/canvas-store', () => ({
  useCanvasStore: {
    getState: () => ({
      sessions: { p1: { phase: { kind: state.phase } } },
      callCanvas: (_projectId: string, method: string, params: string) => {
        state.calls.push({ method, params })
        return Promise.resolve(state.answer(method, params))
      }
    })
  }
}))

vi.mock('@/lib/log-api', () => ({ logFrontendError: vi.fn(() => Promise.resolve()) }))

const {
  parseCanvasJson,
  readCanvasMentionTargets,
  resolveCanvasContexts,
  selectionLabel,
  CANVAS_CONTEXT_CHARS
} = await import('@/lib/canvas-mention')

// The editor's JSON as raw text, so the u64 id is written as a bare number.
const DOCUMENT = `{"currentPage":1,"selection":[${BIG_ID},12],"pages":[{"id":1,"name":"ds4a","children":[
  {"id":${BIG_ID},"name":"ds4a / Foundation","type":"FRAME","width":265,"height":700},
  {"id":20,"name":"ds4a / Components","type":"SECTION","width":80.4,"height":400.6}
]}]}`

const NODES: Record<string, string> = {
  12: '{"id":12,"name":"Button","type":"INSTANCE","width":64,"height":24}',
  [BIG_ID]: `{"id":${BIG_ID},"name":"ds4a / Foundation","type":"FRAME","width":265,"height":700,
    "absoluteBounds":{"x":920,"y":248,"width":265,"height":700},"children":[
      {"id":30,"name":"Title","type":"TEXT","width":200,"height":20,"characters":"The system behind the page"},
      {"id":31,"name":"Primary","type":"INSTANCE","width":60,"height":24,"mainComponentName":"Button","visible":false,"childCount":3}
    ]}`
}

function editor(method: string, params: string): Reply {
  if (method === 'document.inspect') return { ok: true, result: DOCUMENT }
  const id = /"id":(\d+)/.exec(params)?.[1] ?? ''
  const node = NODES[id]
  return node
    ? { ok: true, result: node }
    : { ok: false, error: { code: 'ENGINE_ERROR', message: 'no such node' } }
}

beforeEach(() => {
  state.phase = 'ready'
  state.calls = []
  state.answer = editor
})

describe('parseCanvasJson', () => {
  it('keeps u64 ids exact and leaves numbers inside strings alone', () => {
    const value = parseCanvasJson(`{"id":${BIG_ID},"n":3,"name":"id 12345678901234567"}`)
    expect(value).toEqual({ id: BIG_ID, n: 3, name: 'id 12345678901234567' })
  })
})

describe('readCanvasMentionTargets', () => {
  it('lists the page layers and the selection with exact ids', async () => {
    const targets = await readCanvasMentionTargets('p1')
    expect(targets.pageName).toBe('ds4a')
    expect(targets.layers.map((l) => [l.id, l.name, l.type])).toEqual([
      [BIG_ID, 'ds4a / Foundation', 'FRAME'],
      ['20', 'ds4a / Components', 'SECTION']
    ])
    expect(targets.selection.map((l) => [l.id, l.name])).toEqual([
      [BIG_ID, 'ds4a / Foundation'],
      ['12', 'Button']
    ])
    expect(selectionLabel(targets.selection)).toBe('ds4a / Foundation +1')
    // Only the nested layer needed its own lookup, with its id as a raw number.
    expect(state.calls.filter((c) => c.method === 'node.get').map((c) => c.params)).toEqual([
      '{"id":12,"depth":0,"fields":["name","type","width","height"]}'
    ])
  })

  it('rejects when the canvas is closed', async () => {
    state.answer = () => ({ ok: false, error: { code: 'CANVAS_CLOSED', message: 'closed' } })
    await expect(readCanvasMentionTargets('p1')).rejects.toThrow('CANVAS_CLOSED')
  })
})

describe('resolveCanvasContexts', () => {
  it('summarises each layer with its ids, size, position, text and component', async () => {
    const ref = { label: 'Foundation', ids: [BIG_ID] }
    const { blocks, gaps } = await resolveCanvasContexts('p1', [ref])
    expect(gaps.size).toBe(0)
    expect(blocks).toHaveLength(1)
    const text = blocks[0].text ?? ''
    expect(text.startsWith(`<troupe-canvas ids="${BIG_ID}" label="Foundation">\n`)).toBe(true)
    expect(text).toContain(`- FRAME "ds4a / Foundation" (id ${BIG_ID}) · 265×700 at 920,248`)
    expect(text).toContain('  - TEXT "Title" (id 30) · 200×20 · text "The system behind the page"')
    expect(text).toContain(
      '  - INSTANCE "Primary" (id 31) · 60×24 · instance of "Button" · hidden · 3 children'
    )
    expect(text.endsWith('\n</troupe-canvas>')).toBe(true)
  })

  it('marks every ref closed without calling the editor when the canvas is not open', async () => {
    state.phase = 'loading'
    const { blocks, gaps } = await resolveCanvasContexts('p1', [{ label: 'x', ids: ['12'] }])
    expect(blocks).toEqual([])
    expect([...gaps]).toEqual([['12', 'closed']])
    expect(state.calls).toEqual([])
    expect((await resolveCanvasContexts(null, [{ label: 'x', ids: ['12'] }])).gaps.get('12')).toBe(
      'closed'
    )
  })

  it('marks a ref whose layers are all gone as missing', async () => {
    const { blocks, gaps } = await resolveCanvasContexts('p1', [{ label: 'x', ids: ['999'] }])
    expect(blocks).toEqual([])
    expect(gaps.get('999')).toBe('missing')
  })

  it('omits refs past the context limit', async () => {
    const huge = 'y'.repeat(CANVAS_CONTEXT_CHARS)
    state.answer = (method, params) =>
      /"id":12\b/.test(params)
        ? { ok: true, result: `{"id":12,"name":"${huge}","type":"TEXT"}` }
        : editor(method, params)
    const { blocks, gaps } = await resolveCanvasContexts('p1', [
      { label: 'big', ids: ['12'] },
      { label: 'f', ids: [BIG_ID] }
    ])
    expect(gaps.get('12')).toBe('omitted')
    expect(blocks).toHaveLength(1)
  })
})
