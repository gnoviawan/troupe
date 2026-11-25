import { forwardRef, useId } from 'react'
import { cn } from '@/lib/utils'

interface SettingsGroupProps extends Omit<React.HTMLAttributes<HTMLElement>, 'title'> {
  /** Group heading, sentence case. */
  title?: React.ReactNode
  /** Quiet text after the heading: a count or a summary ("4 servers · 3 on"). */
  meta?: React.ReactNode
  /** Controls that act on the whole group (refresh, add), at the right of the heading. */
  actions?: React.ReactNode
  /** Short note under the group. */
  footnote?: React.ReactNode
}

/**
 * A titled section of settings on a page: a 14/20 heading with its actions at
 * the right, then 12px, then the content. Put rows inside a
 * {@link SettingsCard}; anything else (a filter, a notice, a list) can sit
 * between them.
 */
export const SettingsGroup = forwardRef<HTMLElement, SettingsGroupProps>(function SettingsGroup(
  { title, meta, actions, footnote, className, children, ...props },
  ref
) {
  const titleId = useId()
  return (
    <section
      ref={ref}
      aria-labelledby={title ? titleId : undefined}
      className={cn('flex flex-col gap-3', className)}
      {...props}
    >
      {(title || actions) && (
        <div className="flex min-h-7 flex-wrap items-center justify-between gap-x-3 gap-y-1">
          {title && (
            <div className="flex min-w-0 items-baseline gap-2">
              <h3 id={titleId} className="text-sm font-medium text-foreground">
                {title}
              </h3>
              {meta && (
                <span className="truncate text-xs tabular-nums text-muted-foreground">{meta}</span>
              )}
            </div>
          )}
          {actions && <div className="flex flex-wrap items-center gap-2">{actions}</div>}
        </div>
      )}
      {children}
      {footnote && <p className="text-xs leading-relaxed text-muted-foreground">{footnote}</p>}
    </section>
  )
})

/**
 * Raised card recipe for settings: the card fill one step above the page, a
 * 0.5px edge and a tight black drop (`shadow-control`), radius 8.
 */
export const SETTINGS_CARD_CLASS = 'rounded-lg bg-card shadow-control'

/**
 * The card that holds a group's rows. Rows get 0.5px hairlines between them,
 * and the card is the container that narrow-layout rules measure.
 */
export const SettingsCard = forwardRef<HTMLDivElement, React.HTMLAttributes<HTMLDivElement>>(
  function SettingsCard({ className, ...props }, ref) {
    return (
      <div
        ref={ref}
        className={cn(
          '@container flex flex-col divide-y-[0.5px] divide-border overflow-hidden',
          SETTINGS_CARD_CLASS,
          className
        )}
        {...props}
      />
    )
  }
)
