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

/** A lone image in its paragraph stays a block image, with its resize handles. */
function isInlineImage(image: HTMLElement): boolean {
  if (image.closest('td, th')) return true
  const paragraph = image.closest('p')
  if (!paragraph) return false
  return [...paragraph.childNodes].some(
    (node) =>
      node !== image &&
      !(node.nodeType === Node.TEXT_NODE && !node.textContent?.trim()) &&
      !(node instanceof HTMLElement && node.tagName === 'BR')
  )
}
