import { act, cleanup, render } from '@testing-library/react'
import { useEffect } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
  type EndScroller,
  MessageScrollerContent,
  MessageScrollerProvider,
  MessageScrollerViewport,
  useMessageScroller
} from './message-scroller'

/** ResizeObserver stand-in that can fire the callback for one observed element. */
const observed = new Map<Element, () => void>()
class ResizeObserverStub {
  constructor(private readonly callback: () => void) {}
  observe(element: Element): void {
    observed.set(element, this.callback)
  }
  unobserve(): void {}
  disconnect(): void {}
}

function resize(element: Element): void {
  act(() => observed.get(element)?.())
}

let original: typeof ResizeObserver
beforeEach(() => {
  original = window.ResizeObserver
  window.ResizeObserver = ResizeObserverStub as unknown as typeof ResizeObserver
  observed.clear()
})
afterEach(() => {
  window.ResizeObserver = original
})

/** Puts a pinned viewport 400 px above the content end. */
function setGeometry(viewport: HTMLElement): void {
  Object.defineProperty(viewport, 'scrollHeight', { configurable: true, value: 1000 })
  Object.defineProperty(viewport, 'clientHeight', { configurable: true, value: 600 })
}

function Scroller({
  followContent,
  endScroller,
  onReady
}: {
  followContent: boolean
  endScroller?: EndScroller
  onReady?: (scrollToEnd: (behavior?: ScrollBehavior) => void) => void
}): React.JSX.Element {
  return (
    <MessageScrollerProvider followContent={followContent}>
      <MessageScrollerViewport data-testid="viewport">
        <MessageScrollerContent data-testid="content">
          <Owner endScroller={endScroller} onReady={onReady} />
        </MessageScrollerContent>
      </MessageScrollerViewport>
    </MessageScrollerProvider>
  )
}

function Owner({
  endScroller,
  onReady
}: {
  endScroller?: EndScroller
  onReady?: (scrollToEnd: (behavior?: ScrollBehavior) => void) => void
}): null {
  const { setEndScroller, scrollToEnd } = useMessageScroller()
  useEffect(() => {
    if (!endScroller) return
    setEndScroller(endScroller)
    return () => setEndScroller(null)
  }, [endScroller, setEndScroller])
  useEffect(() => onReady?.(scrollToEnd), [onReady, scrollToEnd])
  return null
}

describe('MessageScrollerProvider scroll ownership', () => {
  it('follows content growth by default', () => {
    const view = render(<Scroller followContent />)
    const viewport = view.getByTestId('viewport')
    setGeometry(viewport)

    resize(view.getByTestId('content'))
    expect(viewport.scrollTop).toBe(1000)
  })

  it('leaves content growth to the list when followContent is off', () => {
    const view = render(<Scroller followContent={false} />)
    const viewport = view.getByTestId('viewport')
    setGeometry(viewport)

    resize(view.getByTestId('content'))
    expect(viewport.scrollTop).toBe(0)
  })

  it('sends viewport resizes and jump-to-latest through the registered end scroller', () => {
    const endScroller = vi.fn<EndScroller>()
    let scrollToEnd: ((behavior?: ScrollBehavior) => void) | undefined
    const view = render(
      <Scroller
        followContent={false}
        endScroller={endScroller}
        onReady={(fn) => {
          scrollToEnd = fn
        }}
      />
    )
    const viewport = view.getByTestId('viewport')
    setGeometry(viewport)
    const scrollTo = vi.spyOn(viewport, 'scrollTo')

    resize(viewport)
    expect(endScroller).toHaveBeenLastCalledWith('auto')

    act(() => scrollToEnd?.('smooth'))
    expect(endScroller).toHaveBeenLastCalledWith('smooth')
    expect(scrollTo).not.toHaveBeenCalled()
    expect(viewport.scrollTop).toBe(0)
  })
})

describe('MessageScrollerViewport scroll anchor', () => {
  it('renders the scroll anchor as the last child, after the content', () => {
    const view = render(<Scroller followContent={false} />)
    const viewport = view.getByTestId('viewport')
    const anchor = viewport.lastElementChild

    expect(anchor?.getAttribute('data-slot')).toBe('message-scroller-anchor')
    expect(anchor?.previousElementSibling).toBe(view.getByTestId('content'))
    expect(anchor?.getAttribute('aria-hidden')).toBe('true')
  })
})

function PinnedProbe(): React.JSX.Element {
  const { pinned, showButton } = useMessageScroller()
  return <output data-testid="probe" data-pinned={pinned} data-button={showButton} />
}

function scrollViewportTo(viewport: HTMLElement, top: number): void {
  viewport.scrollTop = top
  act(() => {
    viewport.dispatchEvent(new Event('scroll'))
  })
}

describe('MessageScrollerProvider reader intent', () => {
  function renderNearEnd(followContent = true): {
    viewport: HTMLElement
    content: HTMLElement
    probe: HTMLElement
  } {
    const view = render(
      <MessageScrollerProvider followContent={followContent}>
        <MessageScrollerViewport data-testid="viewport">
          <MessageScrollerContent data-testid="content">
            <PinnedProbe />
          </MessageScrollerContent>
        </MessageScrollerViewport>
      </MessageScrollerProvider>
    )
    const viewport = view.getByTestId('viewport')
    setGeometry(viewport)
    // 10 px above the end: inside the 48 px threshold, so still pinned.
    scrollViewportTo(viewport, 390)
    return { viewport, content: view.getByTestId('content'), probe: view.getByTestId('probe') }
  }

  it('lets go of the live edge on a wheel up, before the reader leaves the threshold', () => {
    const { viewport, probe } = renderNearEnd()
    expect(probe.dataset.pinned).toBe('true')

    act(() => {
      viewport.dispatchEvent(new WheelEvent('wheel', { deltaY: -40, bubbles: true }))
    })
    expect(probe.dataset.pinned).toBe('false')
    expect(probe.dataset.button).toBe('true')

    // Still within 48 px of the end, but the reader asked to leave.
    scrollViewportTo(viewport, 370)
    expect(probe.dataset.pinned).toBe('false')
  })

  it('lets go on PageUp, ArrowUp and Home', () => {
    for (const key of ['PageUp', 'ArrowUp', 'Home']) {
      const { viewport, probe } = renderNearEnd()
      act(() => {
        viewport.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true }))
      })
      expect(probe.dataset.pinned).toBe('false')
      cleanup()
    }
  })

  it('keeps following when the arrow keys move a caret in a field inside the log', () => {
    const { viewport, probe } = renderNearEnd()
    const field = document.createElement('textarea')
    viewport.append(field)
    act(() => {
      field.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowUp', bubbles: true }))
    })
    expect(probe.dataset.pinned).toBe('true')
  })

  it('lets go when the reader drags the content down with a finger', () => {
    const { viewport, probe } = renderNearEnd()
    act(() => {
      viewport.dispatchEvent(touchEvent('touchstart', 100))
      viewport.dispatchEvent(touchEvent('touchmove', 130))
    })
    expect(probe.dataset.pinned).toBe('false')
  })

  it('treats a scroll up without a wheel or key (scrollbar drag) as leaving', () => {
    const { viewport, probe } = renderNearEnd()
    scrollViewportTo(viewport, 380)
    expect(probe.dataset.pinned).toBe('false')
  })

  it('stops following content growth after the reader lets go', () => {
    const { viewport, content } = renderNearEnd(true)
    act(() => {
      viewport.dispatchEvent(new WheelEvent('wheel', { deltaY: -40, bubbles: true }))
    })
    resize(content)
    expect(viewport.scrollTop).toBe(390)
  })

  it('pins again when the reader scrolls back to the end', () => {
    const { viewport, probe } = renderNearEnd()
    act(() => {
      viewport.dispatchEvent(new WheelEvent('wheel', { deltaY: -40, bubbles: true }))
    })
    scrollViewportTo(viewport, 200)
    expect(probe.dataset.pinned).toBe('false')

    scrollViewportTo(viewport, 400)
    expect(probe.dataset.pinned).toBe('true')
    expect(probe.dataset.button).toBe('false')
  })

  it('ignores wheel down and keys that do not scroll up', () => {
    const { viewport, probe } = renderNearEnd()
    act(() => {
      viewport.dispatchEvent(new WheelEvent('wheel', { deltaY: 40, bubbles: true }))
      viewport.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }))
    })
    expect(probe.dataset.pinned).toBe('true')
  })
})

function touchEvent(type: string, clientY: number): Event {
  const event = new Event(type, { bubbles: true })
  Object.defineProperty(event, 'touches', { value: [{ clientY }] })
  return event
}

describe('MessageScrollerProvider with native scroll anchoring', () => {
  beforeEach(() => {
    vi.stubGlobal('CSS', { supports: (property: string) => property === 'overflow-anchor' })
  })
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  function renderPinned(endScroller: EndScroller): HTMLElement {
    const view = render(<Scroller followContent={false} endScroller={endScroller} />)
    const viewport = view.getByTestId('viewport')
    setGeometry(viewport)
    scrollViewportTo(viewport, 400)
    return viewport
  }

  it('leaves growth to the browser while the live edge is still in view', () => {
    const endScroller = vi.fn<EndScroller>()
    const viewport = renderPinned(endScroller)
    endScroller.mockClear()

    resize(viewport.firstElementChild as Element)
    expect(endScroller).not.toHaveBeenCalled()
  })

  it('catches up when growth outran the anchor (the browser does not anchor at scrollTop 0)', () => {
    const endScroller = vi.fn<EndScroller>()
    const viewport = renderPinned(endScroller)
    endScroller.mockClear()
    Object.defineProperty(viewport, 'scrollHeight', { configurable: true, value: 1200 })

    resize(viewport.firstElementChild as Element)
    expect(endScroller).toHaveBeenCalledWith('auto')
  })
})
