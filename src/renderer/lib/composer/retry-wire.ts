import {
  attachmentToBlock,
  dedupeAttachmentBlocks,
  guessMimeType
} from '@/components/chat/chat-attachments'
import type { ContentBlock } from '@/lib/acp-api'
import { resolveCanvasContexts } from '@/lib/canvas-mention'
import {
  type ConversationContexts,
  resolveConversationContexts
} from '@/lib/conversation-transcript'
import {
  extractConversationRefs,
  isFolderTokenPath,
  parseFileSegments,
  replaceCommandTokensInline
} from '@/lib/skill-tokens'
import type { FramedSkill } from '@/lib/skills-prompt'
import { extractCanvasRefs } from './canvas-tokens'
import { serializeSelectionContexts } from './selection-context'
import { composerWireText } from './wire-text'

/**
 * One `resource_link` block per unique mentioned file in a composer value, so
 * the agent gets the real path next to the `(display)` text. Folder mentions
 * ride as text only.
 */
export function fileMentionBlocks(value: string): ContentBlock[] {
  const blocks: ContentBlock[] = []
  const seenPaths = new Set<string>()
  for (const seg of parseFileSegments(
    replaceCommandTokensInline(serializeSelectionContexts(value))
  )) {
    if (seg.kind !== 'file' || isFolderTokenPath(seg.absPath) || seenPaths.has(seg.absPath)) {
      continue
    }
    seenPaths.add(seg.absPath)
    blocks.push(
      attachmentToBlock({
        kind: 'file-ref',
        id: seg.absPath,
        name: seg.display,
        mimeType: guessMimeType(seg.display),
        path: seg.absPath
      })
    )
  }
  return blocks
}

/**
 * Resolve what a composer value's pills point at: each mentioned
 * conversation's transcript and each mentioned canvas layer's summary, read
 * now from `projectId`'s canvas. `null` when the value mentions neither.
 */
export function resolveMentionContexts(
  tokenText: string,
  projectId: string | null | undefined
): Promise<ConversationContexts> | null {
  const conversationRefs = extractConversationRefs(tokenText)
  const canvasRefs = extractCanvasRefs(tokenText)
  if (conversationRefs.length === 0 && canvasRefs.length === 0) return null
  return Promise.all([
    conversationRefs.length > 0
      ? resolveConversationContexts(conversationRefs)
      : { blocks: [], gaps: new Map() },
    canvasRefs.length > 0 ? resolveCanvasContexts(projectId, canvasRefs) : null
  ]).then(([conversations, canvas]) => ({
    blocks: [...conversations.blocks, ...(canvas?.blocks ?? [])],
    gaps: conversations.gaps,
    ...(canvas ? { canvasGaps: canvas.gaps } : {})
  }))
}

/**
 * The wire blocks for re-sending a user message from its display (token)
 * blocks, built like a fresh composer send: the wire text, the original
 * non-text blocks, a `resource_link` per mentioned file, and the transcript of
 * each mentioned conversation and canvas layer (re-read now; the display holds
 * only the pill). Canvas pills are read from `projectId`'s canvas.
 */
export async function retryWireBlocks(
  displayBlocks: readonly ContentBlock[],
  skills: FramedSkill[],
  projectId?: string | null
): Promise<ContentBlock[]> {
  const tokenText = displayBlocks
    .filter((b) => b.type === 'text')
    .map((b) => b.text ?? '')
    .join('')
  const contexts = (await resolveMentionContexts(tokenText, projectId)) ?? undefined
  const wireText = composerWireText(tokenText, skills, contexts)
  const blocks: ContentBlock[] = wireText.trim() ? [{ type: 'text', text: wireText }] : []
  for (const b of displayBlocks) {
    if (b.type !== 'text') blocks.push(b)
  }
  blocks.push(...fileMentionBlocks(tokenText), ...(contexts?.blocks ?? []))
  return dedupeAttachmentBlocks(blocks)
}
