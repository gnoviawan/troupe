import { useRef, useState } from 'react'
import { RotateCw } from '@/components/icons'
import { Button } from '@/components/ui/button'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { useMobileWebShell } from '@/hooks/use-mobile-web-shell'
import { basename } from '@/lib/path-basename'
import { cn } from '@/lib/utils'
import { ViewerLoading } from '../media/viewer-chrome'
import { ConsolePanel, ConsoleToggle } from './PreviewConsole'
import { useHtmlPreviewSession } from './use-html-preview-session'
import { usePreviewConsole } from './use-preview-console'

/** Scripts run, but never with the app's origin: no `allow-same-origin`. */
export const PREVIEW_SANDBOX = 'allow-scripts allow-forms allow-modals allow-popups'

interface HtmlPreviewProps {
  filePath: string
  /** The editor buffer; the preview shows it, saved or not. */
  content: string
}

export function HtmlPreview({ filePath, content }: HtmlPreviewProps): React.JSX.Element {
  const { state, reload } = useHtmlPreviewSession(filePath, content)
  const frameRef = useRef<HTMLIFrameElement>(null)
  const src = state.status === 'open' ? state.src : null
  const entries = usePreviewConsole(frameRef, src)
  const [consoleOpen, setConsoleOpen] = useState(false)
  const touch = useMobileWebShell()

  return (
    <div className="absolute inset-0 flex flex-col bg-background">
      <div
        className={cn(
          'flex shrink-0 items-center justify-end gap-1 border-b-[0.5px] border-border',
          touch ? 'min-h-11 px-2' : 'h-8 px-1.5'
        )}
      >
        <ConsoleToggle
          entries={entries}
          open={consoleOpen}
          onToggle={() => setConsoleOpen((open) => !open)}
          touch={touch}
        />
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              variant="ghost"
              size={touch ? 'touch' : 'icon-xs'}
              aria-label="Reload preview"
              onClick={reload}
              disabled={state.status === 'opening'}
              className={cn('text-muted-foreground [&_svg]:size-3.5', touch && 'min-w-11')}
            >
              <RotateCw aria-hidden="true" />
            </Button>
          </TooltipTrigger>
          <TooltipContent className="text-xs">Reload preview</TooltipContent>
        </Tooltip>
      </div>
      <div className="relative min-h-0 flex-1">
        {state.status === 'failed' ? (
          <div className="flex h-full items-center justify-center p-4">
            <p className="text-xs text-muted-foreground">
              The preview could not be loaded. Reload to try again.
            </p>
          </div>
        ) : src === null ? (
          <ViewerLoading />
        ) : (
          <iframe
            ref={frameRef}
            src={src}
            title={`Preview of ${basename(filePath)}`}
            sandbox={PREVIEW_SANDBOX}
            referrerPolicy="no-referrer"
            // A page without its own background expects the browser's white
            // canvas, not the editor's dark surface behind the frame.
            className="block size-full border-0 bg-[Canvas] [color-scheme:light]"
          />
        )}
      </div>
      {consoleOpen && <ConsolePanel entries={entries} />}
    </div>
  )
}
