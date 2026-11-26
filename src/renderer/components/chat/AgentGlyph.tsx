import { useId, useMemo } from 'react'
import { Bot } from '@/components/icons'
import {
  agentBrandProps,
  brandGradientId,
  useAgentIconCatalog,
  withBrandGradient
} from '@/lib/agents/agent-icon-catalog'
import { sanitizeInlineAgentSvg } from '@/lib/agents/sanitize-agent-icon'
import { cn } from '@/lib/utils'

/**
 * Renders an ACP agent's icon (inline SVG via `currentColor`). An agent with
 * a registry `brandColor` is drawn in it (per theme, overriding a caller's
 * text colour); the rest keep `text-foreground/80` unless the caller sets one.
 * Resolution precedence: `icon` prop (persisted custom SVG, bundled or
 * uploaded) → `catalog.byKey(`acp:${templateId}`)` (overlay, then bundled) →
 * lucide `<Bot>` fallback.
 *
 * Used by every chat-side surface (AgentBadge, ChatHistoryEntryRow,
 * ChatEmptyState, ChatInputBar) — the single chokepoint for
 * ACP agent icons in the chat UI.
 */
export function AgentGlyph({
  templateId,
  icon,
  size = 14,
  className
}: {
  templateId: string | null
  /** Persisted custom icon SVG (bundled or uploaded). Takes precedence. */
  icon?: string | null
  size?: number
  className?: string
}): React.JSX.Element {
  const catalog = useAgentIconCatalog()
  const normalized = useMemo(() => {
    if (icon) {
      const sanitized = sanitizeInlineAgentSvg(icon)
      if (sanitized) return sanitized
    }
    if (!templateId) return null
    const svg = catalog.byKey(`acp:${templateId}`)?.svg
    return svg ? sanitizeInlineAgentSvg(svg) : null
  }, [catalog, icon, templateId])
  const gradientId = brandGradientId(useId())
  const brand = normalized ? catalog.brandColor(icon, templateId) : null
  const brandProps = brand ? agentBrandProps(brand, gradientId) : null

  if (normalized) {
    return (
      <span
        aria-hidden="true"
        data-agent-brand={brand ? '' : undefined}
        style={{ width: size, height: size, ...brandProps?.style }}
        className={cn(
          'inline-flex shrink-0 text-foreground/80 [&_svg]:h-full [&_svg]:w-full',
          className,
          brandProps?.className
        )}
        // biome-ignore lint/security/noDangerouslySetInnerHtml: icon SVG is sanitized via sanitizeInlineAgentSvg (DOMPurify)
        dangerouslySetInnerHTML={{ __html: withBrandGradient(normalized, brand, gradientId) }}
      />
    )
  }
  return <Bot size={size} className={cn('shrink-0 text-foreground/80', className)} />
}
