import { act, render, waitFor } from '@testing-library/react'
import { useRef } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger
} from '@/components/ui/dropdown-menu'
import {
  type BrowserTabView,
  browserTabCreate,
  browserTabDestroy,
  browserTabHide,
  browserTabNavigate,
  browserTabResize,
  browserTabScreenshot,
  browserTabShow,
  onBrowserTabUpdated
} from '@/lib/browser-api'
import { useBrowserSessionStore } from '@/stores/browser-session-store'
import { useWorkspaceStore } from '@/stores/workspace-store'
import { useBrowserWebview } from './use-browser-webview'
import {
  resetNativeWebviewOcclusionForTests,
  useNativeWebviewOcclusion
} from './use-native-webview-occlusion'
import { applyUiZoom } from './use-ui-zoom'

vi.mock('@/lib/tauri-runtime', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-runtime')>('@/lib/tauri-runtime')
  return { ...actual, isTauriContext: () => true }
})

vi.mock('@/lib/tauri-webview', () => ({
  getCurrentWebview: () => ({ setZoom: () => Promise.resolve() })
}))

vi.mock('@/lib/browser-api', () => ({
  browserTabCreate: vi.fn(),
  browserTabDestroy: vi.fn(),
  browserTabHide: vi.fn(),
  browserTabNavigate: vi.fn(),
  browserTabResize: vi.fn(),
  browserTabScreenshot: vi.fn(),
  browserTabShow: vi.fn(),
  onBrowserTabUpdated: vi.fn(() => ({ unlisten: vi.fn() }))
}))

let resizeCallback: ResizeObserverCallback | null = null
let observedElements: Element[] = []

class ResizeObserverMock {
  constructor(callback: ResizeObserverCallback) {
    resizeCallback = callback
  }

  observe(el: Element): void {
    observedElements.push(el)
  }
  disconnect(): void {}
  unobserve(): void {}
}

function BrowserWebviewHarness({ visible = true }: { visible?: boolean }): JSX.Element {
  const { containerRef, occlusionPreviewUrl } = useBrowserWebview(
    'browser-1',
    visible,
    'https://example.com'
  )
  return (
    <div ref={containerRef}>
      {occlusionPreviewUrl && (
        <img data-testid="occlusion-preview" src={occlusionPreviewUrl} alt="" />
      )}
    </div>
  )
}

/** A fixed logical-px rect (the contract: bounds stay in CSS/DIP units). */
const LOGICAL_RECT = {
  x: 120.5,
  y: 48.25,
  width: 640.75,
  height: 360.5,
  top: 48.25,
  right: 761.25,
  bottom: 408.75,
  left: 120.5,
  toJSON: () => ({})
}

describe('useBrowserWebview bounds updates', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    resizeCallback = null
    observedElements = []
    vi.stubGlobal('ResizeObserver', ResizeObserverMock)
    vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue(LOGICAL_RECT)
    vi.mocked(browserTabCreate).mockResolvedValue({
      success: true,
      data: { id: 'browser-1', url: 'https://example.com', title: '', agentControlled: false }
    })
    vi.mocked(browserTabDestroy).mockResolvedValue({ success: true, data: undefined })
    vi.mocked(browserTabResize).mockResolvedValue({ success: true, data: undefined })
    vi.mocked(browserTabScreenshot).mockResolvedValue({
      success: true,
      data: { base64: 'AA==', mimeType: 'image/png' }
    })
    vi.mocked(browserTabShow).mockResolvedValue({ success: true, data: undefined })
    vi.mocked(browserTabHide).mockResolvedValue({ success: true, data: undefined })
    useBrowserSessionStore.getState().createTab('browser-1', 'https://example.com')
  })

  afterEach(() => {
    useBrowserSessionStore.getState().removeTab('browser-1')
    vi.restoreAllMocks()
    vi.unstubAllGlobals()
  })

  it('keeps DOM bounds in logical pixels without resize diagnostic logging', async () => {
    const consoleLog = vi.spyOn(console, 'log').mockImplementation(() => {})
    const view = render(<BrowserWebviewHarness />)

    await waitFor(() => expect(browserTabCreate).toHaveBeenCalledTimes(1))
    expect(browserTabCreate).toHaveBeenCalledWith('browser-1', 'https://example.com', {
      x: 120.5,
      y: 48.25,
      width: 640.75,
      height: 360.5
    })

    // After creation resolves, the post-create resync polls frames until the
    // rect stabilizes, then re-measures + shows. Await the resync resize and
    // the show so the counts below are deterministic.
    await waitFor(() => expect(browserTabResize).toHaveBeenCalledTimes(1))
    expect(browserTabResize).toHaveBeenLastCalledWith('browser-1', {
      x: 120.5,
      y: 48.25,
      width: 640.75,
      height: 360.5
    })
    await waitFor(() => expect(browserTabShow).toHaveBeenCalledTimes(1))

    await act(async () => {
      resizeCallback?.([], {} as ResizeObserver)
      resizeCallback?.([], {} as ResizeObserver)
    })

    await waitFor(() => expect(browserTabResize).toHaveBeenCalledTimes(3))
    expect(browserTabResize).toHaveBeenLastCalledWith('browser-1', {
      x: 120.5,
      y: 48.25,
      width: 640.75,
      height: 360.5
    })
    expect(consoleLog).not.toHaveBeenCalled()

    view.unmount()
  })

  it('sends window-logical bounds under UI zoom: the CSS rect times the applied zoom', async () => {
    vi.mocked(HTMLElement.prototype.getBoundingClientRect).mockReturnValue({
      ...LOGICAL_RECT,
      x: 263,
      y: 85,
      width: 1264,
      height: 704
    })
    Object.defineProperty(window, '__TAURI_INTERNALS__', {
      value: {},
      configurable: true,
      writable: true
    })
    try {
      await applyUiZoom(1.25)
      const view = render(<BrowserWebviewHarness />)

      await waitFor(() => expect(browserTabCreate).toHaveBeenCalledTimes(1))
      expect(browserTabCreate).toHaveBeenCalledWith('browser-1', 'https://example.com', {
        x: 328.75,
        y: 106.25,
        width: 1580,
        height: 880
      })
      await waitFor(() => expect(browserTabResize).toHaveBeenCalledTimes(1))
      expect(browserTabResize).toHaveBeenLastCalledWith('browser-1', {
        x: 328.75,
        y: 106.25,
        width: 1580,
        height: 880
      })

      view.unmount()
    } finally {
      await applyUiZoom(1)
      delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__
    }
  })

  it('resyncs to settled bounds after creation, not stale creation bounds (#644)', async () => {
    // Simulate the pane layout still mid-transition at creation time: the
    // container reports a wide (unsettled) rect while browserTabCreate is in
    // flight, then settles to the real narrower pane width before the
    // post-create resync reads it. Without the resync the webview would keep
    // the wide creation bounds and overflow into the adjacent pane.
    let measureCount = 0
    const wideRect = { ...LOGICAL_RECT, width: 1646, right: 1766.5 }
    const settledRect = { ...LOGICAL_RECT, width: 928, right: 1048.5 }
    vi.mocked(HTMLElement.prototype.getBoundingClientRect).mockImplementation(function (
      this: HTMLElement
    ) {
      measureCount += 1
      // Creation-time measure (call 1) sees the wide/unsettled rect; every
      // later measure sees the settled rect.
      return measureCount === 1 ? wideRect : settledRect
    })

    const view = render(<BrowserWebviewHarness />)

    await waitFor(() => expect(browserTabCreate).toHaveBeenCalledTimes(1))
    // Created with the stale/wide bounds measured at call time.
    expect(browserTabCreate).toHaveBeenLastCalledWith(
      'browser-1',
      'https://example.com',
      expect.objectContaining({ width: 1646 })
    )

    // The post-create resync must re-measure and adopt the *settled* width,
    // proving the webview does not keep the stale 1646px creation bounds.
    await waitFor(() => expect(browserTabResize).toHaveBeenCalledTimes(1))
    expect(browserTabResize).toHaveBeenLastCalledWith(
      'browser-1',
      expect.objectContaining({ width: 928 })
    )
    expect(browserTabResize).not.toHaveBeenLastCalledWith(
      'browser-1',
      expect.objectContaining({ width: 1646 })
    )

    view.unmount()
  })

  it('resyncs bounds even when hidden at creation so a later show is correctly sized', async () => {
    // Matrix: "Browser hidden at creation" — the tab mounts inactive. The
    // post-create resync must still run (the deferred show relies on correct
    // bounds) and the webview is hidden, not shown.
    const view = render(<BrowserWebviewHarness visible={false} />)

    await waitFor(() => expect(browserTabCreate).toHaveBeenCalledTimes(1))
    await waitFor(() => expect(browserTabResize).toHaveBeenCalledTimes(1))
    expect(browserTabResize).toHaveBeenLastCalledWith(
      'browser-1',
      expect.objectContaining({ width: 640.75 })
    )
    // Hidden path: hide is invoked, show is not.
    expect(browserTabHide).toHaveBeenCalled()
    expect(browserTabShow).not.toHaveBeenCalled()

    view.unmount()
  })

  it('aborts the post-create resync if the component unmounts mid-poll (mount-token guard)', async () => {
    // Verification gap (VG1/BH5): the mount-token guard inside the resync
    // closure is the only thing preventing a stale resize/show/hide IPC after
    // unmount. Unmount immediately after creation resolves — before the stable-
    // bounds poll settles — and assert no show/resize fires post-destroy.
    const view = render(<BrowserWebviewHarness />)
    // Wait for create to resolve and flip createdRef, but unmount before the
    // rAF poll can settle the rect and call updateBounds/show.
    await waitFor(() => expect(browserTabCreate).toHaveBeenCalledTimes(1))
    view.unmount()
    // Drain any pending microtasks/rAF callbacks; the guard must short-circuit.
    await act(async () => {
      await Promise.resolve()
    })
    // The poll never reached its stable-frame branch, so no resync resize and
    // no show occurred (destroy ran in the effect cleanup instead).
    expect(browserTabResize).not.toHaveBeenCalled()
    expect(browserTabShow).not.toHaveBeenCalled()
    expect(browserTabDestroy).toHaveBeenCalled()
  })

  it('hides while a shell popover occludes and reveals only a visible pane afterwards', async () => {
    const OccluderHarness = ({ open }: { open: boolean }): null => {
      useNativeWebviewOcclusion(open)
      return null
    }
    const visible = render(<BrowserWebviewHarness />)
    await waitFor(() => expect(browserTabShow).toHaveBeenCalled())
    vi.mocked(browserTabShow).mockClear()
    vi.mocked(browserTabHide).mockClear()

    const occluder = render(<OccluderHarness open />)
    await waitFor(() => expect(browserTabScreenshot).toHaveBeenCalledWith('browser-1'))
    await waitFor(() => expect(visible.queryByTestId('occlusion-preview')).toBeTruthy())
    await waitFor(() => expect(browserTabHide).toHaveBeenCalledWith('browser-1'))
    expect(browserTabShow).not.toHaveBeenCalled()

    occluder.rerender(<OccluderHarness open={false} />)
    await waitFor(() => expect(browserTabShow).toHaveBeenCalledWith('browser-1'))
    await waitFor(() => expect(visible.queryByTestId('occlusion-preview')).toBeNull())

    occluder.unmount()
    visible.unmount()
    resetNativeWebviewOcclusionForTests()
  })

  it('hides while a shared menu primitive is open and reveals once it closes', async () => {
    const Page = ({ menuOpen }: { menuOpen: boolean }): JSX.Element => (
      <>
        <BrowserWebviewHarness />
        <DropdownMenu open={menuOpen}>
          <DropdownMenuTrigger>Tab</DropdownMenuTrigger>
          <DropdownMenuContent>
            <DropdownMenuItem>Close</DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </>
    )
    const view = render(<Page menuOpen={false} />)
    await waitFor(() => expect(browserTabShow).toHaveBeenCalledTimes(1))
    expect(browserTabHide).not.toHaveBeenCalled()

    view.rerender(<Page menuOpen />)
    await waitFor(() => expect(browserTabHide).toHaveBeenCalledTimes(1))
    expect(browserTabHide).toHaveBeenLastCalledWith('browser-1')

    view.rerender(<Page menuOpen={false} />)
    await waitFor(() => expect(browserTabShow).toHaveBeenCalledTimes(2))
    expect(browserTabShow).toHaveBeenLastCalledWith('browser-1')

    view.unmount()
    resetNativeWebviewOcclusionForTests()
  })

  it('keeps a pane-hidden webview hidden after the occluder closes', async () => {
    const OccluderHarness = ({ open }: { open: boolean }): null => {
      useNativeWebviewOcclusion(open)
      return null
    }
    const view = render(<BrowserWebviewHarness visible={false} />)
    await waitFor(() => expect(browserTabCreate).toHaveBeenCalledTimes(1))
    await waitFor(() => expect(browserTabResize).toHaveBeenCalledTimes(1))

    const occluder = render(<OccluderHarness open />)
    occluder.rerender(<OccluderHarness open={false} />)
    await act(async () => {
      await Promise.resolve()
    })
    expect(browserTabShow).not.toHaveBeenCalled()

    occluder.unmount()
    view.unmount()
    resetNativeWebviewOcclusionForTests()
  })

  it('drops an in-flight occlusion snapshot if the dropdown closes before capture resolves', async () => {
    let resolveShot:
      | ((val: { success: true; data: { base64: string; mimeType: string } }) => void)
      | null = null
    vi.mocked(browserTabScreenshot).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveShot = resolve
        })
    )
    const OccluderHarness = ({ open }: { open: boolean }): null => {
      useNativeWebviewOcclusion(open)
      return null
    }
    const visible = render(<BrowserWebviewHarness />)
    await waitFor(() => expect(browserTabShow).toHaveBeenCalled())

    const occluder = render(<OccluderHarness open />)
    await waitFor(() => expect(browserTabScreenshot).toHaveBeenCalledWith('browser-1'))

    // Close dropdown before screenshot resolves
    occluder.rerender(<OccluderHarness open={false} />)
    await act(async () => {
      resolveShot?.({ success: true, data: { base64: 'AA==', mimeType: 'image/png' } })
      await Promise.resolve()
    })

    expect(visible.queryByTestId('occlusion-preview')).toBeNull()
    occluder.unmount()
    visible.unmount()
    resetNativeWebviewOcclusionForTests()
  })
})

describe('useBrowserWebview agent control', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.stubGlobal('ResizeObserver', ResizeObserverMock)
    vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue(LOGICAL_RECT)
    vi.mocked(browserTabDestroy).mockResolvedValue({ success: true, data: undefined })
    vi.mocked(browserTabResize).mockResolvedValue({ success: true, data: undefined })
    vi.mocked(browserTabScreenshot).mockResolvedValue({
      success: true,
      data: { base64: 'AA==', mimeType: 'image/png' }
    })
    vi.mocked(browserTabShow).mockResolvedValue({ success: true, data: undefined })
    vi.mocked(browserTabHide).mockResolvedValue({ success: true, data: undefined })
    useBrowserSessionStore.getState().createTab('browser-1', 'https://example.com')
  })

  afterEach(() => {
    useBrowserSessionStore.getState().removeTab('browser-1')
    vi.restoreAllMocks()
    vi.unstubAllGlobals()
  })

  function createResolves(agentControlled: boolean): void {
    vi.mocked(browserTabCreate).mockResolvedValue({
      success: true,
      data: { id: 'browser-1', url: 'https://example.com', title: '', agentControlled }
    })
  }

  it('clears a stale Agent flag when the host did not claim the created webview', async () => {
    useBrowserSessionStore.getState().setAgentControlled('browser-1', true)
    createResolves(false)

    const view = render(<BrowserWebviewHarness />)

    await waitFor(() =>
      expect(useBrowserSessionStore.getState().getTab('browser-1')?.agentControlled).toBe(false)
    )
    view.unmount()
  })

  it('flags the tab once the host claims the created webview for an agent', async () => {
    createResolves(true)

    const view = render(<BrowserWebviewHarness />)

    await waitFor(() =>
      expect(useBrowserSessionStore.getState().getTab('browser-1')?.agentControlled).toBe(true)
    )
    view.unmount()
  })
})

describe('useBrowserWebview registry projection', () => {
  let publish: ((view: BrowserTabView) => void) | null = null

  beforeEach(() => {
    vi.clearAllMocks()
    vi.stubGlobal('ResizeObserver', ResizeObserverMock)
    vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue(LOGICAL_RECT)
    vi.mocked(onBrowserTabUpdated).mockImplementation((callback) => {
      publish = callback
      return { unlisten: vi.fn() }
    })
    vi.mocked(browserTabCreate).mockResolvedValue({
      success: true,
      data: { id: 'browser-1', url: 'https://example.com', title: '', agentControlled: false }
    })
    vi.mocked(browserTabDestroy).mockResolvedValue({ success: true, data: undefined })
    vi.mocked(browserTabResize).mockResolvedValue({ success: true, data: undefined })
    vi.mocked(browserTabShow).mockResolvedValue({ success: true, data: undefined })
    useBrowserSessionStore.getState().createTab('browser-1', 'https://example.com')
  })

  afterEach(() => {
    useBrowserSessionStore.getState().removeTab('browser-1')
    publish = null
    vi.restoreAllMocks()
    vi.unstubAllGlobals()
  })

  function UrlFollowingHarness(): JSX.Element {
    const url = useBrowserSessionStore((s) => s.tabs.get('browser-1')?.url ?? '')
    const { containerRef } = useBrowserWebview('browser-1', true, url)
    return <div ref={containerRef} />
  }

  it('mirrors a navigation the page made without navigating the webview again', async () => {
    const view = render(<UrlFollowingHarness />)
    await waitFor(() => expect(browserTabCreate).toHaveBeenCalled())

    act(() => {
      publish?.({
        tabId: 'browser-1',
        url: 'https://example.com/clicked',
        title: 'Clicked',
        loading: false,
        agentControlled: false,
        epoch: 2
      })
      publish?.({
        tabId: 'browser-2',
        url: 'https://other.test/',
        title: 'Other tab',
        loading: true,
        agentControlled: true,
        epoch: 1
      })
    })

    expect(useBrowserSessionStore.getState().getTab('browser-1')).toMatchObject({
      url: 'https://example.com/clicked',
      title: 'Clicked',
      loading: false
    })
    expect(browserTabNavigate).not.toHaveBeenCalled()
    view.unmount()
  })
})

describe('useBrowserWebview refused agent tab', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.stubGlobal('ResizeObserver', ResizeObserverMock)
    vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue(LOGICAL_RECT)
    vi.mocked(browserTabDestroy).mockResolvedValue({ success: true, data: undefined })
    vi.mocked(browserTabCreate).mockResolvedValue({
      success: false,
      error: 'no agent asked for this tab',
      code: 'BROWSER_TAB_UNKNOWN_AGENT_TAB'
    })
    useBrowserSessionStore.getState().createTab('browser-1', 'https://example.com')
    useWorkspaceStore.getState().addBrowserTab('browser-1')
  })

  afterEach(() => {
    vi.restoreAllMocks()
    vi.unstubAllGlobals()
  })

  it('drops the pane tab of an agent tab the host no longer waits on', async () => {
    expect(JSON.stringify(useWorkspaceStore.getState().root)).toContain('browser-browser-1')
    const view = render(<BrowserWebviewHarness />)
    await waitFor(() =>
      expect(useBrowserSessionStore.getState().getTab('browser-1')).toBeUndefined()
    )
    expect(JSON.stringify(useWorkspaceStore.getState().root)).not.toContain('browser-browser-1')
    view.unmount()
  })
})

function StageHarness({ layoutKey = 'mobile-portrait-preset' }: { layoutKey?: string }) {
  const stageRef = useRef<HTMLDivElement>(null)
  const { containerRef } = useBrowserWebview('browser-1', true, 'https://example.com', {
    layoutKey,
    stageRef
  })
  return (
    <div data-testid="panel-root">
      <div ref={stageRef} data-testid="stage-root">
        <div ref={containerRef} data-testid="container-root" />
      </div>
    </div>
  )
}

describe('useBrowserWebview responsive stage bounds resync', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    resizeCallback = null
    observedElements = []
    vi.stubGlobal('ResizeObserver', ResizeObserverMock)
    vi.mocked(browserTabCreate).mockResolvedValue({
      success: true,
      data: { id: 'browser-1', url: 'https://example.com', title: '', agentControlled: false }
    })
    vi.mocked(browserTabDestroy).mockResolvedValue({ success: true, data: undefined })
    vi.mocked(browserTabResize).mockResolvedValue({ success: true, data: undefined })
    vi.mocked(browserTabScreenshot).mockResolvedValue({
      success: true,
      data: { base64: 'AA==', mimeType: 'image/png' }
    })
    vi.mocked(browserTabShow).mockResolvedValue({ success: true, data: undefined })
    vi.mocked(browserTabHide).mockResolvedValue({ success: true, data: undefined })
    useBrowserSessionStore.getState().createTab('browser-1', 'https://example.com')
  })

  afterEach(async () => {
    delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__
    useBrowserSessionStore.getState().removeTab('browser-1')
    vi.restoreAllMocks()
    vi.unstubAllGlobals()
  })

  it('observes stageRef and parent panel, and resyncs on window resize and accordion transitionend even when container size is fixed', async () => {
    let rect = {
      x: 400,
      y: 80,
      width: 390,
      height: 844,
      top: 80,
      right: 790,
      bottom: 924,
      left: 400,
      toJSON: () => ({})
    }
    vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(() => rect)

    const view = render(<StageHarness />)
    await waitFor(() => expect(browserTabCreate).toHaveBeenCalledTimes(1))
    await waitFor(() => expect(browserTabShow).toHaveBeenCalledTimes(1))

    // Both containerRef and stageRef (plus panel wrapper) are observed
    expect(observedElements).toContain(view.getByTestId('container-root'))
    expect(observedElements).toContain(view.getByTestId('stage-root'))
    expect(observedElements).toContain(view.getByTestId('panel-root'))

    vi.mocked(browserTabResize).mockClear()

    // Simulate sidebar toggle shifting x from 400 -> 260 while width/height stay 390x844
    rect = { ...rect, x: 260, left: 260, right: 650 }
    act(() => {
      resizeCallback?.([], {} as ResizeObserver)
    })
    await waitFor(() =>
      expect(browserTabResize).toHaveBeenLastCalledWith('browser-1', {
        x: 260,
        y: 80,
        width: 390,
        height: 844
      })
    )

    // Simulate window resize shifting x from 260 -> 310
    rect = { ...rect, x: 310, left: 310, right: 700 }
    act(() => {
      window.dispatchEvent(new Event('resize'))
    })
    await waitFor(() =>
      expect(browserTabResize).toHaveBeenLastCalledWith('browser-1', {
        x: 310,
        y: 80,
        width: 390,
        height: 844
      })
    )

    // Simulate ResponsiveViewportBar accordion transitionend shifting y from 80 -> 118
    rect = { ...rect, y: 118, top: 118, bottom: 962 }
    act(() => {
      const ev = new Event('transitionend', { bubbles: true }) as TransitionEvent
      Object.defineProperty(ev, 'propertyName', { value: 'grid-template-rows' })
      view.getByTestId('panel-root').dispatchEvent(ev)
    })
    await waitFor(() =>
      expect(browserTabResize).toHaveBeenLastCalledWith('browser-1', {
        x: 310,
        y: 118,
        width: 390,
        height: 844
      })
    )

    view.unmount()
  })

  it('resyncs logical bounds when whole-UI zoom changes even without ResizeObserver firing', async () => {
    ;(window as unknown as { __TAURI_INTERNALS__?: Record<string, unknown> }).__TAURI_INTERNALS__ =
      {}
    vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue({
      x: 200,
      y: 100,
      width: 390,
      height: 844,
      top: 100,
      right: 590,
      bottom: 944,
      left: 200,
      toJSON: () => ({})
    })

    const view = render(<StageHarness />)
    await waitFor(() => expect(browserTabShow).toHaveBeenCalledTimes(1))
    vi.mocked(browserTabResize).mockClear()

    await act(async () => {
      await applyUiZoom(1.25)
    })

    await waitFor(() =>
      expect(browserTabResize).toHaveBeenLastCalledWith('browser-1', {
        x: 250,
        y: 125,
        width: 487.5,
        height: 1055
      })
    )

    await act(async () => {
      await applyUiZoom(1)
    })
    view.unmount()
  })

  it('navigates native webview to the latest requested URL if URL changed while create was pending', async () => {
    let resolveCreate:
      | ((val: {
          success: true
          data: { id: string; url: string; title: string; agentControlled: boolean }
        }) => void)
      | null = null
    vi.mocked(browserTabCreate).mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveCreate = resolve
        })
    )
    vi.mocked(browserTabNavigate).mockResolvedValue({ success: true, data: undefined })

    function DynamicUrlHarness({ url }: { url: string }) {
      const { containerRef } = useBrowserWebview('browser-1', true, url)
      return <div ref={containerRef} />
    }

    const view = render(<DynamicUrlHarness url="https://first.example" />)
    await waitFor(() => expect(browserTabCreate).toHaveBeenCalledTimes(1))

    // User requests a newer URL while browserTabCreate is still in flight
    view.rerender(<DynamicUrlHarness url="https://second.example" />)

    await act(async () => {
      resolveCreate?.({
        success: true,
        data: {
          id: 'browser-1',
          url: 'https://first.example',
          title: '',
          agentControlled: false
        }
      })
    })

    await waitFor(() =>
      expect(browserTabNavigate).toHaveBeenCalledWith('browser-1', 'https://second.example')
    )
    view.unmount()
  })
})
