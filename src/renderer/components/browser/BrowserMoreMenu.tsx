import { useState } from 'react'
import { Bug, ExternalLink, MoreHorizontal, RefreshCw } from '@/components/icons'
import {
  MENU_CONTENT_CLASS,
  MENU_ITEM_CLASS,
  MENU_MOTION_CLASS,
  MENU_SEPARATOR_CLASS,
  MENU_SHORTCUT_CLASS
} from '@/components/ui/menu-styles'
import { PANEL_ICON_BUTTON_CLASS } from '@/components/ui/panel-styles'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { isMac } from '@/lib/platform'
import { cn } from '@/lib/utils'
import { BrowserZoomStepper } from './BrowserZoomStepper'

interface BrowserMoreMenuProps {
  browserTabId: string
  hasUrl: boolean
  onCopyAddress: () => void
  onOpenExternal: () => void
  onHardReload: () => void
  onOpenDevtools?: () => void
}

export function BrowserMoreMenu({
  browserTabId,
  hasUrl,
  onCopyAddress,
  onOpenExternal,
  onHardReload,
  onOpenDevtools
}: BrowserMoreMenuProps): React.JSX.Element {
  const [open, setOpen] = useState(false)

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          type="button"
          aria-label="More browser actions"
          title="More"
          className={cn(
            PANEL_ICON_BUTTON_CLASS,
            'shrink-0',
            open && 'bg-control-fill text-foreground shadow-control'
          )}
        >
          <MoreHorizontal size={14} />
        </button>
      </PopoverTrigger>
      <PopoverContent
        align="end"
        sideOffset={6}
        className={cn(MENU_CONTENT_CLASS, MENU_MOTION_CLASS, 'w-60')}
      >
        <div className="flex h-8 items-center justify-between px-2">
          <span className="text-xs font-medium text-foreground">Zoom</span>
          <BrowserZoomStepper browserTabId={browserTabId} disabled={!hasUrl} disableResetAt100 />
        </div>

        <div className={MENU_SEPARATOR_CLASS} />

        <div className="flex flex-col gap-0.5">
          <button
            type="button"
            disabled={!hasUrl}
            onClick={() => {
              onCopyAddress()
              setOpen(false)
            }}
            className={cn(
              MENU_ITEM_CLASS,
              'w-full justify-between text-foreground hover:bg-foreground/[0.06] disabled:pointer-events-none disabled:opacity-40'
            )}
          >
            <span>Copy address</span>
          </button>

          <button
            type="button"
            disabled={!hasUrl}
            onClick={() => {
              onOpenExternal()
              setOpen(false)
            }}
            className={cn(
              MENU_ITEM_CLASS,
              'w-full justify-between text-foreground hover:bg-foreground/[0.06] disabled:pointer-events-none disabled:opacity-40'
            )}
          >
            <span>Open in default browser</span>
            <ExternalLink size={12} className="text-muted-foreground" aria-hidden="true" />
          </button>
        </div>

        <div className={MENU_SEPARATOR_CLASS} />

        <div className="flex flex-col gap-0.5">
          <button
            type="button"
            disabled={!hasUrl}
            onClick={() => {
              onHardReload()
              setOpen(false)
            }}
            className={cn(
              MENU_ITEM_CLASS,
              'w-full justify-between text-foreground hover:bg-foreground/[0.06] disabled:pointer-events-none disabled:opacity-40'
            )}
          >
            <span className="inline-flex items-center gap-1.5">
              <RefreshCw size={12} className="text-muted-foreground" aria-hidden="true" />
              Hard reload
            </span>
            <span className={MENU_SHORTCUT_CLASS}>{isMac ? '⇧⌘R' : 'Ctrl+Shift+R'}</span>
          </button>

          {onOpenDevtools && (
            <button
              type="button"
              disabled={!hasUrl}
              aria-label="Open debug console"
              onClick={() => {
                onOpenDevtools()
                setOpen(false)
              }}
              className={cn(
                MENU_ITEM_CLASS,
                'w-full justify-between text-muted-foreground hover:bg-foreground/[0.06] hover:text-foreground disabled:pointer-events-none disabled:opacity-40'
              )}
            >
              <span className="inline-flex items-center gap-1.5">
                <Bug size={12} aria-hidden="true" />
                Debug console (dev)
              </span>
            </button>
          )}
        </div>
      </PopoverContent>
    </Popover>
  )
}
