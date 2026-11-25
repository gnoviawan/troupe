/**
 * Runtime filtering on the About page: the updater rows are desktop-only
 * (issue #843) and the server rows are web-only (gh-1105). Both live on About,
 * so the page exists on every runtime and only its search entries change.
 */

import { describe, expect, it, vi } from 'vitest'

const tauriRef = vi.hoisted(() => ({ current: false }))
vi.mock('@/lib/tauri-runtime', () => ({ isTauriContext: () => tauriRef.current }))

import { rowId } from '@/components/settings/row-id'
import { appPrefCategories, appPrefSearchIndex } from './categories'

const anchorsOn = (tauri: boolean): Array<string | undefined> => {
  tauriRef.current = tauri
  return appPrefSearchIndex().map((entry) => entry.anchorId)
}

describe('appPrefCategories About page', () => {
  it('has About on the web runtime, with a server description', () => {
    tauriRef.current = false
    const about = appPrefCategories().find((category) => category.id === 'about')

    expect(about?.description).toContain('server')
  })

  it('has About on the desktop runtime, with an update description', () => {
    tauriRef.current = true
    const about = appPrefCategories().find((category) => category.id === 'about')

    expect(about?.description).toContain('updates')
  })
})

describe('appPrefSearchIndex runtime rows', () => {
  it('searches the server rows and not the updater rows on the web runtime', () => {
    const anchors = anchorsOn(false)

    expect(anchors).toContain(rowId('server-config'))
    expect(anchors).toContain(rowId('server-version'))
    expect(anchors).not.toContain(rowId('check-updates'))
    expect(anchors).not.toContain(rowId('release-channel'))
  })

  it('searches the updater rows and not the server rows on the desktop runtime', () => {
    const anchors = anchorsOn(true)

    expect(anchors).toContain(rowId('check-updates'))
    expect(anchors).toContain(rowId('auto-update'))
    expect(anchors).not.toContain(rowId('server-config'))
    expect(anchors).not.toContain(rowId('server-version'))
  })
})
