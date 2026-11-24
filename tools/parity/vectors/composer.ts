/**
 * Composer vectors: a composer document, what `lib/composer/` serializes it to,
 * and what the composer sends for it. `apps/rune/crates/ui/src/composer/`
 * replays them (`serialize_tests.rs`).
 *
 * A document is a list of parts. A text part may hold `\n`, which starts a new
 * paragraph, as Enter does in the Tiptap editor. The recorder builds the real
 * editor schema from the six pill nodes and reads the display string with
 * `docToDisplayText`. The send repeats `useChatComposer.buildPromptParts`
 * (a hook, so it cannot be called here) and calls `buildComposerSend`.
 */

import { getSchema } from '@tiptap/core'
import StarterKit from '@tiptap/starter-kit'
import type { PendingAttachment } from '../../../src/renderer/components/chat/chat-attachments'
import { CommandPill } from '../../../src/renderer/components/chat/composer/CommandPillNode'
import { ConversationPill } from '../../../src/renderer/components/chat/composer/ConversationPillNode'
import { FilePill } from '../../../src/renderer/components/chat/composer/FilePillNode'
import { SelectionContextPill } from '../../../src/renderer/components/chat/composer/SelectionContextPillNode'
import { SkillPill } from '../../../src/renderer/components/chat/composer/SkillPillNode'
import { ToolPill } from '../../../src/renderer/components/chat/composer/ToolPillNode'
import { buildComposerSend } from '../../../src/renderer/components/chat/composer-send-blocks'
import type { ContentBlock } from '../../../src/renderer/lib/acp-api'
import {
  CMD_PILL_NODE,
  CONTEXT_PILL_NODE,
  CONVERSATION_PILL_NODE,
  docToDisplayText,
  FILE_PILL_NODE,
  SKILL_PILL_NODE,
  TOOL_PILL_NODE
} from '../../../src/renderer/lib/composer/doc-to-prompt'
import { fileMentionBlocks } from '../../../src/renderer/lib/composer/retry-wire'
import {
  createSelectionContext,
  hasSelectionContexts,
  serializeSelectionContexts
} from '../../../src/renderer/lib/composer/selection-context'
import { hasToolTokens } from '../../../src/renderer/lib/composer/tool-tokens'
import { composerWireText } from '../../../src/renderer/lib/composer/wire-text'
import type { TranscriptGap } from '../../../src/renderer/lib/conversation-envelope'
import {
  extractCommandNames,
  extractConversationRefs,
  extractSkillNames,
  sanitizeDisplayText
} from '../../../src/renderer/lib/skill-tokens'

export const composerSources = [
  'src/renderer/components/chat/chat-attachments.ts',
  'src/renderer/components/chat/composer-send-blocks.ts',
  'src/renderer/components/chat/use-chat-composer.ts',
  'src/renderer/lib/composer/doc-to-prompt.ts',
  'src/renderer/lib/composer/retry-wire.ts',
  'src/renderer/lib/composer/selection-context.ts',
  'src/renderer/lib/composer/termul-tools.ts',
  'src/renderer/lib/composer/tool-tokens.ts',
  'src/renderer/lib/composer/wire-text.ts',
  'src/renderer/lib/conversation-envelope.ts',
  'src/renderer/lib/media-kind.ts',
  'src/renderer/lib/path-basename.ts',
  'src/renderer/lib/skill-tokens.ts',
  'src/renderer/lib/skills-prompt.ts'
]

export type DocPart =
  | { text: string }
  | { skill: string; path?: string }
  | { command: string }
  | { file: string; path: string }
  | { context: string; language?: string }
  | { conversation: string; sessionId: string }
  | { tool: string }

export interface ComposerCase {
  name: string
  doc: DocPart[]
  /** The agent's skills list, which names a path for a skill pill restored without one. */
  skills?: Array<{ name: string; path: string }>
  /** Resolved transcripts of mentioned chats; absent means they were not resolved. */
  contexts?: { gaps: Record<string, TranscriptGap>; blocks: ContentBlock[] }
  attachments?: PendingAttachment[]
}

export interface ComposerVector extends ComposerCase {
  display: string
  /** What the sent bubble's Copy puts on the clipboard (`ChatMessage`). */
  copy: string
  /** The send, or the error the composer shows instead of sending. */
  send: ReturnType<typeof buildComposerSend> | { error: string }
}

export interface ContextCase {
  raw: string
  language?: string
}

const schema = getSchema([
  StarterKit,
  SkillPill,
  CommandPill,
  FilePill,
  SelectionContextPill,
  ConversationPill,
  ToolPill
])

type PmJson = { type: string; text?: string; attrs?: Record<string, unknown> }

function pillJson(part: DocPart): PmJson {
  if ('skill' in part)
    return { type: SKILL_PILL_NODE, attrs: { name: part.skill, path: part.path ?? '' } }
  if ('command' in part) return { type: CMD_PILL_NODE, attrs: { name: part.command } }
  if ('file' in part)
    return { type: FILE_PILL_NODE, attrs: { display: part.file, absPath: part.path } }
  if ('context' in part) {
    return {
      type: CONTEXT_PILL_NODE,
      attrs: { text: part.context, language: part.language ?? null }
    }
  }
  if ('conversation' in part) {
    return {
      type: CONVERSATION_PILL_NODE,
      attrs: { title: part.conversation, sessionId: part.sessionId }
    }
  }
  if ('tool' in part) return { type: TOOL_PILL_NODE, attrs: { id: part.tool } }
  throw new Error(`not a pill: ${JSON.stringify(part)}`)
}

/** The editor doc for `parts`: a paragraph per `\n`-separated run. */
export function docJson(parts: DocPart[]): unknown {
  const paragraphs: PmJson[][] = [[]]
  for (const part of parts) {
    if (!('text' in part)) {
      paragraphs[paragraphs.length - 1].push(pillJson(part))
      continue
    }
    part.text.split('\n').forEach((line, i) => {
      if (i > 0) paragraphs.push([])
      if (line.length > 0) paragraphs[paragraphs.length - 1].push({ type: 'text', text: line })
    })
  }
  return {
    type: 'doc',
    content: paragraphs.map((content) =>
      content.length > 0 ? { type: 'paragraph', content } : { type: 'paragraph' }
    )
  }
}

/** `useChatComposer.buildPromptParts`, with skill paths from the pills as the pick recorded them. */
function promptParts(value: string, c: ComposerCase) {
  const picked = new Map<string, string>()
  for (const part of c.doc) {
    if ('skill' in part && part.path && !picked.has(part.skill)) picked.set(part.skill, part.path)
  }
  const contexts = c.contexts
    ? { gaps: new Map(Object.entries(c.contexts.gaps)), blocks: c.contexts.blocks }
    : undefined
  const resolvedSkills = extractSkillNames(value).map((name) => ({
    name,
    path: picked.get(name) ?? c.skills?.find((s) => s.name === name)?.path ?? ''
  }))
  const missingPath = resolvedSkills.find((s) => !s.path)
  if (missingPath) throw new Error(`Skill '${missingPath.name}' is missing a path`)
  const wireText = composerWireText(value, resolvedSkills, contexts)
  return {
    skills: resolvedSkills,
    hasSkills: resolvedSkills.length > 0,
    hasCommand: extractCommandNames(value).length > 0,
    hasSelectionContexts: hasSelectionContexts(value),
    wireText,
    displayText: value,
    wireWithCommand: wireText,
    displayWithCommand: value,
    wireTrimmed: wireText.trim(),
    displayTrimmed: value.trim(),
    fileBlocks: fileMentionBlocks(value),
    hasConversations: extractConversationRefs(value).length > 0,
    hasTools: hasToolTokens(value),
    contextBlocks: contexts?.blocks ?? []
  }
}

export function recordComposer(c: ComposerCase): ComposerVector {
  const display = docToDisplayText(schema.nodeFromJSON(docJson(c.doc)))
  let send: ComposerVector['send']
  try {
    send = buildComposerSend(promptParts(display, c), c.attachments ?? [])
  } catch (error) {
    send = { error: error instanceof Error ? error.message : String(error) }
  }
  const copy = sanitizeDisplayText(serializeSelectionContexts(display))
  return { ...c, display, copy, send }
}

export function recordContext(c: ContextCase): ContextCase & { context: unknown } {
  return { ...c, context: createSelectionContext(c.raw, c.language) }
}

const SKILL_PATH = '/tmp/world/alpha/.claude/skills/parity-skill/SKILL.md'
const image: PendingAttachment = {
  kind: 'image',
  id: 'img-1',
  name: 'pasted-image.png',
  mimeType: 'image/png',
  previewUrl: 'data:image/png;base64,iVBORw0KGgo=',
  base64: 'iVBORw0KGgo='
}
const fileRef: PendingAttachment = {
  kind: 'file-ref',
  id: '/tmp/world/alpha/notes/plan v2.md',
  name: 'plan v2.md',
  mimeType: 'text/markdown',
  path: '/tmp/world/alpha/notes/plan v2.md'
}

export const composerCases: ComposerCase[] = [
  {
    name: 'one_pill_of_each_type',
    doc: [
      { text: 'Use ' },
      { skill: 'parity-skill', path: SKILL_PATH },
      { text: ' on ' },
      { file: 'README.md', path: '/tmp/world/alpha/README.md' },
      { text: ' and ' },
      { command: 'review' },
      { text: ' with ' },
      { conversation: 'Scripted chat', sessionId: 'sess-1' },
      { text: ', ' },
      { tool: 'browser' },
      { text: '\nabout ' },
      { context: 'line one\n\nline three' },
      { text: ' ' }
    ]
  },
  { name: 'plain_text_is_trimmed', doc: [{ text: '  hello there \n' }] },
  { name: 'empty_doc_sends_nothing', doc: [] },
  { name: 'paragraphs_and_blank_lines', doc: [{ text: 'first\n\nthird\n' }] },
  {
    name: 'leading_command_stays_first_before_contexts',
    doc: [
      { command: 'review' },
      { text: ' compare ' },
      { context: 'const x = 1', language: 'ts' },
      { text: ' with ' },
      { context: 'uses ``` fences\nand `ticks`', language: '' }
    ]
  },
  {
    name: 'typed_markers_are_escaped',
    doc: [{ text: 'see [Context 1] and \\[Context 2] near ' }, { context: 'quoted' }]
  },
  {
    name: 'typed_markers_without_chips_need_the_label',
    doc: [{ text: 'Context 1 (selected from the conversation):\n> x\n\nsee [Context 1]' }]
  },
  {
    name: 'folder_pill_reads_bare',
    doc: [{ text: 'look in ' }, { file: 'src', path: '/tmp/world/alpha/src/' }]
  },
  {
    name: 'same_file_twice_links_once',
    doc: [
      { file: 'a.ts', path: '/w/a.ts' },
      { text: ' vs ' },
      { file: 'a.ts', path: '/w/a.ts' },
      { text: ' and ' },
      { file: 'photo.PNG', path: 'C:\\Users\\me\\photo.PNG' }
    ]
  },
  {
    name: 'skill_without_a_path_blocks_the_send',
    doc: [{ skill: 'gone' }, { text: ' run it' }]
  },
  {
    name: 'skill_path_from_the_skills_list',
    doc: [{ skill: 'listed' }, { text: ' twice ' }, { skill: 'listed' }],
    skills: [{ name: 'listed', path: '/skills/listed/SKILL.md' }]
  },
  { name: 'skill_alone', doc: [{ skill: 'parity-skill', path: SKILL_PATH }] },
  {
    name: 'conversation_with_resolved_transcript',
    doc: [
      { text: 'compare ' },
      { conversation: 'Fix  web\nreconnect ', sessionId: 'sess-2' },
      { text: ' and ' },
      { conversation: 'Old chat', sessionId: 'sess-3' }
    ],
    contexts: {
      gaps: { 'sess-3': 'partial' },
      blocks: [
        {
          type: 'resource',
          resource: { uri: 'termul-chat://sess-2', mimeType: 'text/plain', text: 'transcript' }
        }
      ]
    }
  },
  {
    name: 'tools_alone',
    doc: [{ tool: 'canvas' }, { text: ' then ' }, { tool: 'orchestrate' }, { tool: 'automations' }]
  },
  { name: 'image_only', doc: [{ text: '   ' }], attachments: [image] },
  {
    name: 'text_with_image_and_file',
    doc: [{ text: 'what is this?' }],
    attachments: [image, fileRef]
  },
  {
    name: 'pills_with_attachments_keep_display',
    doc: [{ command: 'explain' }, { text: ' ' }, { file: 'plan v2.md', path: fileRef.path }],
    attachments: [fileRef, image]
  },
  {
    name: 'unicode_text',
    doc: [{ text: 'naïve 日本語 😀 ' }, { context: 'emoji 😀 & spaces/slashes?' }]
  }
]

export const contextCases: ContextCase[] = [
  { raw: '  prose\r\nline  ' },
  { raw: '   ' },
  { raw: '```ts\nconst x = 1\n```' },
  { raw: '```\n\n```' },
  { raw: '\n\n  code line\n  more  \n', language: 'type/script!' },
  { raw: 'x', language: 'text' },
  { raw: ' \n ', language: 'rs' },
  { raw: 'has\uE000sentinels\u001Fhere' }
]
