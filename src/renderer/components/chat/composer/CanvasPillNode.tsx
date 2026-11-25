import { mergeAttributes, Node } from '@tiptap/core'
import { NodeViewWrapper, type ReactNodeViewProps, ReactNodeViewRenderer } from '@tiptap/react'
import { canvasToken } from '@/lib/composer/canvas-tokens'
import { CANVAS_PILL_NODE } from '@/lib/composer/doc-to-prompt'
import { cn } from '@/lib/utils'
import { CanvasChip } from '../CanvasChip'

/** `ids` is the comma-joined node ids (decimal text). */
export const CanvasPill = Node.create({
  name: CANVAS_PILL_NODE,
  group: 'inline',
  inline: true,
  atom: true,
  selectable: true,
  draggable: false,
  addAttributes() {
    return {
      label: { default: '' },
      ids: { default: '' }
    }
  },
  parseHTML() {
    return []
  },
  renderHTML({ node, HTMLAttributes }) {
    return [
      'span',
      mergeAttributes(HTMLAttributes, {
        'data-canvas-pill': '',
        'data-canvas-ids': node.attrs.ids
      })
    ]
  },
  renderText({ node }) {
    return canvasToken({
      label: String(node.attrs.label ?? ''),
      ids: String(node.attrs.ids ?? '').split(',')
    })
  },
  addNodeView() {
    return ReactNodeViewRenderer(CanvasPillNodeView)
  }
})

function CanvasPillNodeView({ node, selected }: ReactNodeViewProps): React.JSX.Element {
  const ids = String(node.attrs.ids ?? '')
  return (
    <NodeViewWrapper
      as="span"
      className={cn(
        'inline-flex align-baseline leading-none',
        selected && 'ring-2 ring-primary/40'
      )}
      data-canvas-pill="true"
      data-canvas-ids={ids}
    >
      <CanvasChip
        label={String(node.attrs.label ?? '')}
        count={ids === '' ? 0 : ids.split(',').length}
      />
    </NodeViewWrapper>
  )
}
