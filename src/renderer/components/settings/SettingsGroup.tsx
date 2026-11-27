import { forwardRef, useId } from 'react'
import { cn } from '@/lib/utils'

export type SettingsGroupLayout = 'split' | 'stacked'

/**
 * Shared split-section grid for wide settings pages. Below `52rem` (832px) or
 * outside `@container/settings` (e.g. onboarding), content stays stacked.
 * At `52rem`, 240px left + 32px gap leaves at least 560px (`35rem`) on the
 * right so inner `SettingsRow` items remain horizontal.
 */
export const SETTINGS_SPLIT_GROUP_CLASS =
  'flex flex-col gap-3 @[52rem]/settings:grid @[52rem]/settings:grid-cols-[240px_minmax(0,1fr)] @[52rem]/settings:items-start @[52rem]/settings:gap-x-8 @[52rem]/settings:gap-y-3 @[52rem]/settings:border-t-[0.5px] @[52rem]/settings:border-border @[52rem]/settings:pt-6 @[52rem]/settings:first:border-t-0 @[52rem]/settings:first:pt-0'

interface SettingsGroupProps extends Omit<React.HTMLAttributes<HTMLElement>, 'title'> {
  /** Group heading, sentence case. */
  title?: React.ReactNode
  /** Concise helper copy rendered under the heading. */
  description?: React.ReactNode
  /** Quiet text after the heading: a count or a summary ("4 servers · 3 on"). */
  meta?: React.ReactNode
  /** Controls that act on the whole group (refresh, add), at the right of the heading. */
  actions?: React.ReactNode
  /** Short note under the group. */
  footnote?: React.ReactNode
  /**
   * `'split'` (default) places the heading column on the left on wide settings
   * pages; `'stacked'` keeps full-width content (e.g. multi-column card grids).
   */
  layout?: SettingsGroupLayout
}

/**
 * A titled section of settings on a page: a 14/20 heading with optional helper
 * text, meta and actions, followed by the content. On wide settings pages
 * (`@container/settings >= 52rem`), titled `layout="split"` groups place the
 * heading column on the left (240px) and cards/notices on the right. Put rows
 * inside a {@link SettingsCard}; anything else (a filter, a notice, a list)
 * can sit alongside them.
 */
export const SettingsGroup = forwardRef<HTMLElement, SettingsGroupProps>(function SettingsGroup(
  { title, description, meta, actions, footnote, layout = 'split', className, children, ...props },
  ref
) {
  const titleId = useId()
  const canSplit = layout === 'split' && Boolean(title)

  if (canSplit) {
    return (
      <section
        ref={ref}
        aria-labelledby={titleId}
        className={cn(SETTINGS_SPLIT_GROUP_CLASS, className)}
        {...props}
      >
        <div className="flex min-h-7 flex-wrap items-center justify-between gap-x-3 gap-y-2 @[52rem]/settings:min-h-0 @[52rem]/settings:flex-col @[52rem]/settings:items-start @[52rem]/settings:justify-start @[52rem]/settings:gap-1.5 @[52rem]/settings:pt-1">
          <div className="flex min-w-0 flex-col gap-1 @[52rem]/settings:gap-1.5">
            <div className="flex min-w-0 flex-wrap items-baseline gap-x-2 gap-y-0.5">
              <h3 id={titleId} className="text-sm font-medium text-foreground">
                {title}
              </h3>
              {meta && <span className="text-xs tabular-nums text-muted-foreground">{meta}</span>}
            </div>
            {description && (
              <p className="text-xs leading-relaxed text-muted-foreground">{description}</p>
            )}
          </div>
          {actions && (
            <div className="flex flex-wrap items-center gap-2 @[52rem]/settings:pt-1.5">
              {actions}
            </div>
          )}
        </div>
        <div className="flex min-w-0 flex-col gap-3">
          {children}
          {footnote && <p className="text-xs leading-relaxed text-muted-foreground">{footnote}</p>}
        </div>
      </section>
    )
  }

  return (
    <section
      ref={ref}
      aria-labelledby={title ? titleId : undefined}
      className={cn('flex flex-col gap-3', className)}
      {...props}
    >
      {(title || description || actions) && (
        <div className="flex min-h-7 flex-wrap items-center justify-between gap-x-3 gap-y-1">
          {(title || description) && (
            <div className="flex min-w-0 flex-col gap-1">
              {title && (
                <div className="flex min-w-0 flex-wrap items-baseline gap-x-2 gap-y-0.5">
                  <h3 id={titleId} className="text-sm font-medium text-foreground">
                    {title}
                  </h3>
                  {meta && (
                    <span className="truncate text-xs tabular-nums text-muted-foreground">
                      {meta}
                    </span>
                  )}
                </div>
              )}
              {description && (
                <p className="text-xs leading-relaxed text-muted-foreground">{description}</p>
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
