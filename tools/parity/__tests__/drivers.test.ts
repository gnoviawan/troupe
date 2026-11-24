import { expect, test } from 'bun:test'
import { ordered } from '../drivers/driver'
import { playwrightKeys } from '../drivers/react'
import { x11Keys } from '../drivers/x11'

test('playwrightKeys maps strokes to Playwright key names', () => {
  expect(playwrightKeys('ctrl-shift-k escape a')).toEqual(['Control+Shift+k', 'Escape', 'a'])
})

test('x11Keys maps a stroke to an X keysym combination', () => {
  expect(x11Keys('ctrl-shift-n')).toBe('Control_L+Shift_L+n')
  expect(x11Keys('enter')).toBe('Return')
  expect(x11Keys('alt-,')).toBe('Alt_L+comma')
})

test('ordered finds texts in order whatever their whitespace', () => {
  expect(ordered('alpha  gamma\nbeta', ['alpha', 'gamma', 'beta'])).toBe(true)
  expect(ordered('A B C', ['A   B', 'C'])).toBe(true)
  expect(ordered('alpha beta gamma', ['beta', 'alpha'])).toBe(false)
})
