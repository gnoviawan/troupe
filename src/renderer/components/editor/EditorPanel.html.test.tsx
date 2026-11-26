import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

const { mobile } = vi.hoisted(() => ({ mobile: { current: false } }))

vi.mock('@/lib/api', () => ({
  filesystemApi: { readFile: vi.fn(), writeFile: vi.fn(), getFileInfo: vi.fn() }
}))
vi.mock('@/lib/log-api', () => ({ logFrontendError: vi.fn() }))
vi.mock('@/hooks/use-toc-settings', () => ({ useTocSettings: vi.fn() }))
vi.mock('@/hooks/use-mobile-web-shell', () => ({ useMobileWebShell: () => mobile.current }))
vi.mock('./CodeEditor', () => ({
  CodeEditor: (props: { content: string }) => <pre data-testid="code-editor">{props.content}</pre>
}))
vi.mock('./html-preview/HtmlPreview', () => ({
  HtmlPreview: (props: { content: string }) => <div data-testid="html-preview">{props.content}</div>
}))

import type { EditorViewMode } from '@/lib/editor-file-kind'
import { useEditorStore } from '@/stores/editor-store'
import { EditorPanel } from './EditorPanel'

const FILE = '/site/index.html'

function show(viewMode: EditorViewMode): void {
  useEditorStore.setState({
    openFiles: new Map([
      [
        FILE,
        {
          filePath: FILE,
          content: '<p>buffer</p>',
          kind: 'text',
          originalContent: '<p>disk</p>',
          isDirty: true,
          language: 'html',
          lastModified: 1,
          viewMode,
          cursorPosition: { line: 1, col: 1 },
          scrollTop: 0,
          operationStatus: 'idle'
        }
      ]
    ]),
    activeFilePath: FILE
  })
  render(<EditorPanel filePath={FILE} isVisible />)
}

afterEach(() => {
  cleanup()
  mobile.current = false
  useEditorStore.setState({ openFiles: new Map(), activeFilePath: null })
})

describe('EditorPanel html views', () => {
  it('puts the source and the live buffer preview side by side in split', () => {
    show('split')

    expect(screen.getByTestId('code-editor')).toHaveTextContent('<p>buffer</p>')
    expect(screen.getByTestId('html-preview')).toHaveTextContent('<p>buffer</p>')
    expect(screen.getByTestId('html-preview').parentElement).toHaveClass(
      'border-l-[0.5px]',
      'border-border'
    )
  })

  it('shows only the preview in preview and only the source in source', () => {
    show('preview')
    expect(screen.queryByTestId('code-editor')).toBeNull()
    expect(screen.getByTestId('html-preview')).toBeInTheDocument()
    cleanup()

    show('code')
    expect(screen.getByTestId('code-editor')).toBeInTheDocument()
    expect(screen.queryByTestId('html-preview')).toBeNull()
  })

  it('falls back to the preview for split on the mobile web shell', () => {
    mobile.current = true
    show('split')

    expect(screen.queryByTestId('code-editor')).toBeNull()
    expect(screen.getByTestId('html-preview')).toBeInTheDocument()
    expect(screen.getByRole('radio', { name: 'Preview' })).toHaveAttribute('aria-checked', 'true')
  })
})
