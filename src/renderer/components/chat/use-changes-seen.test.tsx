import { act, renderHook } from '@testing-library/react'
import { beforeEach, describe, expect, it } from 'vitest'
import { useChangesSeenStore } from '@/stores/changes-seen-store'
import { useGitSheetStore } from '@/stores/git-sheet-store'
import { resetShellLayoutStoreForTests, useShellLayoutStore } from '@/stores/shell-layout-store'
import { changesFingerprint, useChangesSeen } from './use-changes-seen'
import type { ChangedFile } from './use-chat-changed-files'

const file = (path: string, added = 1, removed = 0): ChangedFile => ({
  path,
  status: 'modified',
  added,
  removed
})

const A = [file('/w/a.ts')]

function render(files: readonly ChangedFile[] = A, projectId: string | null = 'p1') {
  return renderHook(
    (props: { files: readonly ChangedFile[] }) =>
      useChangesSeen({ sessionId: 's1', cwd: '/w', projectId, files: props.files }),
    { initialProps: { files } }
  )
}

beforeEach(() => {
  localStorage.clear()
  useChangesSeenStore.setState({ seen: {}, order: [] })
  resetShellLayoutStoreForTests()
  useShellLayoutStore.setState({ sidePanelVisible: false, lastColumnTabByProject: {} })
  useGitSheetStore.setState({ open: false, cwd: '', projectId: '' })
})

describe('changesFingerprint', () => {
  it('sorts path:added:removed:status lines', () => {
    expect(changesFingerprint([file('/w/b.ts', 2, 1), file('/w/a.ts')])).toBe(
      '/w/a.ts:1:0:modified\n/w/b.ts:2:1:modified'
    )
  })

  it('does not depend on the file order', () => {
    expect(changesFingerprint([file('/w/b.ts'), file('/w/a.ts')])).toBe(
      changesFingerprint([file('/w/a.ts'), file('/w/b.ts')])
    )
  })
})

describe('useChangesSeen', () => {
  it('is new on the first render when files exist', () => {
    expect(render().result.current.hasNew).toBe(true)
  })

  it('is not new without files', () => {
    expect(render([]).result.current.hasNew).toBe(false)
  })

  it('markSeen clears it, and a different fingerprint brings it back', () => {
    const { result, rerender } = render()
    act(() => result.current.markSeen())
    expect(result.current.hasNew).toBe(false)
    rerender({ files: [file('/w/a.ts', 5, 0)] })
    expect(result.current.hasNew).toBe(true)
  })

  it('keeps the seen state across a remount', () => {
    const first = render()
    act(() => first.result.current.markSeen())
    first.unmount()
    expect(render().result.current.hasNew).toBe(false)
  })

  it('marks seen by itself while the Git tab shows on desktop', () => {
    const { result } = render()
    expect(result.current.hasNew).toBe(true)
    act(() => {
      useShellLayoutStore.setState({
        sidePanelVisible: true,
        lastColumnTabByProject: { p1: 'changes' }
      })
    })
    expect(result.current.hasNew).toBe(false)
  })

  it('does not mark seen when the side panel is hidden or on another tab', () => {
    useShellLayoutStore.setState({
      sidePanelVisible: false,
      lastColumnTabByProject: { p1: 'changes' }
    })
    expect(render().result.current.hasNew).toBe(true)
    useShellLayoutStore.setState({
      sidePanelVisible: true,
      lastColumnTabByProject: { p1: 'files' }
    })
    expect(render().result.current.hasNew).toBe(true)
  })

  it('marks seen when new changes arrive while the Git tab shows', () => {
    useShellLayoutStore.setState({
      sidePanelVisible: true,
      lastColumnTabByProject: { p1: 'changes' }
    })
    const { result, rerender } = render()
    expect(result.current.hasNew).toBe(false)
    rerender({ files: [file('/w/a.ts'), file('/w/b.ts')] })
    expect(result.current.hasNew).toBe(false)
  })

  it('marks seen when the phone Git sheet opens on this folder', () => {
    const { result } = render()
    act(() => useGitSheetStore.setState({ open: true, cwd: '/other', projectId: 'p1' }))
    expect(result.current.hasNew).toBe(true)
    act(() => useGitSheetStore.setState({ open: true, cwd: '/w', projectId: 'p1' }))
    expect(result.current.hasNew).toBe(false)
  })

  it('ignores the desktop Git tab without a project id', () => {
    useShellLayoutStore.setState({
      sidePanelVisible: true,
      lastColumnTabByProject: { '': 'changes' }
    })
    expect(render(A, null).result.current.hasNew).toBe(true)
  })
})
