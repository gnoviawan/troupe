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
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { Spinner } from '@/components/ui/spinner'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
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
import { logFrontendError } from '@/lib/log-api'
import { isMac } from '@/lib/platform'
import { cn } from '@/lib/utils'
import { useAgentTabSession, useSessionBrowserStopped } from '@/stores/browser-control-store'
import { useBrowserSessionStore } from '@/stores/browser-session-store'
import { AnnotationToolbar } from './AnnotationToolbar'
import { BrowserMoreMenu } from './BrowserMoreMenu'
import { normalizeBrowserInputUrl } from './BrowserStartPage'
import { BrowserZoomControl } from './BrowserZoomControl'

interface BrowserControlsProps {
  browserTabId: string
  isVisible?: boolean
  onSendAnnotationsToChat?: (
    markdown: string,
    crops: Array<{ name: string; base64: string; dataUrl: string }>
  ) => void
}

function splitDisplayUrl(
  raw: string
): { scheme: string; host: string; path: string; isHttps: boolean } | null {
  if (!raw) return null
  try {
    const parsed = new URL(raw)
    const scheme = `${parsed.protocol}//`
    const path = `${parsed.pathname}${parsed.search}${parsed.hash}`
    return {
      scheme,
      host: parsed.host,
      path: path === '/' ? '' : path,
      isHttps: parsed.protocol === 'https:'
    }
  } catch {
    return null
  }
}

export function BrowserControls({
  browserTabId,
  isVisible = true,
  onSendAnnotationsToChat
}: BrowserControlsProps): React.JSX.Element {
  const tab = useBrowserSessionStore((state) => state.tabs.get(browserTabId))
  const tabUrl = tab?.url ?? ''
  const tabLoading = tab?.loading ?? false
  const canGoBack = tab?.canGoBack ?? false
  const canGoForward = tab?.canGoForward ?? false
  const agentControlled = tab?.agentControlled ?? false
  const viewportPreset = tab?.viewportPreset ?? 'desktop'
  const annotateMode = tab?.annotateMode ?? false
  const pendingCount = tab?.pendingAnnotations.length ?? 0

  const agentSessionId = useAgentTabSession(browserTabId)
  const agentStopped = useSessionBrowserStopped(agentSessionId ?? '')

  const cycleViewportPreset = useBrowserSessionStore((s) => s.cycleViewportPreset)
  const setAnnotateMode = useBrowserSessionStore((s) => s.setAnnotateMode)
  const setAnnotateTool = useBrowserSessionStore((s) => s.setAnnotateTool)
  const stepZoom = useBrowserSessionStore((s) => s.stepZoom)
  const resetZoom = useBrowserSessionStore((s) => s.resetZoom)

  const [inputUrl, setInputUrl] = useState(tabUrl)
  const [urlFocused, setUrlFocused] = useState(false)
  const [copiedUrl, setCopiedUrl] = useState(false)
  const [screenshotOpen, setScreenshotOpen] = useState(false)
  const [stopPending, setStopPending] = useState(false)
  const inputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    if (!urlFocused) {
      setInputUrl(tabUrl)
    }
  }, [tabUrl, urlFocused])

  const handleNavigate = useCallback(() => {
    const normalized = normalizeBrowserInputUrl(inputUrl)
    if (!normalized) {
      setInputUrl(tabUrl)
      return
    }
    useBrowserSessionStore.getState().updateUrl(browserTabId, normalized)
    inputRef.current?.blur()
  }, [browserTabId, inputUrl, tabUrl])

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
    // Revert unsubmitted edits on blur instead of triggering surprise navigation.
    setInputUrl(tabUrl)
  }, [tabUrl])

  const handleCopyAddress = useCallback(() => {
    if (!tabUrl) return
    setCopiedUrl(true)
    const timer = window.setTimeout(() => setCopiedUrl(false), 1100)
    void navigator.clipboard?.writeText(tabUrl).catch(() => {
      window.clearTimeout(timer)
      setCopiedUrl(false)
    })
  }, [tabUrl])

  const handleOpenExternal = useCallback(() => {
    if (!tabUrl) return
    window.open(tabUrl, '_blank', 'noopener,noreferrer')
  }, [tabUrl])

  const handleEnterAnnotate = useCallback(() => {
    setAnnotateMode(browserTabId, true)
    void browserTabInjectAgentation(browserTabId)
      .then((result) => {
        if (!result.success && result.code !== 'NOT_TAURI') {
          setAnnotateMode(browserTabId, false)
          toast.error(result.error || 'Could not start annotation mode.')
          void logFrontendError({
            level: 'warn',
            message: `Toolbar injection failed: ${result.error ?? 'unknown'} [${result.code ?? 'NO_CODE'}]`,
            source: 'BrowserControls'
          })
          return
        }
        void browserTabAnnotationCmd(browserTabId, { type: 'set_active', active: true })
        void browserTabAnnotationCmd(browserTabId, { type: 'set_tool', tool: 'comment' })
      })
      .catch((e) => {
        setAnnotateMode(browserTabId, false)
        toast.error('Could not start annotation mode.')
        void logFrontendError({
          level: 'warn',
          message: `Toolbar injection error: ${e instanceof Error ? e.message : String(e)}`,
          source: 'BrowserControls'
        })
      })
  }, [browserTabId, setAnnotateMode])

  const handleExitAnnotate = useCallback(() => {
    setAnnotateMode(browserTabId, false)
    void browserTabAnnotationCmd(browserTabId, { type: 'set_active', active: false })
  }, [browserTabId, setAnnotateMode])

  const handleDefaultSendToChat = useCallback(
    (markdown: string, crops: Array<{ name: string; base64: string; dataUrl: string }>) => {
      if (onSendAnnotationsToChat) {
        onSendAnnotationsToChat(markdown, crops)
        return
      }
      void navigator.clipboard?.writeText(markdown)
      toast.success('Copied annotations to clipboard for chat.')
    },
    [onSendAnnotationsToChat]
  )

  const handleQuickScreenshotVisible = useCallback(async () => {
    setScreenshotOpen(false)
    const res = await browserTabScreenshot(browserTabId)
    if (!res.success) {
      toast.error(res.error || 'Could not capture screenshot.')
      return
    }
    const dataUrl = `data:${res.data.mimeType};base64,${res.data.base64}`
    handleDefaultSendToChat(`Viewport screenshot of ${tabUrl || 'current page'}`, [
      { name: 'browser-visible.png', base64: res.data.base64, dataUrl }
    ])
  }, [browserTabId, handleDefaultSendToChat, tabUrl])

  const handleQuickScreenshotArea = useCallback(() => {
    setScreenshotOpen(false)
    setAnnotateMode(browserTabId, true)
    setAnnotateTool(browserTabId, 'screenshot-area')
    void browserTabInjectAgentation(browserTabId)
      .then((result) => {
        if (!result.success && result.code !== 'NOT_TAURI') {
          setAnnotateMode(browserTabId, false)
          toast.error(result.error || 'Could not start area capture.')
          void logFrontendError({
            level: 'warn',
            message: `Area capture injection failed: ${result.error ?? 'unknown'} [${result.code ?? 'NO_CODE'}]`,
            source: 'BrowserControls'
          })
          return
        }
        void browserTabAnnotationCmd(browserTabId, { type: 'set_active', active: true })
        void browserTabAnnotationCmd(browserTabId, { type: 'set_tool', tool: 'screenshot-area' })
      })
      .catch((e) => {
        setAnnotateMode(browserTabId, false)
        toast.error('Could not start area capture.')
        void logFrontendError({
          level: 'warn',
          message: `Area capture injection error: ${e instanceof Error ? e.message : String(e)}`,
          source: 'BrowserControls'
        })
      })
  }, [browserTabId, setAnnotateMode, setAnnotateTool])

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

  // Panel-level keyboard shortcuts while this browser tab is visible
  useEffect(() => {
    if (!isVisible) return
    const onKeyDown = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null
      const inEditable =
        target instanceof HTMLInputElement ||
        target instanceof HTMLTextAreaElement ||
        target?.isContentEditable
      const mod = e.metaKey || e.ctrlKey
      if (mod) {
        const key = e.key.toLowerCase()
        if (key === 'l') {
          e.preventDefault()
          setUrlFocused(true)
          requestAnimationFrame(() => {
            inputRef.current?.focus()
            inputRef.current?.select()
          })
        } else if (e.key === '=' || e.key === '+') {
          e.preventDefault()
          stepZoom(browserTabId, 'in')
        } else if (e.key === '-') {
          e.preventDefault()
          stepZoom(browserTabId, 'out')
        } else if (e.key === '0') {
          e.preventDefault()
          resetZoom(browserTabId)
        } else if (e.shiftKey && key === 'r' && tabUrl) {
          e.preventDefault()
          browserTabReload(browserTabId).catch(console.error)
        } else if (e.shiftKey && key === 's' && tabUrl) {
          e.preventDefault()
          void handleQuickScreenshotVisible()
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
    handleQuickScreenshotArea,
    handleQuickScreenshotVisible,
    isVisible,
    resetZoom,
    setAnnotateTool,
    stepZoom,
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
                  <div className="flex min-w-0 flex-1 items-center truncate text-[13px] font-medium">
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
                  <button
                    type="button"
                    disabled={stopPending}
                    onClick={handleStopAgent}
                    aria-label="Stop agent browsing"
                    className="tactile-destructive inline-flex h-7 shrink-0 items-center gap-1.5 rounded-md bg-destructive/15 px-2.5 text-xs font-medium tracking-[-0.01em] text-destructive transition-[transform,background-color] duration-150 hover:bg-destructive/20 active:scale-[0.97] disabled:opacity-50"
                  >
                    <Square size={9} aria-hidden="true" />
                    <span>Stop</span>
                  </button>
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
                  onExit={handleExitAnnotate}
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
                    disabled={!hasUrl || (!canGoBack && !(tab?.loadError && tab?.previousUrl))}
                    onClick={() => {
                      if (tab?.loadError && tab.previousUrl) {
                        useBrowserSessionStore.getState().updateUrl(browserTabId, tab.previousUrl)
                        return
                      }
                      browserTabGoBack(browserTabId).catch(console.error)
                    }}
                    className="flex size-7 items-center justify-center rounded-md text-secondary-foreground transition-colors duration-150 hover:bg-muted/60 hover:text-foreground active:scale-[0.94] disabled:pointer-events-none disabled:opacity-40"
                    aria-label="Back"
                    title="Back"
                  >
                    <ArrowLeft size={14} />
                  </button>
                  <button
                    type="button"
                    disabled={!hasUrl || !canGoForward}
                    onClick={() => browserTabGoForward(browserTabId).catch(console.error)}
                    className="flex size-7 items-center justify-center rounded-md text-secondary-foreground transition-colors duration-150 hover:bg-muted/60 hover:text-foreground active:scale-[0.94] disabled:pointer-events-none disabled:opacity-40"
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
                        void browserTabStop(browserTabId)
                        useBrowserSessionStore.getState().setLoading(browserTabId, false)
                      } else {
                        browserTabReload(browserTabId).catch(console.error)
                      }
                    }}
                    className={cn(
                      'flex size-7 items-center justify-center rounded-md transition-colors duration-150 active:scale-[0.94] disabled:pointer-events-none disabled:opacity-40',
                      tabLoading
                        ? 'tactile-control bg-muted text-foreground'
                        : 'text-secondary-foreground hover:bg-muted/60 hover:text-foreground'
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
                    'flex h-7 min-w-0 flex-1 items-center gap-2 rounded-md px-2.5 transition-[background-color,box-shadow] duration-150',
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
                        className="flex w-full min-w-0 items-center truncate text-left text-[13px] font-medium"
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
                        'min-w-0 flex-1 bg-transparent text-[13px] font-medium text-foreground placeholder:text-muted-foreground focus:outline-none',
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
                      className="flex size-5 shrink-0 items-center justify-center rounded text-muted-foreground transition-colors duration-150 hover:text-foreground"
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
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <button
                        type="button"
                        disabled={!hasUrl}
                        onClick={() => cycleViewportPreset(browserTabId)}
                        aria-label={`Switch responsive viewport (${viewportPreset})`}
                        data-testid="browser-device-toggle"
                        className={cn(
                          'inline-flex h-7 shrink-0 items-center justify-center gap-1.5 rounded-md transition-[transform,background-color,color] duration-150 active:scale-[0.96] disabled:pointer-events-none disabled:opacity-40',
                          viewportPreset === 'desktop'
                            ? 'w-7 text-muted-foreground hover:bg-muted/60 hover:text-foreground'
                            : 'tactile-control bg-muted px-2 text-xs font-medium text-foreground hover:bg-secondary'
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
                          'inline-flex h-7 shrink-0 items-center justify-center gap-0.5 rounded-md px-1.5 transition-colors duration-150 active:scale-[0.96] disabled:pointer-events-none disabled:opacity-40',
                          screenshotOpen
                            ? 'tactile-control bg-muted text-foreground'
                            : 'text-muted-foreground hover:bg-muted/60 hover:text-foreground'
                        )}
                      >
                        <Camera size={14} />
                        <ChevronDown size={10} className="opacity-70" />
                      </button>
                    </PopoverTrigger>
                    <PopoverContent
                      align="end"
                      sideOffset={6}
                      className="t-dropdown tactile-menu w-56 rounded-lg border-0 bg-popover p-1"
                    >
                      <button
                        type="button"
                        onClick={handleQuickScreenshotArea}
                        className="flex h-8 w-full items-center justify-between rounded-md px-2 text-[13px] font-medium text-foreground transition-colors duration-150 hover:bg-muted"
                      >
                        <span className="inline-flex items-center gap-2">
                          <Crop size={14} aria-hidden="true" />
                          Screenshot area
                        </span>
                        <span className="font-mono text-xs text-muted-foreground">
                          {isMac ? '⇧⌘A' : 'Ctrl+Shift+A'}
                        </span>
                      </button>
                      <button
                        type="button"
                        onClick={() => void handleQuickScreenshotVisible()}
                        className="flex h-8 w-full items-center justify-between rounded-md px-2 text-[13px] font-medium text-secondary-foreground transition-colors duration-150 hover:bg-muted hover:text-foreground"
                      >
                        <span className="inline-flex items-center gap-2">
                          <Monitor size={14} aria-hidden="true" />
                          Screenshot visible
                        </span>
                        <span className="font-mono text-xs text-muted-foreground">
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
                          'inline-flex h-7 shrink-0 items-center justify-center gap-1.5 rounded-md transition-colors duration-150 active:scale-[0.96] disabled:pointer-events-none disabled:opacity-40',
                          pendingCount > 0
                            ? 'tactile-control bg-card px-2 text-xs font-medium text-foreground hover:bg-muted'
                            : 'w-7 text-muted-foreground hover:bg-muted/60 hover:text-foreground'
                        )}
                      >
                        <PenTool size={13} />
                        {pendingCount > 0 && (
                          <span className="inline-flex min-w-[18px] items-center justify-center rounded-full bg-primary/15 px-1 font-mono text-xs font-medium text-primary">
                            {pendingCount}
                          </span>
                        )}
                      </button>
                    </TooltipTrigger>
                    <TooltipContent side="bottom">Annotate page</TooltipContent>
                  </Tooltip>

                  <BrowserMoreMenu
                    browserTabId={browserTabId}
                    hasUrl={hasUrl}
                    onCopyAddress={handleCopyAddress}
                    onOpenExternal={handleOpenExternal}
                    onHardReload={() => browserTabReload(browserTabId).catch(console.error)}
                    onOpenDevtools={
                      !import.meta.env.PROD
                        ? () => browserTabOpenDevtools(browserTabId).catch(console.error)
                        : undefined
                    }
                  />
                </div>
              </motion.div>
            )}
          </AnimatePresence>
        </div>

        {/* Non-blocking 2px loading progress hairline along bottom edge */}
        {tabLoading && hasUrl && (
          <div
            aria-hidden="true"
            data-testid="browser-loading-bar"
            className="pointer-events-none absolute right-0 bottom-0 left-0 h-[2px] overflow-hidden bg-border/60"
          >
            <div className="h-full w-2/3 origin-left animate-pulse bg-primary transition-transform duration-200 ease-[cubic-bezier(0.22,1,0.36,1)]" />
          </div>
        )}
      </div>
    </MotionConfig>
  )
}
