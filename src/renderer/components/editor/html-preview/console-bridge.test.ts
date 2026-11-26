import { describe, expect, it } from 'vitest'
import { BRIDGE_SOURCE, injectConsoleBridge, parseBridgeMessage } from './console-bridge'

const bridgeAt = (html: string): number => injectConsoleBridge(html).indexOf('<script>(()=>{')

describe('injectConsoleBridge', () => {
  it('places the bridge right after the opening head tag', () => {
    const html = '<!doctype html><html><head lang="en"><title>t</title></head><body></body></html>'
    const out = injectConsoleBridge(html)

    expect(out.startsWith('<!doctype html><html><head lang="en"><script>(()=>{')).toBe(true)
    expect(out.endsWith('</script><title>t</title></head><body></body></html>')).toBe(true)
  })

  it('does not take a header element for the head', () => {
    const html = '<!DOCTYPE html><body><header>Top</header></body>'

    expect(bridgeAt(html)).toBe('<!DOCTYPE html>'.length)
  })

  it('goes after the doctype when there is no head, so the page keeps standards mode', () => {
    expect(bridgeAt('<!doctype html>\n<p>hi</p>')).toBe('<!doctype html>'.length)
  })

  it('goes at the start of a fragment', () => {
    const out = injectConsoleBridge('<p>hi</p>')

    expect(bridgeAt('<p>hi</p>')).toBe(0)
    expect(out.endsWith('</script><p>hi</p>')).toBe(true)
  })

  it('posts with the bridge source tag', () => {
    expect(injectConsoleBridge('')).toContain(`const S="${BRIDGE_SOURCE}"`)
  })
})

describe('parseBridgeMessage', () => {
  it('accepts a bridge message', () => {
    expect(parseBridgeMessage({ source: BRIDGE_SOURCE, level: 'warn', text: 'careful' })).toEqual({
      level: 'warn',
      text: 'careful'
    })
  })

  it('rejects other messages', () => {
    expect(parseBridgeMessage({ source: 'other', level: 'log', text: 'x' })).toBeNull()
    expect(parseBridgeMessage({ source: BRIDGE_SOURCE, level: 'debug', text: 'x' })).toBeNull()
    expect(parseBridgeMessage({ source: BRIDGE_SOURCE, level: 'log', text: 3 })).toBeNull()
    expect(parseBridgeMessage('troupe-html-preview')).toBeNull()
    expect(parseBridgeMessage(null)).toBeNull()
  })
})
