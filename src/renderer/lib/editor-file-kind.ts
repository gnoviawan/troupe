import { type MediaKind, mediaKindForPath } from './media-kind'
import { basename } from './path-basename'

export type EditorFileKind = 'text' | MediaKind

export type EditorViewMode = 'code' | 'preview' | 'split'

export type TextPreviewKind = 'markdown' | 'svg' | 'html'

function extensionOf(filePath: string): string {
  const name = basename(filePath)
  const dotIndex = name.lastIndexOf('.')
  if (dotIndex <= 0) return ''
  return name.slice(dotIndex + 1).toLowerCase()
}

export function editorFileKindForPath(filePath: string): EditorFileKind {
  if (extensionOf(filePath) === 'svg') return 'text'
  return mediaKindForPath(filePath) ?? 'text'
}

const LANGUAGE_BY_EXTENSION: Readonly<Record<string, string>> = {
  ts: 'typescript',
  tsx: 'typescript',
  js: 'javascript',
  jsx: 'javascript',
  json: 'json',
  css: 'css',
  scss: 'css',
  html: 'html',
  htm: 'html',
  astro: 'astro',
  md: 'markdown',
  yaml: 'yaml',
  yml: 'yaml',
  xml: 'xml',
  svg: 'xml',
  sh: 'shell',
  bash: 'shell',
  py: 'python',
  rs: 'rust',
  go: 'go',
  toml: 'toml'
}

export function detectLanguage(filePath: string): string {
  const ext = extensionOf(filePath)
  return Object.hasOwn(LANGUAGE_BY_EXTENSION, ext) ? LANGUAGE_BY_EXTENSION[ext] : ''
}

const PREVIEW_BY_EXTENSION: Readonly<Record<string, TextPreviewKind>> = {
  md: 'markdown',
  svg: 'svg',
  html: 'html',
  htm: 'html'
}

/** The view modes each previewable kind offers, in toolbar order. */
export const VIEW_MODES_BY_PREVIEW: Readonly<Record<TextPreviewKind, readonly EditorViewMode[]>> = {
  markdown: ['preview', 'code'],
  svg: ['preview', 'code'],
  html: ['preview', 'split', 'code']
}

const DEFAULT_VIEW_MODE: Readonly<Record<TextPreviewKind, EditorViewMode>> = {
  markdown: 'preview',
  svg: 'preview',
  html: 'split'
}

export function textPreviewForPath(filePath: string): TextPreviewKind | null {
  const ext = extensionOf(filePath)
  return Object.hasOwn(PREVIEW_BY_EXTENSION, ext) ? PREVIEW_BY_EXTENSION[ext] : null
}

export function viewModesForPath(filePath: string): readonly EditorViewMode[] {
  const preview = textPreviewForPath(filePath)
  return preview ? VIEW_MODES_BY_PREVIEW[preview] : ['code']
}

export function getDefaultViewMode(filePath: string): EditorViewMode {
  const preview = textPreviewForPath(filePath)
  return preview ? DEFAULT_VIEW_MODE[preview] : 'code'
}

/** `mode` when `filePath` offers it (a rename can change the kind), else the path's default. */
export function normalizeViewMode(filePath: string, mode: EditorViewMode): EditorViewMode {
  return viewModesForPath(filePath).includes(mode) ? mode : getDefaultViewMode(filePath)
}
