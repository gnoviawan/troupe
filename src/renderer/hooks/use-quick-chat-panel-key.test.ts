import { describe, expect, it } from 'vitest'
import { quickChatPanelKey } from './use-quick-chat-panel-key'

describe('quickChatPanelKey', () => {
  it('keys a first chat by its id', () => {
    expect(quickChatPanelKey(null, 'sess-1')).toBe('sess-1')
  })

  it('keeps the key while the same chat stays open', () => {
    expect(quickChatPanelKey({ sessionId: 'sess-1', key: 'k' }, 'sess-1')).toBe('k')
  })

  it('gives another chat a new key, so no draft carries over', () => {
    expect(quickChatPanelKey({ sessionId: 'sess-1', key: 'sess-1' }, 'sess-2')).toBe('sess-2')
  })

  it('keeps the key when the launch placeholder becomes its session', () => {
    expect(quickChatPanelKey({ sessionId: 'launch-1', key: 'launch-1' }, 'sess-9')).toBe('launch-1')
  })

  it('gives a new placeholder a new key', () => {
    expect(quickChatPanelKey({ sessionId: 'sess-1', key: 'sess-1' }, 'launch-2')).toBe('launch-2')
    expect(quickChatPanelKey({ sessionId: 'launch-1', key: 'launch-1' }, 'launch-2')).toBe(
      'launch-2'
    )
  })
})
