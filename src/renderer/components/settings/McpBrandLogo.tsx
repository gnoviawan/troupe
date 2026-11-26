import type { McpBrandLogo as BrandLogo } from '@/lib/mcp-brand-logos'
import { cn } from '@/lib/utils'

/**
 * A bundled brand mark in a logo tile. The paths are our own (see
 * `mcp-brand-logos.ts`); registry icons never come through here, they stay
 * an `<img>` in `McpCatalogIcon`.
 */
export function McpBrandLogo({
  logo,
  size
}: {
  logo: BrandLogo
  size: 32 | 40
}): React.JSX.Element {
  const mark = size === 40 ? 22 : 18
  return (
    <span
      aria-hidden
      data-testid="mcp-brand-logo"
      className={cn(
        size === 40 ? 'size-10' : 'size-8',
        'flex shrink-0 items-center justify-center rounded-md bg-foreground/[0.06] text-foreground'
      )}
    >
      <svg width={mark} height={mark} viewBox={logo.viewBox} aria-hidden role="presentation">
        {logo.paths.map((d) => (
          <path key={d} d={d} fill={logo.fill} />
        ))}
      </svg>
    </span>
  )
}
