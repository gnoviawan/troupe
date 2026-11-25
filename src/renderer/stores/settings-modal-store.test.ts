import { beforeEach, describe, expect, it } from 'vitest'
import { useSettingsModalStore } from './settings-modal-store'

describe('settings-modal-store', () => {
  beforeEach(() => {
    useSettingsModalStore.setState({
      view: null,
      appPage: null,
      projectPage: null,
      projectId: null
    })
  })

  it('stores the target project on openProject', () => {
    useSettingsModalStore.getState().openProject(undefined, 'b')
    expect(useSettingsModalStore.getState().view).toBe('project')
    expect(useSettingsModalStore.getState().projectId).toBe('b')
  })

  it('clears projectId on close', () => {
    useSettingsModalStore.getState().openProject(undefined, 'b')
    useSettingsModalStore.getState().close()
    expect(useSettingsModalStore.getState().view).toBeNull()
    expect(useSettingsModalStore.getState().projectId).toBeNull()
  })

  it('clears projectId when switching to App Preferences', () => {
    useSettingsModalStore.getState().openProject(undefined, 'b')
    useSettingsModalStore.getState().openApp()
    expect(useSettingsModalStore.getState().view).toBe('app')
    expect(useSettingsModalStore.getState().projectId).toBeNull()
  })

  it.each([
    ['interface', 'preferences'],
    ['notifications', 'preferences'],
    ['editor', 'preferences'],
    ['appearance', 'terminal'],
    ['shell', 'terminal'],
    ['updates', 'about'],
    ['server', 'about'],
    ['diagnostics', 'about'],
    ['reset', 'about']
  ])('opens the merged page for the old page id "%s"', (oldId, newId) => {
    useSettingsModalStore.getState().openApp(oldId)
    expect(useSettingsModalStore.getState().appPage).toBe(newId)

    useSettingsModalStore.getState().setAppPage('provider')
    useSettingsModalStore.getState().setAppPage(oldId)
    expect(useSettingsModalStore.getState().appPage).toBe(newId)
  })

  it('keeps page ids that were not merged', () => {
    useSettingsModalStore.getState().openApp('mcp-servers')
    expect(useSettingsModalStore.getState().appPage).toBe('mcp-servers')
  })
})
