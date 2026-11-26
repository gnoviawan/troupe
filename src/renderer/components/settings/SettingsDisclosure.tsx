import { useCallback, useEffect, useId, useRef, useState } from 'react'
import { ChevronDown } from '@/components/icons'
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible'
import { FOCUS_RING_CLASS } from '@/components/ui/panel-styles'
import { cn } from '@/lib/utils'
import {
  SETTINGS_CARD_CLASS,
  SETTINGS_SPLIT_GROUP_CLASS,
  type SettingsGroupLayout
} from './SettingsGroup'
import { SETTINGS_REVEAL_EVENT } from './SettingsLayout'

interface SettingsDisclosureProps {
  /** Card title, e.g. "Advanced". */
  title: string
  /** Concise helper copy rendered in the left column on wide settings pages. */
  description?: React.ReactNode
  /** One line that sums up the hidden rows while closed. */
  summary?: React.ReactNode
  /** Rows shown when open. */
  children: React.ReactNode
  defaultOpen?: boolean
  /** Controlled open state (a search jump opens it). */
  open?: boolean
  onOpenChange?: (open: boolean) => void
  /**
   * `'split'` (default) aligns with split settings groups on wide pages;
   * `'stacked'` keeps a single full-width card (e.g. inside another group).
   */
  layout?: SettingsGroupLayout
  id?: string
}

/**
 * One card for settings most people never change: a title and a summary line,
 * closed by default. Opening it shows the rows under a hairline. The rows stay
 * mounted, so search anchors inside still exist.
 */
export function SettingsDisclosure({
  title,
  description,
  summary,
  children,
  defaultOpen = false,
  open: openProp,
  onOpenChange,
  layout = 'split',
  id
}: SettingsDisclosureProps): React.JSX.Element {
  const [uncontrolledOpen, setUncontrolledOpen] = useState(defaultOpen)
  const open = openProp ?? uncontrolledOpen
  const contentId = useId()
  const rootRef = useRef<HTMLDivElement>(null)

  const setOpen = useCallback(
    (next: boolean): void => {
      if (openProp === undefined) setUncontrolledOpen(next)
      onOpenChange?.(next)
    },
    [openProp, onOpenChange]
  )

  // A settings search that jumps to a row inside opens the card first.
  useEffect(() => {
    const root = rootRef.current
    if (!root) return
    const reveal = (): void => setOpen(true)
    root.addEventListener(SETTINGS_REVEAL_EVENT, reveal)
    return () => root.removeEventListener(SETTINGS_REVEAL_EVENT, reveal)
  }, [setOpen])

  const isSplit = layout === 'split'

  const collapsible = (
    <Collapsible
      ref={rootRef}
      open={open}
      onOpenChange={setOpen}
      id={isSplit ? undefined : id}
      className={cn('@container flex min-w-0 flex-col overflow-hidden', SETTINGS_CARD_CLASS)}
    >
      <CollapsibleTrigger
        aria-controls={contentId}
        className={cn(
          'flex w-full items-center gap-3 px-4 py-3 text-left transition-colors duration-150 ease-out hover:bg-foreground/[0.03] pointer-coarse:min-h-11',
          FOCUS_RING_CLASS
        )}
      >
        <span className="min-w-0 flex-1">
          <span
            className={cn(
              'block text-xs font-medium leading-4 text-foreground',
              isSplit && '@[52rem]/settings:sr-only'
            )}
          >
            {title}
          </span>
          {!open ? (
            summary ? (
              <span
                className={cn(
                  'mt-0.5 block truncate text-xs leading-4 text-muted-foreground',
                  isSplit && '@[52rem]/settings:mt-0'
                )}
              >
                {summary}
              </span>
            ) : isSplit ? (
              <span className="hidden text-xs leading-4 text-muted-foreground @[52rem]/settings:block">
                Show options
              </span>
            ) : null
          ) : isSplit ? (
            <span className="hidden text-xs font-medium leading-4 text-muted-foreground @[52rem]/settings:block">
              Hide options
            </span>
          ) : null}
        </span>
        <ChevronDown
          size={14}
          aria-hidden
          className={cn(
            'shrink-0 text-muted-foreground transition-transform duration-150 ease-out motion-reduce:transition-none',
            open && 'rotate-180'
          )}
        />
      </CollapsibleTrigger>
      <CollapsibleContent
        id={contentId}
        forceMount
        hidden={!open}
        className="flex flex-col divide-y-[0.5px] divide-border border-t-[0.5px] border-border"
      >
        {children}
      </CollapsibleContent>
    </Collapsible>
  )

  if (!isSplit) return collapsible

  return (
    <div id={id} className={SETTINGS_SPLIT_GROUP_CLASS}>
      <div
        aria-hidden="true"
        className="hidden @[52rem]/settings:flex @[52rem]/settings:flex-col @[52rem]/settings:gap-1.5 @[52rem]/settings:pt-1"
      >
        <span className="text-sm font-medium text-foreground">{title}</span>
        {description && (
          <p className="text-xs leading-relaxed text-muted-foreground">{description}</p>
        )}
      </div>
      {collapsible}
    </div>
  )
}
