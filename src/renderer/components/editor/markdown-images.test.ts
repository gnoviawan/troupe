import { BlockNoteEditor } from '@blocknote/core'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { markdownEditorSchema } from '@/hooks/use-blocknote'
import { readMediaBytes } from '@/lib/media-bytes-api'
import { createEditorFileUrlResolver } from './editor-file-url'

vi.mock('@/lib/media-bytes-api', () => ({ readMediaBytes: vi.fn() }))

const DOC = `# Workflow

| Guide | Paint |
| --- | --- |
| ![Guide v0](assets/guide.png) | ![Paint v0](assets/paint.png) |

![Turntable](assets/turntable.png)

Badges ![build](https://img.example/build.svg) inline.
`

function imagesOf(value: unknown): Array<{ type: string; url: string }> {
  if (Array.isArray(value)) return value.flatMap(imagesOf)
  if (value === null || typeof value !== 'object') return []
  const node = value as Record<string, unknown>
  const props = node.props as { url?: string } | undefined
  const own =
    node.type === 'image' || node.type === 'inlineImage'
      ? [{ type: String(node.type), url: String(props?.url) }]
      : []
  return [...own, ...Object.values(node).flatMap(imagesOf)]
}

describe('markdown images in the editor', () => {
  const editor = BlockNoteEditor.create({ schema: markdownEditorSchema })

  it('keeps images in table cells and in text, and a lone image as a block', async () => {
    const blocks = await editor.tryParseMarkdownToBlocks(DOC)
    expect(imagesOf(blocks)).toEqual([
      { type: 'inlineImage', url: 'assets/guide.png' },
      { type: 'inlineImage', url: 'assets/paint.png' },
      { type: 'image', url: 'assets/turntable.png' },
      { type: 'inlineImage', url: 'https://img.example/build.svg' }
    ])
  })

  it('writes every image back when the file is saved', async () => {
    const blocks = await editor.tryParseMarkdownToBlocks(DOC)
    const markdown = await editor.blocksToMarkdownLossy(blocks)
    expect(markdown).toContain('| ![Guide v0](assets/guide.png) | ![Paint v0](assets/paint.png) |')
    expect(markdown).toContain('![build](https://img.example/build.svg)')
    expect(markdown).toContain('assets/turntable.png')
  })
})

describe('createEditorFileUrlResolver', () => {
  afterEach(() => vi.mocked(readMediaBytes).mockReset())

  it('reads a relative image from the open file folder once', async () => {
    vi.mocked(readMediaBytes).mockResolvedValue({ success: true, data: new Blob(['png']) })
    const resolver = createEditorFileUrlResolver(() => '/root/game/docs/workflow.md')

    const first = await resolver.resolveFileUrl('assets/guide.png')
    const second = await resolver.resolveFileUrl('./assets/guide.png')

    expect(first).toMatch(/^blob:/)
    expect(second).toBe(first)
    expect(vi.mocked(readMediaBytes).mock.calls).toEqual([['/root/game/docs/assets/guide.png']])
  })

  it('resolves parent folders and file URLs', async () => {
    vi.mocked(readMediaBytes).mockResolvedValue({ success: true, data: new Blob(['png']) })
    const resolver = createEditorFileUrlResolver(() => '/root/game/docs/workflow.md')

    await resolver.resolveFileUrl('../art/tree.png')
    await resolver.resolveFileUrl('file:///tmp/shot.png')

    expect(vi.mocked(readMediaBytes).mock.calls).toEqual([
      ['/root/game/art/tree.png'],
      ['/tmp/shot.png']
    ])
  })

  it('leaves web URLs alone and returns the original URL when a read fails', async () => {
    vi.mocked(readMediaBytes).mockResolvedValue({
      success: false,
      code: 'NOT_FOUND',
      error: 'missing'
    })
    const resolver = createEditorFileUrlResolver(() => '/root/game/docs/workflow.md')

    expect(await resolver.resolveFileUrl('https://img.example/a.png')).toBe(
      'https://img.example/a.png'
    )
    expect(await resolver.resolveFileUrl('missing.png')).toBe('missing.png')
    expect(vi.mocked(readMediaBytes)).toHaveBeenCalledTimes(1)
  })
})
