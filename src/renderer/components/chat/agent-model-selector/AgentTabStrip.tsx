import { motion } from 'framer-motion'
import { type KeyboardEvent, type ReactNode, useRef } from 'react'
import { cn } from '@/lib/utils'
import { ROW_FOCUS } from './SelectorOptionRows'
import { AGENT_TRACK_CLASS } from './selector-model'
import { PICKER_ROW_SELECTED } from './selector-tokens'

/** `--ease-smooth-out`; framer needs the curve itself, not the variable. */
const EASE_OUT = [0.22, 1, 0.36, 1] as const
export const quick = (reduced: boolean) => ({ duration: reduced ? 0 : 0.15, ease: EASE_OUT })

export interface TabItem {
  key: string
  label: string
  glyph: ReactNode
  selected: boolean
  title: string
  /** Sits at the track's far end (right, or bottom in the rail): "More". */
  end?: boolean
  onSelect: () => void
}

const TAB_KEYS = {
  horizontal: { next: 'ArrowRight', prev: 'ArrowLeft' },
  vertical: { next: 'ArrowDown', prev: 'ArrowUp' }
} as const

/**
 * Agent tabs as glyphs. Horizontal: a strip over a hairline, the active tab
 * underlined. Vertical: a rail left of the list, the active tab filled. The
 * mark slides between tabs. Tabs pattern: one tab stop
 * (the selected tab); the arrow keys of the orientation, Home and End move
 * focus, Enter or Space selects. With `followFocus` the arrows select too.
 */
export function AgentTabStrip({
  id,
  items,
  orientation,
  followFocus,
  disabled,
  touch,
  reduced
}: {
  id: string
  items: TabItem[]
  orientation: 'horizontal' | 'vertical'
  followFocus: boolean
  disabled: boolean
  touch: boolean
  reduced: boolean
}): React.JSX.Element {
  const ref = useRef<HTMLDivElement>(null)
  const vertical = orientation === 'vertical'
  // One tab stop: the selected tab, or the first when none is selected.
  const stop = Math.max(
    items.findIndex((item) => item.selected),
    0
  )
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>): void => {
    const tabs = Array.from(ref.current?.querySelectorAll<HTMLElement>('[role="tab"]') ?? [])
    const index = tabs.indexOf(document.activeElement as HTMLElement)
    if (index < 0) return
    const last = tabs.length - 1
    const keys = TAB_KEYS[orientation]
    const next =
      event.key === keys.next
        ? index === last
          ? 0
          : index + 1
        : event.key === keys.prev
          ? index === 0
            ? last
            : index - 1
          : event.key === 'Home'
            ? 0
            : event.key === 'End'
              ? last
              : null
    if (next == null) return
    event.preventDefault()
    tabs[next]?.focus()
    if (followFocus) items[next]?.onSelect()
  }
  return (
    <div
      ref={ref}
      role="tablist"
      aria-label="Agents"
      aria-orientation={orientation}
      onKeyDown={onKeyDown}
      className={cn(
        'flex',
        vertical
          ? // `box-content`: the hairline sits outside the 40px (56px touch) column,
            // so a 28px (44px) tab has 6px on each side and its centre is the rail's.
            cn(
              'box-content shrink-0 flex-col items-center border-r border-border',
              AGENT_TRACK_CLASS.railPadding,
              touch ? 'w-14' : 'w-10'
            )
          : cn('border-b-[0.5px] border-border', touch ? 'px-3' : 'px-1')
      )}
    >
      {items.map((item, index) => (
        <button
          key={item.key}
          type="button"
          role="tab"
          aria-selected={item.selected}
          aria-label={item.label}
          title={item.title}
          tabIndex={index === stop ? 0 : -1}
          disabled={disabled}
          data-testid={`agent-tab-${item.key}`}
          onClick={item.onSelect}
          className={cn(
            'relative flex items-center justify-center transition-colors duration-150 ease-out disabled:cursor-not-allowed disabled:opacity-60',
            vertical
              ? // A square as tall as a list row (28px), so each icon sits on the
                // list's row grid; 6px radius like the rows.
                cn('rounded-md', touch ? AGENT_TRACK_CLASS.railTabTouch : AGENT_TRACK_CLASS.railTab)
              : cn(
                  'rounded-t-md',
                  touch
                    ? cn(AGENT_TRACK_CLASS.stripTabTouch, 'h-11')
                    : cn(AGENT_TRACK_CLASS.stripTab, 'h-8')
                ),
            item.end && (vertical ? 'mt-auto' : 'ml-auto'),
            // Icons keep their colour (brand or foreground); state reads from
            // the icon's opacity, as the active fill alone is too faint.
            'group/tab text-foreground',
            !item.selected && (vertical ? 'hover:bg-foreground/5' : 'hover:bg-foreground/[0.06]'),
            ROW_FOCUS
          )}
        >
          {item.selected ? (
            <motion.span
              layoutId={`${id}-indicator`}
              aria-hidden="true"
              transition={quick(reduced)}
              className={cn(
                'pointer-events-none absolute',
                // Rail: the popover's active-segment fill (docs/design/colors.md)
                // filling the tab. Top strip: a bar under the tab.
                vertical
                  ? cn('inset-0 rounded-md', PICKER_ROW_SELECTED)
                  : 'inset-x-2 -bottom-px h-0.5 rounded-full bg-foreground'
              )}
            />
          ) : null}
          <span
            data-tab-glyph=""
            className={cn(
              'relative inline-flex transition-opacity duration-150 ease-out motion-reduce:transition-none',
              item.selected ? 'opacity-100' : 'opacity-55 group-hover/tab:opacity-80'
            )}
          >
            {item.glyph}
          </span>
        </button>
      ))}
    </div>
  )
}
