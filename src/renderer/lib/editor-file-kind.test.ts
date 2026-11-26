import { describe, expect, it } from 'vitest'
import {
  getDefaultViewMode,
  normalizeViewMode,
  textPreviewForPath,
  VIEW_MODES_BY_PREVIEW,
  viewModesForPath
} from './editor-file-kind'

describe('editor view modes', () => {
  it('offers preview, split and source for html, and preview and source for markdown and svg', () => {
    expect(VIEW_MODES_BY_PREVIEW).toEqual({
      markdown: ['preview', 'code'],
      svg: ['preview', 'code'],
      html: ['preview', 'split', 'code']
    })
    expect(viewModesForPath('/site/index.html')).toEqual(['preview', 'split', 'code'])
    expect(viewModesForPath('/site/OLD.HTM')).toEqual(['preview', 'split', 'code'])
    expect(viewModesForPath('/docs/spec.md')).toEqual(['preview', 'code'])
    expect(viewModesForPath('/src/app.ts')).toEqual(['code'])
  })

  it('detects html as a previewable kind', () => {
    expect(textPreviewForPath('/site/index.html')).toBe('html')
    expect(textPreviewForPath('/site/page.htm')).toBe('html')
    expect(textPreviewForPath('/icons/logo.svg')).toBe('svg')
    expect(textPreviewForPath('/src/app.ts')).toBeNull()
  })

  it('opens html in split, markdown and svg in preview, and the rest in source', () => {
    expect(getDefaultViewMode('/site/index.html')).toBe('split')
    expect(getDefaultViewMode('/docs/spec.md')).toBe('preview')
    expect(getDefaultViewMode('/icons/logo.svg')).toBe('preview')
    expect(getDefaultViewMode('/src/app.ts')).toBe('code')
  })

  it('keeps a mode the renamed path offers and falls back to its default otherwise', () => {
    expect(normalizeViewMode('/site/index.html', 'split')).toBe('split')
    expect(normalizeViewMode('/site/index.html', 'code')).toBe('code')
    expect(normalizeViewMode('/docs/spec.md', 'split')).toBe('preview')
    expect(normalizeViewMode('/docs/spec.md', 'code')).toBe('code')
    expect(normalizeViewMode('/src/app.ts', 'split')).toBe('code')
    expect(normalizeViewMode('/src/app.ts', 'preview')).toBe('code')
  })
})
