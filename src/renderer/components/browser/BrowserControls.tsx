import { AnimatePresence, MotionConfig, motion } from 'framer-motion'
import { useCallback, useEffect, useRef, useState } from 'react'
import { toast } from 'sonner'
import {
  ArrowLeft,
  ArrowRight,
  Camera,
  Check,
  ChevronDown,
  Copy,
  Crop,
  Globe,
  Lock,
  Monitor,
  PenTool,
  RefreshCw,
  Smartphone,
  Square,
  Tablet,
  X
} from '@/components/icons'
import { Button } from '@/components/ui/button'
import {
  MENU_CONTENT_CLASS,
  MENU_ITEM_CLASS,
  MENU_MOTION_CLASS,
  MENU_SHORTCUT_CLASS
} from '@/components/ui/menu-styles'
import {
  FOCUS_RING_CLASS,
  INNER_FIELD_FOCUS_CLASS,
  PANEL_ICON_BUTTON_CLASS
} from '@/components/ui/panel-styles'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { Spinner } from '@/components/ui/spinner'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import {
  isNativeWebviewOccluded,
  useInterfaceOverlayOpen
} from '@/hooks/use-native-webview-occlusion'
import {
  browserTabAnnotationCmd,
  browserTabGoBack,
  browserTabGoForward,
  browserTabInjectAgentation,
  browserTabOpenDevtools,
  browserTabReload,
  browserTabScreenshot,
  browserTabStop
} from '@/lib/browser-api'
import { stopBrowserAgent } from '@/lib/browser-consent-api'
import { copyBase64PngToClipboard } from '@/lib/browser-screenshot'
import { normalizeBrowserInputUrl, splitDisplayUrl } from '@/lib/browser-url'
import { logFrontendError } from '@/lib/log-api'
import { isMac } from '@/lib/platform'
import { openerApi } from '@/lib/tauri-opener-api'
import { cn } from '@/lib/utils'
import { useAgentTabSession, useSessionBrowserStopped } from '@/stores/browser-control-store'
import { type AnnotateTool, useBrowserSessionStore } from '@/stores/browser-session-store'
import {
  findPaneById,
  getAllLeafPanes,
  useWorkspaceStore,
  browserTabId as workspaceBrowserTabId
} from '@/stores/workspace-store'
import { AnnotationToolbar } from './AnnotationToolbar'
import { BrowserMoreMenu } from './BrowserMoreMenu'
import { BrowserZoomControl } from './BrowserZoomControl'

interface BrowserControlsProps {
  browserTabId: string
  isVisible?: boolean
  onSendAnnotationsToChat?: (
    markdown: string,
    crops: Array<{ name: string; base64: string; dataUrl: string; path?: string }>
  ) => Promise<boolean> | boolean
}

function logControlError(action: string, error: unknown): void {
  void logFrontendError({
    level: 'warn',
    message: `${action} failed: ${error instanceof Error ? error.message : String(error)}`,
    source: 'BrowserControls'
  })
}

/**
 * Wait briefly for an open toolbar popover occluder to unmount and the native
 * child webview to reveal again before capturing a native viewport screenshot.
 */
async function waitForOcclusionClear(): Promise<void> {
  if (!isNativeWebviewOccluded()) return
  const deadline = performance.now() + 240
  while (isNativeWebviewOccluded() && performance.now() < deadline) {
    await new Promise<void>((resolve) => {
      requestAnimationFrame(() => resolve())
    })
  }
  await new Promise<void>((resolve) => {
    requestAnimationFrame(() => resolve())
  })
}

export function BrowserControls({
  browserTabId,
  isVisible = true,
  onSendAnnotationsToChat
}: BrowserControlsProps): React.JSX.Element {
  const tab = useBrowserSessionStore((state) => state.tabs.get(browserTabId))
  const tabUrl = tab?.url ?? ''
  const tabLoading = tab?.loading ?? false
  const agentControlled = tab?.agentControlled === true
  const viewportPreset = tab?.viewportPreset ?? 'desktop'
  const annotateMode = tab?.annotateMode ?? false
  const pendingCount = tab?.pendingAnnotations.length ?? 0
  const lastScreenshotCopiedAt = tab?.lastScreenshotCopiedAt

  const agentSessionId = useAgentTabSession(browserTabId)
  const agentStopped = useSessionBrowserStopped(agentSessionId)
  const overlayOpen = useInterfaceOverlayOpen()

  const cycleViewportPreset = useBrowserSessionStore((s) => s.cycleViewportPreset)
  const setAnnotateMode = useBrowserSessionStore((s) => s.setAnnotateMode)
  const setAnnotateTool = useBrowserSessionStore((s) => s.setAnnotateTool)
  const setStandaloneScreenshotArea = useBrowserSessionStore((s) => s.setStandaloneScreenshotArea)
  const bumpScreenshotAreaToken = useBrowserSessionStore((s) => s.bumpScreenshotAreaToken)
  const markScreenshotCopied = useBrowserSessionStore((s) => s.markScreenshotCopied)

  const [inputUrl, setInputUrl] = useState(tabUrl)
  const [urlFocused, setUrlFocused] = useState(false)
  const [copiedUrl, setCopiedUrl] = useState(false)
  const [copiedScreenshot, setCopiedScreenshot] = useState(false)
  const [screenshotOpen, setScreenshotOpen] = useState(false)
  const [stopPending, setStopPending] = useState(false)
  const inputRef = useRef<HTMLInputElement>(null)
  const annotateReqIdRef = useRef(0)
  const copyTimerRef = useRef<number | null>(null)
  const screenshotCopyTimerRef = useRef<number | null>(null)

  useEffect(() => {
    if (!urlFocused) {
      setInputUrl(tabUrl)
    }
  }, [tabUrl, urlFocused])

  useEffect(() => {
    if (!lastScreenshotCopiedAt) return
    const remainingMs = 2000 - (Date.now() - lastScreenshotCopiedAt)
    if (remainingMs <= 0) return
    setCopiedScreenshot(true)
    if (screenshotCopyTimerRef.current !== null) {
      window.clearTimeout(screenshotCopyTimerRef.current)
    }
    screenshotCopyTimerRef.current = window.setTimeout(() => {
      setCopiedScreenshot(false)
      screenshotCopyTimerRef.current = null
    }, remainingMs)
  }, [lastScreenshotCopiedAt])

  useEffect(() => {
    return () => {
      if (copyTimerRef.current !== null) {
        window.clearTimeout(copyTimerRef.current)
      }
      if (screenshotCopyTimerRef.current !== null) {
        window.clearTimeout(screenshotCopyTimerRef.current)
      }
    }
  }, [])

  const handleNavigate = useCallback(() => {
    const normalized = normalizeBrowserInputUrl(inputUrl)
    if (!normalized) return
    useBrowserSessionStore.getState().updateUrl(browserTabId, normalized)
    setUrlFocused(false)
    inputRef.current?.blur()
  }, [browserTabId, inputUrl])

  const handleUrlKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLInputElement>) => {
      if (e.key === 'Enter') {
        e.preventDefault()
        handleNavigate()
      } else if (e.key === 'Escape') {
        e.preventDefault()
        setInputUrl(tabUrl)
        inputRef.current?.blur()
      }
    },
    [handleNavigate, tabUrl]
  )

  const handleUrlBlur = useCallback(() => {
    setUrlFocused(false)
    setInputUrl(tabUrl)
  }, [tabUrl])

  const handleCopyAddress = useCallback(() => {
    if (!tabUrl) return
    if (copyTimerRef.current !== null) {
      window.clearTimeout(copyTimerRef.current)
    }
    setCopiedUrl(true)
    copyTimerRef.current = window.setTimeout(() => {
      setCopiedUrl(false)
      copyTimerRef.current = null
    }, 1100)
    void navigator.clipboard?.writeText(tabUrl).catch(() => {
      if (copyTimerRef.current !== null) {
        window.clearTimeout(copyTimerRef.current)
        copyTimerRef.current = null
      }
      setCopiedUrl(false)
    })
  }, [tabUrl])

  const handleOpenExternal = useCallback(() => {
    if (!tabUrl) return
    void openerApi.openUrlWithSystemBrowser(tabUrl).then((res) => {
      if (!res.success) {
        toast.error(res.error || 'Could not open URL in default browser.')
        logControlError('openUrlWithSystemBrowser', res.error)
      }
    })
  }, [tabUrl])

  const activateOverlay = useCallback(
    (tool: AnnotateTool, failureMessage: string) => {
      const reqId = ++annotateReqIdRef.current
      setAnnotateMode(browserTabId, true)
      setAnnotateTool(browserTabId, tool)
      void browserTabInjectAgentation(browserTabId)
        .then((result) => {
          if (reqId !== annotateReqIdRef.current) return
          if (!result.success && result.code !== 'NOT_TAURI') {
            setAnnotateMode(browserTabId, false)
            toast.error(result.error || failureMessage)
            logControlError(`injectAgentation(${tool})`, result.error ?? result.code)
            return
          }
          void browserTabAnnotationCmd(browserTabId, { type: 'set_active', active: true })
          void browserTabAnnotationCmd(browserTabId, { type: 'set_tool', tool })
        })
        .catch((e) => {
          if (reqId !== annotateReqIdRef.current) return
          setAnnotateMode(browserTabId, false)
          toast.error(failureMessage)
          logControlError(`injectAgentation(${tool})`, e)
        })
    },
    [browserTabId, setAnnotateMode, setAnnotateTool]
  )

  const handleEnterAnnotate = useCallback(() => {
    setStandaloneScreenshotArea(browserTabId, false)
    activateOverlay('comment', 'Could not start annotation mode.')
  }, [activateOverlay, browserTabId, setStandaloneScreenshotArea])

  const handleQuickScreenshotArea = useCallback(() => {
    setScreenshotOpen(false)
    bumpScreenshotAreaToken(browserTabId)
    setStandaloneScreenshotArea(browserTabId, !annotateMode)
    activateOverlay('screenshot-area', 'Could not start area capture.')
  }, [
    activateOverlay,
    annotateMode,
    browserTabId,
    bumpScreenshotAreaToken,
    setStandaloneScreenshotArea
  ])

  const handleExitAnnotate = useCallback(() => {
    annotateReqIdRef.current += 1
    setAnnotateMode(browserTabId, false)
    void browserTabAnnotationCmd(browserTabId, { type: 'set_active', active: false })
  }, [browserTabId, setAnnotateMode])

  const handleDefaultSendToChat = useCallback(
    async (
      markdown: string,
      crops: Array<{ name: string; base64: string; dataUrl: string; path?: string }>
    ): Promise<boolean> => {
      if (onSendAnnotationsToChat) {
        return await onSendAnnotationsToChat(markdown, crops)
      }
      if (!navigator.clipboard?.writeText) {
        toast.error('Clipboard is unavailable; annotations kept in batch.')
        return false
      }
      try {
        await navigator.clipboard.writeText(markdown)
        const savedCount = crops.filter((c) => Boolean(c.path)).length
        toast.success(
          savedCount > 0
            ? `Copied annotations and ${savedCount} saved crop ${savedCount === 1 ? 'path' : 'paths'} to clipboard for chat.`
            : 'Copied annotations to clipboard for chat.'
        )
        return true
      } catch (e) {
        logControlError('clipboard.writeText', e)
        toast.error('Could not write annotations to clipboard.')
        return false
      }
    },
    [onSendAnnotationsToChat]
  )

  const handleCaptureVisible = useCallback(async () => {
    setScreenshotOpen(false)
    await waitForOcclusionClear()
    const wasAnnotating =
      useBrowserSessionStore.getState().getTab(browserTabId)?.annotateMode === true
    if (wasAnnotating) {
      await browserTabAnnotationCmd(browserTabId, { type: 'set_capture_hidden', hidden: true })
      await new Promise<void>((resolve) => {
        requestAnimationFrame(() => {
          requestAnimationFrame(() => resolve())
        })
      })
    }
    try {
      const res = await browserTabScreenshot(browserTabId)
      if (!res.success) {
        toast.error(res.error || 'Could not capture screenshot.')
        logControlError('browserTabScreenshot', res.error ?? res.code)
        return
      }
      const copied = await copyBase64PngToClipboard(res.data.base64)
      if (copied.ok) {
        markScreenshotCopied(browserTabId)
        toast.success('Screenshot copied to clipboard')
      } else {
        toast.error(copied.error)
      }
    } finally {
      if (wasAnnotating) {
        void browserTabAnnotationCmd(browserTabId, { type: 'set_capture_hidden', hidden: false })
      }
    }
  }, [browserTabId, markScreenshotCopied])

  const handleStopAgent = useCallback(() => {
    if (!agentSessionId || stopPending) return
    setStopPending(true)
    void stopBrowserAgent(agentSessionId)
      .then((ok) => {
        if (!ok) {
          toast.error('Could not stop the agent browser. Try again.')
        }
      })
      .finally(() => setStopPending(false))
  }, [agentSessionId, stopPending])

  // Browser-scoped keyboard shortcuts (zoom is handled centrally in use-workspace-keyboard.ts)
  useEffect(() => {
    if (!isVisible || overlayOpen) return
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.defaultPrevented) return
      const ws = useWorkspaceStore.getState()
      const hasWorkspaceTabs = getAllLeafPanes(ws.root).some((leaf) => leaf.tabs.length > 0)
      if (hasWorkspaceTabs) {
        const activePane = findPaneById(ws.root, ws.activePaneId)
        const activeTabId = activePane?.type === 'leaf' ? activePane.activeTabId : null
        if (activeTabId !== workspaceBrowserTabId(browserTabId)) return
      }

      const target = e.target as HTMLElement | null
      const inEditable =
        target instanceof HTMLInputElement ||
        target instanceof HTMLTextAreaElement ||
        target?.isContentEditable
      const mod = e.metaKey || e.ctrlKey
      if (mod) {
        const key = e.key.toLowerCase()
        if (key === 'l' && !e.shiftKey && !e.altKey) {
          e.preventDefault()
          setUrlFocused(true)
          requestAnimationFrame(() => {
            inputRef.current?.focus()
            inputRef.current?.select()
          })
          return
        }
        if (inEditable) return
        if (e.shiftKey && key === 'r' && tabUrl) {
          e.preventDefault()
          browserTabReload(browserTabId).catch((err) => logControlError('browserTabReload', err))
        } else if (e.shiftKey && key === 's' && tabUrl) {
          e.preventDefault()
          void handleCaptureVisible()
        } else if (e.shiftKey && key === 'a' && tabUrl) {
          e.preventDefault()
          handleQuickScreenshotArea()
        }
        return
      }
      if (!annotateMode || inEditable) return
      const key = e.key.toLowerCase()
      if (key === 'c') {
        setAnnotateTool(browserTabId, 'comment')
        void browserTabAnnotationCmd(browserTabId, { type: 'set_tool', tool: 'comment' })
      } else if (key === 'r') {
        setAnnotateTool(browserTabId, 'rectangle')
        void browserTabAnnotationCmd(browserTabId, { type: 'set_tool', tool: 'rectangle' })
      } else if (key === 'm') {
        setAnnotateTool(browserTabId, 'ruler')
        void browserTabAnnotationCmd(browserTabId, { type: 'set_tool', tool: 'ruler' })
      } else if (key === 'p') {
        const next = useBrowserSessionStore.getState().toggleAnimationsPaused(browserTabId)
        void browserTabAnnotationCmd(browserTabId, { type: 'set_pause', paused: next })
      } else if (key === 'h') {
        const next = useBrowserSessionStore.getState().togglePinsHidden(browserTabId)
        void browserTabAnnotationCmd(browserTabId, { type: 'set_pins_hidden', hidden: next })
      }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [
    annotateMode,
    browserTabId,
    handleCaptureVisible,
    handleQuickScreenshotArea,
    isVisible,
    overlayOpen,
    setAnnotateTool,
    tabUrl
  ])

  const parts = splitDisplayUrl(tabUrl)
  const hasUrl = Boolean(tabUrl)

  const modeKey = agentControlled && !agentStopped ? 'agent' : annotateMode ? 'annotate' : 'default'

  return (
    <MotionConfig reducedMotion="user">
      <div className="relative flex shrink-0 flex-col bg-background">
        <div className="relative h-10 overflow-hidden">
          <AnimatePresence mode="popLayout" initial={false}>
            {modeKey === 'agent' ? (
              <motion.div
                key="agent"
                initial={{ opacity: 0, y: 4 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, y: -4 }}
                transition={{ duration: 0.15, ease: [0.22, 1, 0.36, 1] }}
                data-testid="browser-agent-toolbar"
                className="flex h-10 items-center gap-2 border-b border-border/80 bg-background px-2.5"
              >
                <span className="inline-flex h-6 shrink-0 items-center gap-1.5 rounded-full bg-primary/15 px-2.5 text-xs font-medium text-primary">
                  <span className="size-1.5 rounded-full bg-primary" />
                  <span>Agent driving</span>
                </span>

                <div className="flex h-7 min-w-0 flex-1 items-center gap-2 rounded-md bg-card px-2.5">
                  <Globe size={13} className="shrink-0 text-muted-foreground" aria-hidden="true" />
                  <div className="flex min-w-0 flex-1 items-center truncate text-xs font-medium">
                    {parts ? (
                      <>
                        <span className="text-muted-foreground">{parts.scheme}</span>
                        <span className="text-foreground">{parts.host}</span>
                        <span className="truncate text-muted-foreground">{parts.path}</span>
                      </>
                    ) : (
                      <span className="truncate text-foreground">{tabUrl}</span>
                    )}
                  </div>
                </div>

                <span className="hidden shrink-0 text-xs font-medium text-muted-foreground sm:inline">
                  Shared profile
                </span>

                {agentSessionId && (
                  <Button
                    type="button"
                    variant="destructive"
                    size="sm"
                    disabled={stopPending}
                    onClick={handleStopAgent}
                    aria-label="Stop agent browsing"
                    className="h-7 shrink-0 gap-1.5 px-2.5 text-xs"
                  >
                    <Square size={9} aria-hidden="true" />
                    <span>Stop</span>
                  </Button>
                )}
              </motion.div>
            ) : modeKey === 'annotate' ? (
              <motion.div
                key="annotate"
                initial={{ opacity: 0, y: 4 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, y: -4 }}
                transition={{ duration: 0.15, ease: [0.22, 1, 0.36, 1] }}
              >
                <AnnotationToolbar
                  browserTabId={browserTabId}
                  copiedScreenshot={copiedScreenshot}
                  onExit={handleExitAnnotate}
                  onCaptureVisible={handleCaptureVisible}
                  onSendToChat={handleDefaultSendToChat}
                />
              </motion.div>
            ) : (
              <motion.div
                key="default"
                initial={{ opacity: 0, y: 4 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, y: -4 }}
                transition={{ duration: 0.15, ease: [0.22, 1, 0.36, 1] }}
                className="flex h-10 items-center gap-1.5 border-b border-border/80 bg-background px-2"
              >
                <div className="flex shrink-0 items-center gap-0.5">
                  <button
                    type="button"
                    disabled={!hasUrl}
                    onClick={() => {
                      if (tab?.loadError && tab.previousUrl) {
                        useBrowserSessionStore.getState().updateUrl(browserTabId, tab.previousUrl)
                        return
                      }
                      browserTabGoBack(browserTabId).catch((err) =>
                        logControlError('browserTabGoBack', err)
                      )
                    }}
                    className={cn(
                      PANEL_ICON_BUTTON_CLASS,
                      'disabled:pointer-events-none disabled:opacity-40'
                    )}
                    aria-label="Back"
                    title="Back"
                  >
                    <ArrowLeft size={14} />
                  </button>
                  <button
                    type="button"
                    disabled={!hasUrl}
                    onClick={() =>
                      browserTabGoForward(browserTabId).catch((err) =>
                        logControlError('browserTabGoForward', err)
                      )
                    }
                    className={cn(
                      PANEL_ICON_BUTTON_CLASS,
                      'disabled:pointer-events-none disabled:opacity-40'
                    )}
                    aria-label="Forward"
                    title="Forward"
                  >
                    <ArrowRight size={14} />
                  </button>
                  <button
                    type="button"
                    disabled={!hasUrl}
                    onClick={() => {
                      if (tabLoading) {
                        useBrowserSessionStore.getState().bumpStopGeneration(browserTabId)
                        void browserTabStop(browserTabId)
                      } else {
                        browserTabReload(browserTabId).catch((err) =>
                          logControlError('browserTabReload', err)
                        )
                      }
                    }}
                    className={cn(
                      PANEL_ICON_BUTTON_CLASS,
                      'disabled:pointer-events-none disabled:opacity-40',
                      tabLoading && 'bg-control-fill text-foreground shadow-control'
                    )}
                    aria-label={tabLoading ? 'Stop loading' : 'Reload'}
                    title={tabLoading ? 'Stop' : 'Reload'}
                  >
                    <span className="t-icon-swap" data-state={tabLoading ? 'b' : 'a'}>
                      <span className="t-icon" data-icon="a">
                        <RefreshCw size={13} />
                      </span>
                      <span className="t-icon" data-icon="b">
                        <X size={13} />
                      </span>
                    </span>
                  </button>
                </div>

                {/* Borderless sunk URL well — neutral 2px focus ring, never blue */}
                <div
                  className={cn(
                    'flex h-7 min-w-0 flex-1 items-center gap-2 rounded-md px-2.5 transition-[background-color,box-shadow] duration-150 ease-out',
                    urlFocused ? 'bg-muted ring-2 ring-focus-ring' : 'bg-card'
                  )}
                >
                  {tabLoading ? (
                    <Spinner size={13} decorative className="shrink-0 text-primary" />
                  ) : parts?.isHttps ? (
                    <Lock size={12} className="shrink-0 text-muted-foreground" aria-hidden="true" />
                  ) : (
                    <Globe
                      size={13}
                      className="shrink-0 text-muted-foreground"
                      aria-hidden="true"
                    />
                  )}

                  <div className="relative flex min-w-0 flex-1 items-center">
                    {!urlFocused && parts && (
                      <button
                        type="button"
                        onClick={() => {
                          setUrlFocused(true)
                          requestAnimationFrame(() => {
                            inputRef.current?.focus()
                            inputRef.current?.select()
                          })
                        }}
                        className="flex w-full min-w-0 items-center truncate text-left text-xs font-medium"
                      >
                        <span className="text-muted-foreground">{parts.scheme}</span>
                        <span className="text-foreground">{parts.host}</span>
                        <span className="truncate text-muted-foreground">{parts.path}</span>
                      </button>
                    )}
                    <input
                      ref={inputRef}
                      type="text"
                      value={inputUrl}
                      onFocus={() => setUrlFocused(true)}
                      onBlur={handleUrlBlur}
                      onChange={(e) => setInputUrl(e.target.value)}
                      onKeyDown={handleUrlKeyDown}
                      placeholder={hasUrl ? 'Enter URL…' : ''}
                      aria-label="Address bar"
                      className={cn(
                        'min-w-0 flex-1 bg-transparent text-xs pointer-coarse:text-base font-medium text-foreground placeholder:text-muted-foreground',
                        INNER_FIELD_FOCUS_CLASS,
                        !urlFocused && parts && 'sr-only'
                      )}
                    />
                  </div>

                  {!hasUrl && !urlFocused && (
                    <span className="rounded bg-muted px-1.5 py-0.5 font-mono text-xs text-muted-foreground">
                      {isMac ? '⌘L' : 'Ctrl+L'}
                    </span>
                  )}

                  {hasUrl && !urlFocused && (
                    <button
                      type="button"
                      onClick={handleCopyAddress}
                      aria-label="Copy address"
                      title="Copy address"
                      className={cn(
                        'flex size-5 shrink-0 items-center justify-center rounded text-muted-foreground transition-colors duration-150 ease-out hover:text-foreground',
                        FOCUS_RING_CLASS
                      )}
                    >
                      <span className="t-icon-swap" data-state={copiedUrl ? 'b' : 'a'}>
                        <span className="t-icon" data-icon="a">
                          <Copy size={12} />
                        </span>
                        <span className="t-icon" data-icon="b">
                          <Check size={12} className="text-success" />
                        </span>
                      </span>
                    </button>
                  )}

                  <BrowserZoomControl browserTabId={browserTabId} />
                </div>

                {/* Canonical 4 right-hand controls: Device · Screenshot · Annotate · More */}
                <div className="flex shrink-0 items-center gap-1">
                  {copiedScreenshot && (
                    <span
                      role="status"
                      aria-live="polite"
                      data-testid="browser-screenshot-copied-pill"
                      className="inline-flex h-6 shrink-0 items-center gap-1 rounded-md border border-success/20 bg-success/10 px-2 text-xs font-medium text-success"
                    >
                      <Check size={12} aria-hidden="true" />
                      <span>Copied</span>
                    </span>
                  )}
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <button
                        type="button"
                        disabled={!hasUrl}
                        onClick={() => cycleViewportPreset(browserTabId)}
                        aria-label={`Switch responsive viewport (${viewportPreset})`}
                        data-testid="browser-device-toggle"
                        className={cn(
                          'inline-flex h-7 shrink-0 items-center justify-center gap-1.5 rounded-md transition-colors duration-150 ease-out disabled:pointer-events-none disabled:opacity-40',
                          FOCUS_RING_CLASS,
                          viewportPreset === 'desktop'
                            ? 'w-7 text-muted-foreground hover:bg-foreground/[0.03] hover:text-foreground'
                            : 'bg-control-fill px-2 text-xs font-medium text-foreground shadow-control hover:bg-control-fill-hover'
                        )}
                      >
                        {viewportPreset === 'desktop' ? (
                          <Monitor size={14} />
                        ) : viewportPreset === 'tablet' ? (
                          <>
                            <Tablet size={13} className="text-primary" />
                            <span>Tablet</span>
                          </>
                        ) : (
                          <>
                            <Smartphone size={13} className="text-primary" />
                            <span>Mobile</span>
                          </>
                        )}
                      </button>
                    </TooltipTrigger>
                    <TooltipContent side="bottom">Switch Desktop / Tablet / Mobile</TooltipContent>
                  </Tooltip>

                  <Popover open={screenshotOpen} onOpenChange={setScreenshotOpen}>
                    <PopoverTrigger asChild>
                      <button
                        type="button"
                        disabled={!hasUrl}
                        aria-label="Screenshot options"
                        title="Screenshot"
                        className={cn(
                          'inline-flex h-7 shrink-0 items-center justify-center gap-0.5 rounded-md px-1.5 transition-colors duration-150 ease-out disabled:pointer-events-none disabled:opacity-40',
                          FOCUS_RING_CLASS,
                          screenshotOpen
                            ? 'bg-control-fill text-foreground shadow-control'
                            : 'text-muted-foreground hover:bg-foreground/[0.03] hover:text-foreground'
                        )}
                      >
                        <Camera size={14} />
                        <ChevronDown size={10} className="opacity-70" />
                      </button>
                    </PopoverTrigger>
                    <PopoverContent
                      align="end"
                      sideOffset={6}
                      className={cn(MENU_CONTENT_CLASS, MENU_MOTION_CLASS, 'w-56')}
                    >
                      <button
                        type="button"
                        onClick={handleQuickScreenshotArea}
                        className={cn(
                          MENU_ITEM_CLASS,
                          'w-full justify-between text-foreground hover:bg-foreground/[0.06]'
                        )}
                      >
                        <span className="inline-flex items-center gap-2">
                          <Crop size={14} aria-hidden="true" />
                          Screenshot area
                        </span>
                        <span className={MENU_SHORTCUT_CLASS}>
                          {isMac ? '⇧⌘A' : 'Ctrl+Shift+A'}
                        </span>
                      </button>
                      <button
                        type="button"
                        onClick={() => void handleCaptureVisible()}
                        className={cn(
                          MENU_ITEM_CLASS,
                          'w-full justify-between text-foreground hover:bg-foreground/[0.06]'
                        )}
                      >
                        <span className="inline-flex items-center gap-2">
                          <Monitor size={14} aria-hidden="true" />
                          Screenshot visible
                        </span>
                        <span className={MENU_SHORTCUT_CLASS}>
                          {isMac ? '⇧⌘S' : 'Ctrl+Shift+S'}
                        </span>
                      </button>
                    </PopoverContent>
                  </Popover>

                  <Tooltip>
                    <TooltipTrigger asChild>
                      <button
                        type="button"
                        disabled={!hasUrl}
                        onClick={handleEnterAnnotate}
                        aria-label="Annotate page"
                        title="Annotate page"
                        className={cn(
                          'inline-flex h-7 shrink-0 items-center justify-center gap-1 rounded-md transition-colors duration-150 ease-out disabled:pointer-events-none disabled:opacity-40',
                          FOCUS_RING_CLASS,
                          pendingCount > 0
                            ? 'bg-control-fill px-2 text-xs font-medium text-foreground shadow-control hover:bg-control-fill-hover'
                            : 'w-7 text-muted-foreground hover:bg-foreground/[0.03] hover:text-foreground'
                        )}
                      >
                        <PenTool size={13} />
                        {pendingCount > 0 && (
                          <span className="font-mono text-xs tabular-nums">{pendingCount}</span>
                        )}
                      </button>
                    </TooltipTrigger>
                    <TooltipContent side="bottom">
                      Annotate page (pins, boxes & ruler)
                    </TooltipContent>
                  </Tooltip>

                  <BrowserMoreMenu
                    browserTabId={browserTabId}
                    hasUrl={hasUrl}
                    onCopyAddress={handleCopyAddress}
                    onOpenExternal={handleOpenExternal}
                    onHardReload={() =>
                      browserTabReload(browserTabId).catch((err) =>
                        logControlError('browserTabReload', err)
                      )
                    }
                    onOpenDevtools={
                      import.meta.env.PROD
                        ? undefined
                        : () =>
                            browserTabOpenDevtools(browserTabId).catch((err) =>
                              logControlError('browserTabOpenDevtools', err)
                            )
                    }
                  />
                </div>
              </motion.div>
            )}
          </AnimatePresence>
        </div>

        {/* Non-blocking 2px top progress bar during navigation */}
        {tabLoading && (
          <div
            data-testid="browser-loading-bar"
            className="pointer-events-none absolute right-0 bottom-0 left-0 h-[2px] overflow-hidden bg-transparent"
          >
            <div className="h-full w-2/3 animate-pulse bg-primary" />
          </div>
        )}
      </div>
    </MotionConfig>
  )
}
