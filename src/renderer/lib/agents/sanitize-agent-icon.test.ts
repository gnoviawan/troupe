import { readdirSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import DOMPurify from 'dompurify'
import { describe, expect, it } from 'vitest'
import { sanitizeInlineAgentSvg } from '@/lib/agents/sanitize-agent-icon'

const ICON_ROOT = join(__dirname, '../../assets/agent-icons')

function bundledSvgPaths(): string[] {
  return [ICON_ROOT, join(ICON_ROOT, 'acp')].flatMap((dir) =>
    readdirSync(dir)
      .filter((name) => name.endsWith('.svg'))
      .map((name) => join(dir, name))
  )
}

/** Every element with its attributes, minus the root size the sanitizer strips on purpose. */
function shape(svg: string): string[] {
  const host = document.createElement('div')
  host.innerHTML = svg
  const root = host.querySelector('svg')
  root?.removeAttribute('width')
  root?.removeAttribute('height')
  return Array.from(host.querySelectorAll('*')).map((element) => {
    const attributes = Array.from(element.attributes)
      .map((attribute) => `${attribute.name}=${attribute.value}`)
      .sort()
    return `${element.tagName.toLowerCase()}[${attributes.join(' ')}]`
  })
}

describe('sanitizeInlineAgentSvg', () => {
  it('accepts viewBox on the root svg only', () => {
    const svg = '<svg viewBox="0 0 16 16"><circle cx="8" cy="8" r="4" /></svg>'
    expect(sanitizeInlineAgentSvg(svg)).toContain('viewBox="0 0 16 16"')
  })

  it('rejects nested viewBox without root viewBox', () => {
    const svg = '<svg width="16" height="16"><g viewBox="0 0 1 1"></g></svg>'
    expect(sanitizeInlineAgentSvg(svg)).toBeNull()
  })

  it('removes elements that restyle the app, embed HTML or reach the network', () => {
    const svg = [
      '<svg viewBox="0 0 4 4">',
      '<style>body{display:none}</style>',
      '<image href="https://evil.example/ping.png" width="4" height="4"/>',
      '<a href="https://evil.example"><path d="M0 0h4v4z"/></a>',
      '<foreignObject><div>html</div></foreignObject>',
      '<script>alert(1)</script>',
      '</svg>'
    ].join('')
    expect(sanitizeInlineAgentSvg(svg)).toBe(
      '<svg viewBox="0 0 4 4"><path d="M0 0h4v4z"></path></svg>'
    )
  })

  it('drops hrefs and url() references that leave the icon', () => {
    const svg = [
      '<svg viewBox="0 0 4 4">',
      '<filter id="f"><feImage href="https://evil.example/ping.png"></feImage>',
      '<feImage xlink:href="data:image/svg+xml,x"></feImage></filter>',
      '<path fill="url(https://evil.example/fill)" d="M0 0h4v4z"/>',
      '<path style="fill: URL( \'https://evil.example/style\' )" d="M0 0h1v1z"/>',
      '<path style="fill: \\75 rl(https://evil.example/escaped)" d="M0 0h2v2z"/>',
      '</svg>'
    ].join('')
    expect(sanitizeInlineAgentSvg(svg)).toBe(
      [
        '<svg viewBox="0 0 4 4">',
        '<filter id="f"><feImage></feImage><feImage></feImage></filter>',
        '<path d="M0 0h4v4z"></path>',
        '<path d="M0 0h1v1z"></path>',
        '<path d="M0 0h2v2z"></path>',
        '</svg>'
      ].join('')
    )
  })

  it('keeps fragment references inside the icon', () => {
    const svg = [
      '<svg viewBox="0 0 4 4">',
      '<defs><linearGradient id="g"><stop offset="0" stop-color="#000"></stop></linearGradient></defs>',
      '<path id="p" fill="url(#g)" style="stroke: url(\'#g\')" d="M0 0h4v4z"></path>',
      '<filter id="f"><feImage href="#p"></feImage></filter>',
      '</svg>'
    ].join('')
    expect(sanitizeInlineAgentSvg(svg)).toBe(svg)
  })

  it('leaves other DOMPurify callers on the stock rules', () => {
    sanitizeInlineAgentSvg('<svg viewBox="0 0 1 1"></svg>')
    expect(DOMPurify.sanitize('<a href="https://example.com">x</a>')).toBe(
      '<a href="https://example.com">x</a>'
    )
  })

  it('passes every bundled icon through unchanged', () => {
    const paths = bundledSvgPaths()
    expect(paths.length).toBeGreaterThan(40)
    for (const path of paths) {
      const svg = readFileSync(path, 'utf8')
      const sanitized = sanitizeInlineAgentSvg(svg)
      expect(sanitized, path).not.toBeNull()
      expect(shape(sanitized ?? ''), path).toEqual(shape(svg))
    }
  })
})
