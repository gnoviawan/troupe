import { createInlineContentSpec } from '@blocknote/core'

const spec = createInlineContentSpec(
  {
    type: 'inlineImage',
    propSchema: {
      url: { default: '' },
      alt: { default: '' }
    },
    content: 'none'
  } as const,
  {
    render: (inlineContent, _update, editor) => {
      const { url, alt } = inlineContent.props
      const image = document.createElement('img')
      image.className = 'markdown-inline-image'
      image.alt = alt
      image.draggable = false
      if (editor.resolveFileUrl) {
        void editor.resolveFileUrl(url).then((src) => {
          image.src = src
        })
      } else {
        image.src = url
      }
      return { dom: image }
    },
    toExternalHTML: (inlineContent) => {
      const image = document.createElement('img')
      image.src = inlineContent.props.url
      image.alt = inlineContent.props.alt
      return { dom: image }
    },
    parse: (element) => {
      if (element.tagName !== 'IMG' || !isInlineImage(element)) return undefined
      return { url: element.getAttribute('src') ?? '', alt: element.getAttribute('alt') ?? '' }
    }
  }
)

/**
 * An image that sits in a line of text or a table cell. BlockNote only has a
 * block image, so without this the parser drops every image that is not alone
 * in its paragraph, and saving the file deletes them. The block image's `img`
 * rule would claim every image first and then fail to fit it in a cell or a
 * line, so these rules run before it.
 */
export const inlineImageSpec = {
  ...spec,
  implementation: {
    ...spec.implementation,
    node: spec.implementation.node.extend({
      parseHTML() {
        return (this.parent?.() ?? []).map((rule) => ({ ...rule, priority: 60 }))
      }
    })
  }
}

const INLINE_WRAPPERS = new Set(['A', 'B', 'DEL', 'EM', 'I', 'S', 'SPAN', 'STRONG', 'SUB', 'SUP'])
const INLINE_ONLY_BLOCKS = new Set(['H1', 'H2', 'H3', 'H4', 'H5', 'H6'])

/**
 * Heading and cell text only holds inline content. Elsewhere an image is
 * inline when its block has text or other images; a lone image, even inside
 * a link, stays a block image with its resize handles. List item content is
 * parsed from a detached copy, so the block is found by walking up past inline
 * wrappers rather than by looking for an `li`.
 */
function isInlineImage(image: HTMLElement): boolean {
  let block = image.parentElement
  while (block && INLINE_WRAPPERS.has(block.tagName)) block = block.parentElement
  if (!block) return false
  if (INLINE_ONLY_BLOCKS.has(block.tagName) || block.closest('td, th')) return true
  return block.textContent?.trim() !== '' || block.querySelectorAll('img').length > 1
}
