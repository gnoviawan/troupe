import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react'
import { toast } from 'sonner'
import {
  Camera,
  Check,
  ChevronDown,
  Copy,
  Crop,
  Eye,
  EyeOff,
  MessageSquare,
  Pause,
  Ruler,
  Square,
  Trash2,
  X
} from '@/components/icons'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { browserTabAnnotationCmd, browserTabScreenshot } from '@/lib/browser-api'
import { formatBatchAnnotationsForChat } from '@/lib/browser-screenshot'
import { logFrontendError } from '@/lib/log-api'
import { cn } from '@/lib/utils'
import { type AnnotateTool, useBrowserSessionStore } from '@/stores/browser-session-store'

interface AnnotationToolbarProps {
  browserTabId: string
  onExit: () => void
  onSendToChat: (
    markdown: string,
    crops: Array<{ name: string; base64: string; dataUrl: string }>
  ) => Promise<boolean> | boolean
}

const CORE_TOOLS: ReadonlyArray<{
  id: Exclude<AnnotateTool, 'screenshot-area'>
  label: string
  shortcut: string
}> = [
  { id: 'comment', label: 'Comment', shortcut: 'C' },
  { id: 'rectangle', label: 'Rectangle', shortcut: 'R' },
  { id: 'ruler', label: 'Ruler', shortcut: 'M' }
]

export function AnnotationToolbar({
  browserTabId,
  onExit,
  onSendToChat
}: AnnotationToolbarProps): React.JSX.Element {
  const tab = useBrowserSessionStore((s) => s.tabs.get(browserTabId))
  const setAnnotateTool = useBrowserSessionStore((s) => s.setAnnotateTool)
  const toggleAnimationsPaused = useBrowserSessionStore((s) => s.toggleAnimationsPaused)
  const togglePinsHidden = useBrowserSessionStore((s) => s.togglePinsHidden)
  const removePendingAnnotation = useBrowserSessionStore((s) => s.removePendingAnnotation)
  const removePendingAnnotations = useBrowserSessionStore((s) => s.removePendingAnnotations)
  const clearPendingAnnotations = useBrowserSessionStore((s) => s.clearPendingAnnotations)

  const [copied, setCopied] = useState(false)
  const [queueOpen, setQueueOpen] = useState(false)
  const pillRef = useRef<HTMLSpanElement>(null)
  const toolRefs = useRef<Record<string, HTMLButtonElement | null>>({})
  const mountedRef = useRef(false)

  const activeTool = tab?.annotateTool ?? 'comment'
  const paused = tab?.animationsPaused ?? false
  const pinsHidden = tab?.pinsHidden ?? false
  const pending = tab?.pendingAnnotations ?? []
  const count = pending.length

  const syncPill = useCallback(
    (animate: boolean) => {
      const pill = pillRef.current
      const el = toolRefs.current[activeTool]
      if (!pill || !el) return
      if (!animate) {
        const prev = pill.style.transition
        pill.style.transition = 'none'
        pill.style.transform = `translateX(${el.offsetLeft}px)`
        pill.style.width = `${el.offsetWidth}px`
        void pill.offsetWidth
        pill.style.transition = prev
      } else {
        pill.style.transform = `translateX(${el.offsetLeft}px)`
        pill.style.width = `${el.offsetWidth}px`
      }
    },
    [activeTool]
  )

  useLayoutEffect(() => {
    syncPill(mountedRef.current)
    mountedRef.current = true
  }, [syncPill])

  useEffect(() => {
    const onResize = () => syncPill(false)
    window.addEventListener('resize', onResize)
    return () => window.removeEventListener('resize', onResize)
  }, [syncPill])

  const sendCmd = useCallback(
    (cmd: Parameters<typeof browserTabAnnotationCmd>[1]) => {
      void browserTabAnnotationCmd(browserTabId, cmd).then((res) => {
        if (!res.success && res.code !== 'NOT_TAURI') {
          void logFrontendError({
            level: 'warn',
            message: `browserTabAnnotationCmd failed: ${res.error}`,
            source: 'AnnotationToolbar'
          })
        }
      })
    },
    [browserTabId]
  )

  const selectTool = (tool: AnnotateTool) => {
    setAnnotateTool(browserTabId, tool)
    sendCmd({ type: 'set_tool', tool })
  }

  const handleTogglePause = () => {
    const next = toggleAnimationsPaused(browserTabId)
    sendCmd({ type: 'set_pause', paused: next })
  }

  const handleTogglePins = () => {
    const next = togglePinsHidden(browserTabId)
    sendCmd({ type: 'set_pins_hidden', hidden: next })
  }

  const handleClearAll = () => {
    clearPendingAnnotations(browserTabId)
    sendCmd({ type: 'clear' })
    setQueueOpen(false)
  }

  const handleVisibleScreenshot = async () => {
    const res = await browserTabScreenshot(browserTabId)
    if (!res.success) {
      toast.error(res.error || 'Could not capture screenshot.')
      return
    }
    const dataUrl = `data:${res.data.mimeType};base64,${res.data.base64}`
    onSendToChat(`Viewport screenshot of ${tab?.url || 'current page'}`, [
      { name: 'browser-visible.png', base64: res.data.base64, dataUrl }
    ])
  }

  const handleCopyBatch = () => {
    if (count === 0) return
    const text = formatBatchAnnotationsForChat(tab?.url ?? '', pending)
    setCopied(true)
    const timer = window.setTimeout(() => setCopied(false), 1200)
    void navigator.clipboard?.writeText(text).catch(() => {
      window.clearTimeout(timer)
      setCopied(false)
    })
  }

  const handleSendBatch = async () => {
    if (count === 0) return
    const submitted = [...pending]
    const submittedIds = submitted.map((item) => item.id)
    const markdown = formatBatchAnnotationsForChat(tab?.url ?? '', submitted)
    const crops = submitted
      .filter((item) => item.cropBase64 && item.cropDataUrl)
      .map((item) => ({
        name: `annotation-${item.num}.png`,
        base64: item.cropBase64 as string,
        dataUrl: item.cropDataUrl as string
      }))
    const delivered = await onSendToChat(markdown, crops)
    if (!delivered) return
    removePendingAnnotations(browserTabId, submittedIds)
    const remaining =
      useBrowserSessionStore.getState().getTab(browserTabId)?.pendingAnnotations ?? []
    sendCmd(remaining.length === 0 ? { type: 'clear' } : { type: 'clear', ids: submittedIds })
    if (remaining.length === 0) setQueueOpen(false)
  }

  return (
    <div
      data-testid="browser-annotation-toolbar"
      className="flex h-10 w-full items-center justify-between gap-2 border-b border-border/80 bg-background px-2"
    >
      <div className="flex min-w-0 items-center gap-2">
        <button
          type="button"
          onClick={onExit}
          aria-label="Exit annotate mode"
          className="tactile-control inline-flex h-7 shrink-0 items-center gap-1 rounded-md bg-muted px-2 text-xs font-medium text-foreground transition-[transform,background-color] duration-150 hover:bg-secondary active:scale-[0.97]"
        >
          <X size={12} aria-hidden="true" />
          <span className="hidden sm:inline">Done</span>
        </button>

        <div className="h-[18px] w-[0.5px] shrink-0 bg-border" />

        <div
          role="tablist"
          aria-label="Annotation tools"
          className="t-tabs h-8 gap-0.5 rounded-[7px] bg-card p-0.5"
        >
          <span
            ref={pillRef}
            aria-hidden="true"
            className="t-tabs-pill tactile-control top-0.5 h-7 rounded-md bg-muted"
          />
          {CORE_TOOLS.map((tool) => {
            const selected = activeTool === tool.id
            return (
              <button
                key={tool.id}
                ref={(el) => {
                  toolRefs.current[tool.id] = el
                }}
                type="button"
                role="tab"
                aria-selected={selected}
                title={`${tool.label} (${tool.shortcut})`}
                onClick={() => selectTool(tool.id)}
                className={cn(
                  't-tab inline-flex h-7 items-center gap-1.5 rounded-md px-2 text-xs font-medium',
                  selected ? 'text-foreground' : 'text-muted-foreground hover:text-foreground'
                )}
              >
                {tool.id === 'comment' ? (
                  <MessageSquare size={13} aria-hidden="true" />
                ) : tool.id === 'rectangle' ? (
                  <Square size={13} aria-hidden="true" />
                ) : (
                  <Ruler size={13} aria-hidden="true" />
                )}
                <span className="hidden md:inline">{tool.label}</span>
              </button>
            )
          })}
        </div>

        <div className="h-[18px] w-[0.5px] shrink-0 bg-border" />

        <div className="flex items-center gap-0.5">
          <button
            type="button"
            title="Screenshot area (⇧⌘A)"
            aria-label="Screenshot area"
            onClick={() => selectTool('screenshot-area')}
            className={cn(
              'inline-flex h-7 items-center gap-1 rounded-md px-2 text-xs font-medium transition-colors duration-150',
              activeTool === 'screenshot-area'
                ? 'tactile-control bg-muted text-foreground'
                : 'text-secondary-foreground hover:bg-muted/60 hover:text-foreground'
            )}
          >
            <Crop size={13} aria-hidden="true" />
            <span className="hidden lg:inline">Area</span>
          </button>
          <button
            type="button"
            title="Screenshot visible viewport (⇧⌘S)"
            aria-label="Screenshot visible"
            onClick={() => void handleVisibleScreenshot()}
            className="inline-flex h-7 items-center gap-1 rounded-md px-2 text-xs font-medium text-secondary-foreground transition-colors duration-150 hover:bg-muted/60 hover:text-foreground active:scale-[0.97]"
          >
            <Camera size={13} aria-hidden="true" />
            <span className="hidden lg:inline">Visible</span>
          </button>
        </div>
      </div>

      <div className="flex shrink-0 items-center gap-1.5">
        {paused ? (
          <button
            type="button"
            onClick={handleTogglePause}
            aria-label="Resume page animations"
            className="inline-flex h-6 items-center gap-1.5 rounded-full bg-warning/15 px-2.5 text-xs font-medium text-warning transition-colors duration-150 hover:bg-warning/20"
          >
            <span className="size-1.5 rounded-full bg-warning" />
            <span>Paused</span>
          </button>
        ) : (
          <button
            type="button"
            onClick={handleTogglePause}
            aria-label="Pause page animations"
            title="Pause animations (P)"
            className="flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors duration-150 hover:bg-muted/60 hover:text-foreground"
          >
            <Pause size={13} />
          </button>
        )}

        <button
          type="button"
          onClick={handleTogglePins}
          aria-label={pinsHidden ? 'Show annotation pins' : 'Hide annotation pins'}
          title={pinsHidden ? 'Show pins (H)' : 'Hide pins (H)'}
          className="flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors duration-150 hover:bg-muted/60 hover:text-foreground"
        >
          <span className="t-icon-swap" data-state={pinsHidden ? 'b' : 'a'}>
            <span className="t-icon" data-icon="a">
              <Eye size={14} />
            </span>
            <span className="t-icon" data-icon="b">
              <EyeOff size={14} />
            </span>
          </span>
        </button>

        <button
          type="button"
          disabled={count === 0}
          onClick={handleCopyBatch}
          aria-label="Copy batch annotations"
          title="Copy feedback"
          className="flex size-7 items-center justify-center rounded-md text-secondary-foreground transition-colors duration-150 hover:bg-muted/60 hover:text-foreground disabled:opacity-40"
        >
          <span className="t-icon-swap" data-state={copied ? 'b' : 'a'}>
            <span className="t-icon" data-icon="a">
              <Copy size={13} />
            </span>
            <span className="t-icon" data-icon="b">
              <Check size={13} className="text-success" />
            </span>
          </span>
        </button>

        {count > 0 && (
          <Popover open={queueOpen} onOpenChange={setQueueOpen}>
            <PopoverTrigger asChild>
              <button
                type="button"
                aria-label={`View ${count} pending comments`}
                className="tactile-control inline-flex h-7 items-center gap-1 rounded-md bg-muted px-2 font-mono text-xs font-medium text-foreground transition-colors duration-150 hover:bg-secondary"
              >
                <span>{count}</span>
                <ChevronDown size={11} className="text-muted-foreground" />
              </button>
            </PopoverTrigger>
            <PopoverContent
              align="end"
              sideOffset={6}
              className="t-dropdown tactile-menu w-96 overflow-hidden rounded-lg border-0 bg-popover p-0"
            >
              <div className="flex items-center justify-between border-b border-border/80 px-3.5 py-2.5">
                <div className="flex items-center gap-2">
                  <span className="text-[13px] font-medium text-foreground">Batch comments</span>
                  <span className="rounded-full bg-primary/15 px-2 py-0.5 font-mono text-xs font-medium text-primary">
                    {count} pending
                  </span>
                </div>
                <button
                  type="button"
                  onClick={handleClearAll}
                  className="text-xs font-medium text-muted-foreground hover:text-foreground"
                >
                  Clear all
                </button>
              </div>
              <div className="max-h-72 overflow-y-auto divide-y divide-border/70">
                {pending.map((item) => (
                  <div key={item.id} className="flex items-start gap-2.5 px-3.5 py-2.5">
                    <span
                      className={cn(
                        'mt-0.5 flex size-5 shrink-0 items-center justify-center rounded-full font-mono text-xs font-medium text-primary-foreground',
                        item.kind === 'guide' ? 'bg-destructive' : 'bg-primary-fill'
                      )}
                    >
                      {item.num}
                    </span>
                    <div className="min-w-0 flex-1">
                      <p className="text-[13px] font-medium leading-5 text-foreground">
                        {item.comment}
                      </p>
                      <p className="truncate font-mono text-xs text-muted-foreground">
                        {item.elementPath || item.element}
                        {item.meta ? ` · ${item.meta}` : ''}
                      </p>
                    </div>
                    {item.cropDataUrl && (
                      <img
                        src={item.cropDataUrl}
                        alt=""
                        className="h-8 w-12 shrink-0 rounded object-cover ring-1 ring-border"
                      />
                    )}
                    <button
                      type="button"
                      aria-label={`Remove comment ${item.num}`}
                      onClick={() => removePendingAnnotation(browserTabId, item.id)}
                      className="flex size-6 shrink-0 items-center justify-center rounded text-muted-foreground hover:bg-muted hover:text-destructive"
                    >
                      <Trash2 size={12} />
                    </button>
                  </div>
                ))}
              </div>
              <div className="flex items-center justify-between border-t border-border/80 bg-card px-3.5 py-2.5">
                <span className="text-xs text-muted-foreground">
                  Includes DOM selectors + auto-crops
                </span>
                <div className="flex items-center gap-2">
                  <button
                    type="button"
                    onClick={handleCopyBatch}
                    className="tactile-control h-7 rounded-md bg-muted px-2.5 text-xs font-medium text-foreground hover:bg-secondary"
                  >
                    Copy
                  </button>
                  <button
                    type="button"
                    onClick={handleSendBatch}
                    className="h-7 rounded-md bg-primary-fill px-2.5 text-xs font-medium text-primary-foreground hover:bg-primary-fill-hover"
                  >
                    Send to chat ({count})
                  </button>
                </div>
              </div>
            </PopoverContent>
          </Popover>
        )}

        <button
          type="button"
          disabled={count === 0}
          onClick={handleSendBatch}
          className={cn(
            'inline-flex h-7 items-center gap-1.5 rounded-md px-2.5 text-xs font-medium tracking-[-0.01em] transition-[transform,background-color] duration-150',
            count > 0
              ? 'tactile-primary bg-primary-fill text-primary-foreground hover:bg-primary-fill-hover active:scale-[0.97]'
              : 'bg-muted text-disabled-foreground'
          )}
        >
          <span>Send to chat</span>
          {count > 0 && (
            <span className="inline-flex min-w-[18px] items-center justify-center rounded-full bg-primary-foreground/20 px-1 font-mono text-xs tabular-nums">
              {count}
            </span>
          )}
        </button>
      </div>
    </div>
  )
}
