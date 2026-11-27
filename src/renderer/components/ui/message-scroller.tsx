import { AnimatePresence, motion, useReducedMotion } from 'framer-motion'
import * as React from 'react'
import { CHAT_SPRING } from '@/components/chat/chat-motion'
import { ArrowDown } from '@/components/icons'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'

/**
 * Native (React 18-safe) message scroller. Mirrors the shadcn MessageScroller
 * component API without the `@shadcn/react` headless engine (which requires
 * React 19). Behavior: auto-follow the live edge only while the reader is
 * pinned to the bottom; surface a jump-to-latest control otherwise.
 */

export const BOTTOM_THRESHOLD_PX = 48

/**
 * Whether the browser follows a growing list by itself through CSS scroll
 * anchoring (`overflow-anchor`). WebView2 and Chromium do; WKWebView only from
 * WebKit 27, so older macOS webviews keep the JS follow.
 */
export function supportsScrollAnchoring(): boolean {
  return typeof CSS !== 'undefined' && CSS.supports?.('overflow-anchor', 'auto') === true
}

/** Scrolls the list to its live edge. A virtualizer registers one to own the end. */
export type EndScroller = (behavior: ScrollBehavior) => void

interface MessageScrollerContextValue {
  registerViewport: (el: HTMLDivElement | null) => void
  /** The current viewport scroll element (for the virtualizer's `getScrollElement`). */
  viewportEl: HTMLDivElement | null
  pinned: boolean
  /** Whether the list follows its live edge at all; off also drops the scroll anchor. */
  autoScroll: boolean
  /** The browser's scroll anchoring follows content growth; lists must not write scrollTop for it. */
  anchoring: boolean
  showButton: boolean
  /** Number of items added while the reader was scrolled away from the live edge. */
  newCount: number
  /** Report the current item count so unread can be derived. */
  setItemCount: (n: number) => void
  scrollToEnd: (behavior?: ScrollBehavior) => void
  /**
   * Hands "scroll to the end" to the list (null gives it back). A virtualized
   * list must own it: it knows the real end before every row is measured.
   */
  setEndScroller: (scroller: EndScroller | null) => void
}

const MessageScrollerContext = React.createContext<MessageScrollerContextValue | null>(null)

function useMessageScroller(): MessageScrollerContextValue {
  const ctx = React.useContext(MessageScrollerContext)
  if (!ctx) {
    throw new Error('useMessageScroller must be used within <MessageScrollerProvider>')
  }
  return ctx
}

function MessageScrollerProvider({
  autoScroll = true,
  followContent = true,
  children
}: {
  autoScroll?: boolean
  /**
   * Follow the live edge when the content grows. Turn it off when a
   * virtualizer pins the end itself (`anchorTo: 'end'`): two scroll writers
   * that react to the same resize move the viewport against each other.
   * Viewport resizes (panel resize, on-screen keyboard) still follow. With
   * native scroll anchoring the provider only catches up when the browser did
   * not follow (it does not anchor at `scrollTop` 0).
   */
  followContent?: boolean
  children: React.ReactNode
}): React.JSX.Element {
  const [viewportEl, setViewportEl] = React.useState<HTMLDivElement | null>(null)
  const [pinned, setPinned] = React.useState(true)
  const [anchoring] = React.useState(supportsScrollAnchoring)
  const [showButton, setShowButton] = React.useState(false)
  const [itemCount, setItemCount] = React.useState(0)
  const [seenCount, setSeenCount] = React.useState(0)
  const pinnedRef = React.useRef(pinned)
  pinnedRef.current = pinned
  const endScrollerRef = React.useRef<EndScroller | null>(null)
  /** The reader moved away from the live edge on purpose; stays until they come back. */
  const releasedRef = React.useRef(false)
  const setEndScroller = React.useCallback((scroller: EndScroller | null) => {
    endScrollerRef.current = scroller
  }, [])

  // While pinned to the live edge, everything is "seen". Unread is whatever
  // arrived since the reader last sat at the bottom.
  React.useEffect(() => {
    if (pinned) setSeenCount(itemCount)
  }, [pinned, itemCount])
  const newCount = pinned ? 0 : Math.max(0, itemCount - seenCount)

  const scrollToEnd = React.useCallback(
    (behavior: ScrollBehavior = 'smooth') => {
      if (!viewportEl) return
      if (endScrollerRef.current) endScrollerRef.current(behavior)
      else viewportEl.scrollTo({ top: viewportEl.scrollHeight, behavior })
      releasedRef.current = false
      setPinned(true)
      setShowButton(false)
    },
    [viewportEl]
  )

  React.useEffect(() => {
    if (!viewportEl) return
    let last = {
      top: viewportEl.scrollTop,
      height: viewportEl.scrollHeight,
      client: viewportEl.clientHeight
    }
    const update = (): void => {
      const { scrollTop, scrollHeight, clientHeight } = viewportEl
      const distance = scrollHeight - scrollTop - clientHeight
      // A scroll up that no layout change explains is the reader's own
      // (a scrollbar drag); every scroll writer here only moves down. A bounce
      // past the end (macOS rubber-banding) settling back is not.
      const settlingFromOverscroll = last.top > last.height - last.client
      if (
        scrollTop < last.top &&
        !settlingFromOverscroll &&
        scrollHeight >= last.height &&
        clientHeight === last.client
      ) {
        releasedRef.current = true
      } else if (releasedRef.current && scrollTop > last.top && distance <= BOTTOM_THRESHOLD_PX) {
        releasedRef.current = false
      }
      last = { top: scrollTop, height: scrollHeight, client: clientHeight }
      const isPinned = !releasedRef.current && distance <= BOTTOM_THRESHOLD_PX
      const overflowing = scrollHeight - clientHeight > BOTTOM_THRESHOLD_PX
      setPinned(isPinned)
      setShowButton(overflowing && !isPinned)
    }

    const onScroll = (): void => update()
    viewportEl.addEventListener('scroll', onScroll, { passive: true })

    // Let go of the live edge on the reader's first move up, not once they
    // are already past the threshold: a follow write in between pulls them back.
    const release = (): void => {
      if (viewportEl.scrollTop <= 0) return
      releasedRef.current = true
      update()
    }
    const onWheel = (event: WheelEvent): void => {
      if (event.deltaY < 0) release()
    }
    const onKeyDown = (event: KeyboardEvent): void => {
      if (event.key !== 'PageUp' && event.key !== 'ArrowUp' && event.key !== 'Home') return
      const target = event.target
      if (target instanceof Element && target.closest('input, textarea, select, [contenteditable]'))
        return
      release()
    }
    let touchY: number | null = null
    const onTouchStart = (event: TouchEvent): void => {
      touchY = event.touches[0]?.clientY ?? null
    }
    const onTouchMove = (event: TouchEvent): void => {
      const y = event.touches[0]?.clientY
      if (touchY !== null && y !== undefined && y > touchY) release()
      touchY = y ?? null
    }
    viewportEl.addEventListener('wheel', onWheel, { passive: true })
    viewportEl.addEventListener('keydown', onKeyDown)
    viewportEl.addEventListener('touchstart', onTouchStart, { passive: true })
    viewportEl.addEventListener('touchmove', onTouchMove, { passive: true })

    const follow = (): void => {
      if (autoScroll && pinnedRef.current && !releasedRef.current) {
        if (endScrollerRef.current) endScrollerRef.current('auto')
        else viewportEl.scrollTop = viewportEl.scrollHeight
      }
      update()
    }

    const catchUp = (): void => {
      const { scrollTop, scrollHeight, clientHeight } = viewportEl
      if (scrollHeight - scrollTop - clientHeight > 1) follow()
      else update()
    }

    const ro = new ResizeObserver(follow)
    ro.observe(viewportEl)
    const content = viewportEl.firstElementChild
    // Without content following, a content resize only refreshes the pinned state.
    const contentRo = new ResizeObserver(followContent ? follow : anchoring ? catchUp : update)
    if (content) contentRo.observe(content)

    update()

    return () => {
      viewportEl.removeEventListener('scroll', onScroll)
      viewportEl.removeEventListener('wheel', onWheel)
      viewportEl.removeEventListener('keydown', onKeyDown)
      viewportEl.removeEventListener('touchstart', onTouchStart)
      viewportEl.removeEventListener('touchmove', onTouchMove)
      ro.disconnect()
      contentRo.disconnect()
    }
  }, [viewportEl, autoScroll, followContent, anchoring])

  const value = React.useMemo<MessageScrollerContextValue>(
    () => ({
      registerViewport: setViewportEl,
      viewportEl,
      pinned,
      autoScroll,
      anchoring,
      showButton,
      newCount,
      setItemCount,
      scrollToEnd,
      setEndScroller
    }),
    [viewportEl, pinned, autoScroll, anchoring, showButton, newCount, scrollToEnd, setEndScroller]
  )

  return <MessageScrollerContext.Provider value={value}>{children}</MessageScrollerContext.Provider>
}

function MessageScroller({ className, ...props }: React.ComponentProps<'div'>): React.JSX.Element {
  return (
    <div
      data-slot="message-scroller"
      className={cn(
        'group/message-scroller relative flex size-full min-h-0 flex-col overflow-hidden',
        className
      )}
      {...props}
    />
  )
}

/**
 * Scrolls vertically only. `overflow-x-hidden` stops a row's enter transform
 * (the user bubble drifts in from x: 12) from flashing a horizontal scrollbar;
 * wide content keeps its own `overflow-x-auto` region (tables, code, media).
 */
function MessageScrollerViewport({
  className,
  children,
  ...props
}: React.ComponentProps<'div'>): React.JSX.Element {
  const { registerViewport, autoScroll } = useMessageScroller()
  return (
    <div
      ref={registerViewport}
      data-slot="message-scroller-viewport"
      className={cn(
        'size-full min-h-0 min-w-0 overflow-y-auto overflow-x-hidden overscroll-contain [scrollbar-gutter:stable_both-edges] *:[overflow-anchor:none]',
        className
      )}
      {...props}
    >
      {children}
      {autoScroll && <MessageScrollerAnchor />}
    </div>
  )
}

/**
 * CSS scroll anchoring target. Every other child opts out, so the browser can
 * anchor only to this last pixel: while it is on screen (the reader is at the
 * live edge) content growth above it keeps it in place, which follows the
 * stream; once the reader scrolls it off screen nothing is anchored and the
 * reading position stays put. `-mt-px` overlaps the content's last pixel so
 * `scrollHeight` matches the virtualizer's end.
 */
function MessageScrollerAnchor(): React.JSX.Element {
  return (
    <div
      aria-hidden="true"
      data-slot="message-scroller-anchor"
      className="pointer-events-none -mt-px h-px [overflow-anchor:auto]!"
    />
  )
}

function MessageScrollerContent({
  className,
  live = true,
  ...props
}: React.ComponentProps<'div'> & {
  /**
   * Whether the log announces additions. The default keeps the polite log.
   * `false` renders `aria-live="off"` with no `aria-relevant`: `role="log"`
   * implies polite, so omitting the attribute would not silence it. The mobile
   * shell turns it off because its own live region announces turn events, and
   * a virtualised list re-announces history as rows mount while scrolling.
   */
  live?: boolean
}): React.JSX.Element {
  return (
    <div
      data-slot="message-scroller-content"
      role="log"
      {...(live ? { 'aria-relevant': 'additions', 'aria-live': 'polite' } : { 'aria-live': 'off' })}
      className={cn('flex min-h-full flex-col', className)}
      {...props}
    />
  )
}

/**
 * Single timeline row. Forwards its ref so the TanStack Virtual virtualizer
 * can attach `measureElement` for dynamic row-height measurement.
 */
const MessageScrollerItem = React.forwardRef<
  HTMLDivElement,
  React.ComponentProps<'div'> & {
    scrollAnchor?: boolean
    messageId?: string
  }
>(function MessageScrollerItem({ className, scrollAnchor = false, messageId, ...props }, ref) {
  return (
    <div
      ref={ref}
      data-slot="message-scroller-item"
      data-scroll-anchor={scrollAnchor || undefined}
      data-message-id={messageId}
      className={cn('min-w-0 shrink-0', className)}
      {...props}
    />
  )
})

function MessageScrollerButton({
  className,
  ...props
}: Omit<React.ComponentProps<typeof Button>, 'children'>): React.JSX.Element {
  const { showButton, newCount, scrollToEnd } = useMessageScroller()
  const reduced = useReducedMotion() ?? false
  const hasNew = newCount > 0
  return (
    <div className="pointer-events-none absolute bottom-4 left-1/2 z-20 -translate-x-1/2">
      <AnimatePresence>
        {showButton && (
          <motion.div
            key="jump-to-latest"
            className="pointer-events-auto relative"
            initial={reduced ? { opacity: 0 } : { opacity: 0, scale: 0.96, y: 8 }}
            animate={reduced ? { opacity: 1 } : { opacity: 1, scale: 1, y: 0 }}
            exit={reduced ? { opacity: 0 } : { opacity: 0, y: 8 }}
            transition={CHAT_SPRING}
          >
            {hasNew && (
              <span className="pointer-events-none absolute inset-0 animate-ping rounded-full bg-foreground/30 motion-reduce:animate-none" />
            )}
            {hasNew && (
              <span className="pointer-events-none absolute -top-1.5 left-1/2 z-10 -translate-x-1/2 rounded-full bg-foreground px-1.5 text-3xs font-semibold tabular-nums text-background shadow-sm">
                {newCount > 99 ? '99+' : newCount}
              </span>
            )}
            <Button
              data-slot="message-scroller-button"
              type="button"
              variant="secondary"
              size="icon-sm"
              onClick={() => scrollToEnd('smooth')}
              className={cn(
                'relative size-11 rounded-full border border-border bg-background text-foreground shadow-md hover:bg-muted active:bg-muted @[400px]:pointer-fine:size-10',
                hasNew && 'border-foreground/40',
                className
              )}
              {...props}
            >
              <ArrowDown />
              <span className="sr-only">
                {hasNew ? `Scroll to latest (${newCount} new)` : 'Scroll to latest'}
              </span>
            </Button>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  )
}

export {
  MessageScroller,
  MessageScrollerButton,
  MessageScrollerContent,
  MessageScrollerItem,
  MessageScrollerProvider,
  MessageScrollerViewport,
  useMessageScroller
}
