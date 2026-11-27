import { useId, useMemo } from 'react'
import { RefreshCw } from '@/components/icons'
import { Button } from '@/components/ui/button'
import { Spinner } from '@/components/ui/spinner'
import { useAgentIconCatalog } from '@/lib/agents/agent-icon-catalog'
import { sanitizeInlineAgentSvg } from '@/lib/agents/sanitize-agent-icon'
import { formatUsageChecked } from '@/lib/claude-usage-format'
import { cn } from '@/lib/utils'
import { SETTINGS_SPLIT_GROUP_CLASS } from '../SettingsGroup'

export interface ProviderGroupHeaderProps {
  /** Bundled agent icon key, e.g. `acp:claude-acp`. */
  iconKey: string
  name: string
  /** Quiet text after the name, e.g. "2 accounts". */
  count?: string
  /** Oldest usage read in the group, epoch ms. */
  lastChecked: number | null
  refreshing: boolean
  /** Omit to hide the refresh control (nothing to refresh). */
  onRefresh?: () => void
  /** Extra action at the trailing edge (e.g. "Add account"). */
  action?: React.ReactNode
}

export interface ProviderGroupProps extends ProviderGroupHeaderProps {
  /** Optional concise helper copy shown under the provider heading. */
  description?: React.ReactNode
  children: React.ReactNode
  className?: string
  'data-testid'?: string
}

function useSanitizedAgentIcon(iconKey: string): string | null {
  const catalog = useAgentIconCatalog()
  return useMemo(() => {
    const entry = catalog.byKey(iconKey)
    return entry ? sanitizeInlineAgentSvg(entry.svg) : null
  }, [catalog, iconKey])
}

/** The heading of one provider's account group: brand mark, name, usage refresh. */
export function ProviderGroupHeader({
  iconKey,
  name,
  count,
  lastChecked,
  refreshing,
  onRefresh,
  action
}: ProviderGroupHeaderProps): React.JSX.Element {
  const svg = useSanitizedAgentIcon(iconKey)

  return (
    <div className="flex min-h-7 flex-wrap items-center gap-x-2 gap-y-2">
      {svg && (
        <span
          aria-hidden="true"
          className="inline-flex size-5 shrink-0 items-center justify-center rounded-md bg-foreground/[0.06] text-foreground/85 [&_svg]:size-3.5"
          // biome-ignore lint/security/noDangerouslySetInnerHtml: bundled icon SVG sanitized via sanitizeInlineAgentSvg (DOMPurify)
          dangerouslySetInnerHTML={{ __html: svg }}
        />
      )}
      <h3 className="text-sm font-medium leading-5 text-foreground">{name}</h3>
      {count && <span className="text-xs tabular-nums text-muted-foreground">{count}</span>}
      <span
        className="ml-auto flex items-center gap-1.5 text-2xs text-muted-foreground"
        aria-live="polite"
      >
        {refreshing ? (
          <>
            <Spinner size={11} decorative />
            Updating…
          </>
        ) : lastChecked ? (
          formatUsageChecked(lastChecked)
        ) : null}
      </span>
      {onRefresh && (
        <Button
          type="button"
          size="icon-xs"
          variant="ghost"
          onClick={onRefresh}
          disabled={refreshing}
          aria-label={`Refresh ${name} usage`}
          className="text-muted-foreground"
        >
          <RefreshCw size={13} />
        </Button>
      )}
      {action}
    </div>
  )
}

/**
 * Split-section wrapper for one provider's account group. Below `52rem`
 * (or outside `@container/settings`), renders a horizontal header above the
 * account list; at `52rem`, stacks the provider identity, usage check and
 * primary action in the 240px left column beside the account cards.
 */
export function ProviderGroup({
  iconKey,
  name,
  count,
  description,
  lastChecked,
  refreshing,
  onRefresh,
  action,
  children,
  className,
  'data-testid': testId
}: ProviderGroupProps): React.JSX.Element {
  const titleId = useId()
  const svg = useSanitizedAgentIcon(iconKey)
  const hasUsageStatus = Boolean(refreshing || lastChecked || onRefresh)

  return (
    <section
      aria-labelledby={titleId}
      data-testid={testId}
      className={cn(SETTINGS_SPLIT_GROUP_CLASS, className)}
    >
      <div className="flex min-h-7 flex-wrap items-center gap-x-2 gap-y-2 @[52rem]/settings:min-h-0 @[52rem]/settings:flex-col @[52rem]/settings:items-start @[52rem]/settings:gap-1.5 @[52rem]/settings:pt-1">
        <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-0.5">
          {svg && (
            <span
              aria-hidden="true"
              className="inline-flex size-5 shrink-0 items-center justify-center rounded-md bg-foreground/[0.06] text-foreground/85 [&_svg]:size-3.5"
              // biome-ignore lint/security/noDangerouslySetInnerHtml: bundled icon SVG sanitized via sanitizeInlineAgentSvg (DOMPurify)
              dangerouslySetInnerHTML={{ __html: svg }}
            />
          )}
          <h3 id={titleId} className="text-sm font-medium leading-5 text-foreground">
            {name}
          </h3>
          {count && <span className="text-xs tabular-nums text-muted-foreground">{count}</span>}
        </div>
        {description && (
          <p className="order-last w-full text-xs leading-relaxed text-muted-foreground @[52rem]/settings:order-none @[52rem]/settings:w-auto">
            {description}
          </p>
        )}
        {hasUsageStatus && (
          <div className="ml-auto flex items-center gap-1.5 @[52rem]/settings:ml-0">
            <span
              className="flex items-center gap-1.5 text-2xs text-muted-foreground"
              aria-live="polite"
            >
              {refreshing ? (
                <>
                  <Spinner size={11} decorative />
                  Updating…
                </>
              ) : lastChecked ? (
                formatUsageChecked(lastChecked)
              ) : null}
            </span>
            {onRefresh && (
              <Button
                type="button"
                size="icon-xs"
                variant="ghost"
                onClick={onRefresh}
                disabled={refreshing}
                aria-label={`Refresh ${name} usage`}
                className="text-muted-foreground"
              >
                <RefreshCw size={13} />
              </Button>
            )}
          </div>
        )}
        {action && (
          <div
            className={cn(
              'flex flex-wrap items-center gap-2 @[52rem]/settings:pt-1.5',
              !hasUsageStatus && 'ml-auto @[52rem]/settings:ml-0'
            )}
          >
            {action}
          </div>
        )}
      </div>
      <div className="flex min-w-0 flex-col gap-3">{children}</div>
    </section>
  )
}

/** Oldest usage read, so "Updated …" never claims fresher data than it shows. */
export function oldestCheck(usage: Record<string, { checkedAt: number }>): number | null {
  const times = Object.values(usage).map((read) => read.checkedAt)
  return times.length > 0 ? Math.min(...times) : null
}
