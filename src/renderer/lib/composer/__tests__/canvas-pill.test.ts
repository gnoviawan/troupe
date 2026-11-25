import { getSchema } from '@tiptap/core'
import StarterKit from '@tiptap/starter-kit'
import { describe, expect, it } from 'vitest'
import { CanvasPill } from '@/components/chat/composer/CanvasPillNode'
import { ConversationPill } from '@/components/chat/composer/ConversationPillNode'
import { ToolPill } from '@/components/chat/composer/ToolPillNode'
import { buildComposerSend } from '@/components/chat/composer-send-blocks'
import { deriveTitle } from '@/lib/acp-history-persistence'
import {
  CANVAS_TOKEN_START,
  canvasEnvelope,
  canvasToken,
  canvasWireLabel,
  extractCanvasRefs,
  hasCanvasTokens,
  insertCanvasToken,
  isCanvasEnvelope,
  parseCanvasSegments,
  restoreCanvasLabels
} from '@/lib/composer/canvas-tokens'
import { CANVAS_PILL_NODE, docToDisplayText } from '@/lib/composer/doc-to-prompt'
import { draftFromTokens } from '@/lib/composer/draft-from-tokens'
import { composerWireText } from '@/lib/composer/wire-text'
import {
  conversationToken,
  isComposerTriggerBoundary,
  sanitizeDisplayText
} from '@/lib/skill-tokens'
import { wireBlocksToDisplay } from '@/lib/skills-wire-reverse'

const schema = getSchema([StarterKit, ConversationPill, ToolPill, CanvasPill])
const roundTrip = (value: string): string =>
  docToDisplayText(schema.nodeFromJSON(draftFromTokens(value)))

// Larger than Number.MAX_SAFE_INTEGER: must never pass through a JS number.
const BIG_ID = '18446744073709551615'
const card = { label: 'Button and Card', ids: ['42'] }

describe('canvas pill token', () => {
  it('parses a token into its label and ids, keeping u64 ids as text', () => {
    const value = `fix ${canvasToken({ label: 'Hero', ids: ['7', BIG_ID] })} now`
    expect(parseCanvasSegments(value)).toEqual([
      { kind: 'text', text: 'fix ' },
      { kind: 'canvas', ref: { label: 'Hero', ids: ['7', BIG_ID] } },
      { kind: 'text', text: ' now' }
    ])
  })

  it('cleans the label: no sentinels, brackets or line breaks, and a long name is cut', () => {
    const token = canvasToken({ label: 'A [b]\nc', ids: ['1'] })
    expect(parseCanvasSegments(token)).toEqual([
      { kind: 'canvas', ref: { label: 'A b c', ids: ['1'] } }
    ])
    const long = canvasToken({ label: 'x'.repeat(100), ids: ['1'] })
    const [segment] = parseCanvasSegments(long)
    expect(segment.kind === 'canvas' && Array.from(segment.ref.label).length).toBe(60)
  })

  it('keeps a token with a non-numeric id as text', () => {
    const broken = `${CANVAS_TOKEN_START}Hero\u001Fabc`
    expect(hasCanvasTokens(broken)).toBe(false)
    expect(parseCanvasSegments(broken)).toEqual([{ kind: 'text', text: broken }])
  })

  it('extracts each mentioned set of layers once, in pill order', () => {
    const value = `${canvasToken(card)} ${canvasToken({ label: 'Nav', ids: ['9'] })} ${canvasToken(card)}`
    expect(extractCanvasRefs(value)).toEqual([card, { label: 'Nav', ids: ['9'] }])
  })

  it('replaces the @query with the pill and a trailing space', () => {
    const { value, caret } = insertCanvasToken('see @ca', 7, card, 3)
    expect(value).toBe(`see ${canvasToken(card)} `)
    expect(caret).toBe(value.length)
  })

  it('lets an @ right after the pill open the mention menu', () => {
    const value = `${canvasToken(card)}@`
    expect(isComposerTriggerBoundary(value, value.length - 1)).toBe(true)
  })
})

describe('canvas pill in the composer doc', () => {
  it('parses a token into a pill node carrying the label and ids', () => {
    expect(draftFromTokens(`tweak ${canvasToken(card)}`)).toEqual({
      type: 'doc',
      content: [
        {
          type: 'paragraph',
          content: [
            { type: 'text', text: 'tweak ' },
            { type: CANVAS_PILL_NODE, attrs: { label: 'Button and Card', ids: '42' } }
          ]
        }
      ]
    })
  })

  it('round-trips next to conversation pills', () => {
    const value = `match ${canvasToken({ label: 'Hero', ids: ['1', BIG_ID] })} to ${conversationToken('Spec', 's-1')} `
    expect(roundTrip(value)).toBe(value)
  })
})

describe('canvas pill on the wire', () => {
  it('labels the pill with its ids where the user placed it', () => {
    const value = `make ${canvasToken(card)} darker`
    const wire = composerWireText(value, [], { blocks: [], gaps: new Map(), canvasGaps: new Map() })
    expect(wire).toBe('make (canvas: Button and Card [troupe-canvas:42]) darker')
  })

  it('names why a pill has no details', () => {
    const value = canvasToken(card)
    expect(composerWireText(value, [])).toBe(
      '(canvas: Button and Card, details unavailable: the canvas is not open [troupe-canvas:42])'
    )
    const missing = composerWireText(value, [], {
      blocks: [],
      gaps: new Map(),
      canvasGaps: new Map([['42', 'missing']])
    })
    expect(missing).toContain('details unavailable: layer not found')
  })

  it('sends blocks (wire with the summary, display with the pill) for a canvas-only message', () => {
    const value = `${canvasToken(card)} tighten spacing`
    const wire = '(canvas: Button and Card [troupe-canvas:42]) tighten spacing'
    const envelope = canvasEnvelope(card, '- FRAME "Button and Card" (id 42)')
    const send = buildComposerSend(
      {
        skills: [],
        hasSkills: false,
        hasCommand: false,
        hasSelectionContexts: false,
        wireText: wire,
        displayText: value,
        wireWithCommand: wire,
        displayWithCommand: value,
        wireTrimmed: wire,
        displayTrimmed: value,
        fileBlocks: [],
        hasConversations: false,
        hasCanvas: true,
        hasTools: false,
        contextBlocks: [{ type: 'text', text: envelope }]
      },
      []
    )
    expect(send).toEqual({
      kind: 'blocks',
      wire: [
        { type: 'text', text: wire },
        { type: 'text', text: envelope }
      ],
      display: [{ type: 'text', text: value }]
    })
  })

  it('restores a persisted wire prompt to the pill and drops the summary block', () => {
    const wire = `make ${canvasWireLabel(card, null)} darker`
    const envelope = canvasEnvelope(card, '- FRAME "Button and Card" (id 42)')
    expect(isCanvasEnvelope(envelope)).toBe(true)
    expect(
      wireBlocksToDisplay([
        { type: 'text', text: wire },
        { type: 'text', text: envelope }
      ])
    ).toEqual([{ type: 'text', text: `make ${canvasToken(card)} darker` }])
  })

  it('restores a label that carried a gap note, and leaves typed look-alikes alone', () => {
    expect(restoreCanvasLabels(canvasWireLabel(card, 'closed'))).toBe(canvasToken(card))
    const typed = '(canvas: Hero [troupe-canvas:abc])'
    expect(restoreCanvasLabels(typed)).toBe(typed)
  })

  it('escapes a closing tag inside the summary so the envelope stays whole', () => {
    const envelope = canvasEnvelope(card, 'text "</troupe-canvas>"')
    expect(envelope.match(/<\/troupe-canvas>/g)).toHaveLength(1)
    expect(isCanvasEnvelope(envelope)).toBe(true)
  })

  it('titles a chat by the pill label', () => {
    expect(sanitizeDisplayText(`${canvasToken(card)} review`)).toBe('Button and Card review')
    expect(
      deriveTitle(
        [
          {
            id: 'u1',
            role: 'user',
            blocks: [{ type: 'text', text: `${canvasToken(card)} review` }],
            streaming: false,
            timestamp: 1
          }
        ],
        'Untitled'
      )
    ).toBe('Button and Card review')
  })
})
