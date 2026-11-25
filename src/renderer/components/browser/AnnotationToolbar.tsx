import { useCallback, useEffect, useRef, useState } from 'react'
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
import { Button } from '@/components/ui/button'
import {
  MENU_MOTION_CLASS,
  MENU_SHORTCUT_CLASS,
  OVERLAY_PANEL_SURFACE_CLASS
} from '@/components/ui/menu-styles'
import {
  FOCUS_RING_CLASS,
  PANEL_ICON_BUTTON_CLASS,
  SEGMENTED_TRACK_CLASS,
  segmentClass
} from '@/components/ui/panel-styles'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { browserAnnotationsSync, browserTabAnnotationCmd } from '@/lib/browser-api'
import { formatBatchAnnotationsForChat } from '@/lib/browser-screenshot'
import { logFrontendError } from '@/lib/log-api'
import { isMac } from '@/lib/platform'
import { cn } from '@/lib/utils'
import { type AnnotateTool, useBrowserSessionStore } from '@/stores/browser-session-store'

interface AnnotationToolbarProps {
  browserTabId: string
  onExit: () => void
  onCaptureVisible: () => Promise<void>
  onSendToChat: (
    markdown: string,
    crops: Array<{ name: string; base64: string; dataUrl: string; path?: string }>
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
  onCaptureVisible,
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
  const copyTimerRef = useRef<number | null>(null)

  useEffect(() => {
    return () => {
      if (copyTimerRef.current !== null) {
        window.clearTimeout(copyTimerRef.current)
      }
    }
  }, [])

  const activeTool = tab?.annotateTool ?? 'comment'
  const paused = tab?.animationsPaused ?? false
  const pinsHidden = tab?.pinsHidden ?? false
  const pending = tab?.pendingAnnotations ?? []
  const count = pending.length

  const sendCmd = useCallback(
    (cmd: Parameters<typeof browserTabAnnotationCmd>[1]) => {
      void browserTabAnnotationCmd(browserTabId, cmd).then((res) => {
        if (!res.success && res.code !== 'NOT_TAURI') {
          void logFrontendError({
            level: 'warn',
            message: `browserTabAnnotationCmd(${cmd.type}) failed: ${res.error}`,
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

  const handleRemoveSingle = (annotationId: string) => {
    removePendingAnnotation(browserTabId, annotationId)
    sendCmd({ type: 'clear', ids: [annotationId] })
    void browserAnnotationsSync('delete', [annotationId])
  }

  const handleClearAll = () => {
    const ids = pending.map((item) => item.id)
    clearPendingAnnotations(browserTabId)
    sendCmd({ type: 'clear' })
    if (ids.length > 0) {
      void browserAnnotationsSync('delete', ids)
    }
    setQueueOpen(false)
  }

  const handleCopyBatch = () => {
    if (count === 0) return
    const text = formatBatchAnnotationsForChat(tab?.url ?? '', pending)
    if (copyTimerRef.current !== null) {
      window.clearTimeout(copyTimerRef.current)
    }
    setCopied(true)
    copyTimerRef.current = window.setTimeout(() => {
      setCopied(false)
      copyTimerRef.current = null
    }, 1200)
    void navigator.clipboard?.writeText(text).catch(() => {
      if (copyTimerRef.current !== null) {
        window.clearTimeout(copyTimerRef.current)
        copyTimerRef.current = null
      }
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
        dataUrl: item.cropDataUrl as string,
        path: item.cropPath
      }))
    const delivered = await onSendToChat(markdown, crops)
    if (!delivered) return
    removePendingAnnotations(browserTabId, submittedIds)
    void browserAnnotationsSync('resolve', submittedIds)
    const remaining =
      useBrowserSessionStore.getState().getTab(browserTabId)?.pendingAnnotations ?? []
    sendCmd(remaining.length === 0 ? { type: 'clear' } : { type: 'clear', ids: submittedIds })
    if (remaining.length === 0) setQueueOpen(false)
  }

  const areaHint = isMac ? '⇧⌘A' : 'Ctrl+Shift+A'
  const visibleHint = isMac ? '⇧⌘S' : 'Ctrl+Shift+S'

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
          className={cn(
            'inline-flex h-7 shrink-0 items-center gap-1 rounded-md bg-control-fill px-2 text-xs font-medium text-foreground shadow-control transition-colors duration-150 ease-out hover:bg-control-fill-hover',
            FOCUS_RING_CLASS
          )}
        >
          <X size={12} aria-hidden="true" />
          <span className="hidden sm:inline">Done</span>
        </button>

        <div className="h-[18px] w-[0.5px] shrink-0 bg-border" />

        <div
          role="tablist"
          aria-label="Annotation tools"
          className={cn(SEGMENTED_TRACK_CLASS, 'h-7 shrink-0 bg-card')}
        >
          {CORE_TOOLS.map((tool) => {
            const selected = activeTool === tool.id
            return (
              <button
                key={tool.id}
                type="button"
                role="tab"
                aria-selected={selected}
                onClick={() => selectTool(tool.id)}
                title={`${tool.label} (${tool.shortcut})`}
                className={cn(segmentClass(selected), 'h-[22px] gap-1 px-2')}
              >
                {tool.id === 'comment' ? (
                  <MessageSquare size={12} aria-hidden="true" />
                ) : tool.id === 'rectangle' ? (
                  <Square size={12} aria-hidden="true" />
                ) : (
                  <Ruler size={12} aria-hidden="true" />
                )}
                <span className="hidden md:inline">{tool.label}</span>
              </button>
            )
          })}
        </div>

        <div className="hidden h-[18px] w-[0.5px] shrink-0 bg-border lg:block" />

        <div className="hidden items-center gap-1 lg:flex">
          <button
            type="button"
            onClick={() => selectTool('screenshot-area')}
            aria-label="Screenshot area"
            title={`Screenshot area (${areaHint})`}
            className={cn(
              'inline-flex h-7 items-center gap-1.5 rounded-md px-2 text-xs font-medium transition-colors duration-150 ease-out',
              FOCUS_RING_CLASS,
              activeTool === 'screenshot-area'
                ? 'bg-control-fill text-foreground shadow-control'
                : 'text-muted-foreground hover:bg-foreground/[0.04] hover:text-foreground'
            )}
          >
            <Crop size={12} aria-hidden="true" />
            <span>Area</span>
            <span className={MENU_SHORTCUT_CLASS}>{areaHint}</span>
          </button>
          <button
            type="button"
            onClick={() => void onCaptureVisible()}
            aria-label="Screenshot visible viewport"
            title={`Screenshot visible (${visibleHint})`}
            className={cn(
              'inline-flex h-7 items-center gap-1.5 rounded-md px-2 text-xs font-medium text-muted-foreground transition-colors duration-150 ease-out hover:bg-foreground/[0.04] hover:text-foreground',
              FOCUS_RING_CLASS
            )}
          >
            <Camera size={12} aria-hidden="true" />
            <span>Visible</span>
          </button>
        </div>
      </div>

      <div className="flex shrink-0 items-center gap-1">
        {paused ? (
          <button
            type="button"
            onClick={handleTogglePause}
            aria-label="Resume page animations"
            title="Resume animations (P)"
            className={cn(
              'inline-flex h-7 items-center gap-1.5 rounded-full bg-warning/15 px-2.5 text-xs font-medium text-warning transition-colors duration-150 ease-out hover:bg-warning/20',
              FOCUS_RING_CLASS
            )}
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
            className={PANEL_ICON_BUTTON_CLASS}
          >
            <Pause size={13} />
          </button>
        )}

        <button
          type="button"
          onClick={handleTogglePins}
          aria-label={pinsHidden ? 'Show annotation pins' : 'Hide annotation pins'}
          title={pinsHidden ? 'Show pins (H)' : 'Hide pins (H)'}
          className={PANEL_ICON_BUTTON_CLASS}
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
          className={cn(
            PANEL_ICON_BUTTON_CLASS,
            'disabled:pointer-events-none disabled:opacity-40'
          )}
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
                className={cn(
                  'inline-flex h-7 items-center gap-1 rounded-md bg-control-fill px-2 font-mono text-xs font-medium text-foreground shadow-control transition-colors duration-150 ease-out hover:bg-control-fill-hover',
                  FOCUS_RING_CLASS
                )}
              >
                <span>{count}</span>
                <ChevronDown size={11} className="text-muted-foreground" />
              </button>
            </PopoverTrigger>
            <PopoverContent
              align="end"
              sideOffset={6}
              className={cn(
                OVERLAY_PANEL_SURFACE_CLASS,
                MENU_MOTION_CLASS,
                'w-96 overflow-hidden border-0 p-0'
              )}
            >
              <div className="flex items-center justify-between border-b border-border/80 px-3.5 py-2.5">
                <div className="flex items-center gap-2">
                  <span className="text-sm font-medium text-foreground">Batch comments</span>
                  <span className="rounded-full bg-primary/15 px-2 py-0.5 font-mono text-xs font-medium text-primary">
                    {count} pending
                  </span>
                </div>
                <button
                  type="button"
                  onClick={handleClearAll}
                  className={cn(
                    'rounded text-xs font-medium text-muted-foreground hover:text-foreground',
                    FOCUS_RING_CLASS
                  )}
                >
                  Clear all
                </button>
              </div>
              <div className="max-h-72 divide-y divide-border/70 overflow-y-auto">
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
                      <p className="text-xs font-medium leading-5 text-foreground">
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
                      onClick={() => handleRemoveSingle(item.id)}
                      className={cn(
                        'flex size-6 shrink-0 items-center justify-center rounded text-muted-foreground transition-colors duration-150 ease-out hover:bg-foreground/[0.06] hover:text-destructive',
                        FOCUS_RING_CLASS
                      )}
                    >
                      <Trash2 size={12} />
                    </button>
                  </div>
                ))}
              </div>
              <div className="flex items-center justify-between border-t border-border/80 bg-card px-3.5 py-2.5">
                <span className="text-xs text-muted-foreground">
                  Includes DOM selectors + saved crops
                </span>
                <div className="flex items-center gap-2">
                  <Button type="button" variant="outline" size="sm" onClick={handleCopyBatch}>
                    Copy
                  </Button>
                  <Button
                    type="button"
                    variant="default"
                    size="sm"
                    onClick={() => void handleSendBatch()}
                  >
                    Send to chat ({count})
                  </Button>
                </div>
              </div>
            </PopoverContent>
          </Popover>
        )}

        <Button
          type="button"
          variant={count > 0 && !queueOpen ? 'default' : 'outline'}
          size="sm"
          disabled={count === 0}
          onClick={() => void handleSendBatch()}
          className="h-7 gap-1.5 px-2.5 text-xs"
        >
          <span>Send to chat</span>
          {count > 0 && (
            <span className="inline-flex min-w-[18px] items-center justify-center rounded-full bg-current/15 px-1 font-mono text-xs tabular-nums">
              {count}
            </span>
          )}
        </Button>
      </div>
    </div>
  )
}
