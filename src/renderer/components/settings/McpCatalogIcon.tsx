import type { McpCatalogIconUrl } from '@shared/types/mcp-catalog.types'
import { useEffect, useState } from 'react'
import { mcpBrandLogo } from '@/lib/mcp-brand-logos'
import { mcpCatalogApi } from '@/lib/mcp-catalog-api'
import { cn } from '@/lib/utils'

function useMcpCatalogIconUrl(name: string, hasIcon: boolean): McpCatalogIconUrl | null {
  const [loaded, setLoaded] = useState<{ name: string; url: McpCatalogIconUrl } | null>(null)

  useEffect(() => {
    if (!hasIcon) return
    let cancelled = false
    void mcpCatalogApi.icon(name).then((result) => {
      if (!cancelled && result.success) {
        setLoaded({ name, url: result.data })
      }
    })
    return () => {
      cancelled = true
    }
  }, [name, hasIcon])

  return hasIcon && loaded?.name === name ? loaded.url : null
}

/**
 * An MCP server's logo in a tile: the registry's icon when it has one, else a
 * bundled brand logo for a well-known server, else the name's first letter.
 * `size` is the tile: 32 on a card, 40 on the detail page.
 */
export function McpCatalogIcon({
  name,
  displayName,
  hasIcon,
  size = 32
}: {
  /** Registry name, or `null` for a server added by hand. */
  name: string | null
  displayName: string
  hasIcon: boolean
  size?: 32 | 40
}): React.JSX.Element {
  const url = useMcpCatalogIconUrl(name ?? '', hasIcon && name !== null)
  const [brokenUrl, setBrokenUrl] = useState<string | null>(null)
  const tile = size === 40 ? 'size-10' : 'size-8'

  if (url && url !== brokenUrl) {
    return (
      <img
        src={url}
        alt=""
        width={size}
        height={size}
        draggable={false}
        onError={() => setBrokenUrl(url)}
        className={cn(tile, 'shrink-0 rounded-md object-contain')}
      />
    )
  }
  const brand = name ? mcpBrandLogo(name) : null
  if (brand) {
    const mark = size === 40 ? 22 : 18
    return (
      <span
        aria-hidden
        data-testid="mcp-brand-logo"
        className={cn(
          tile,
          'flex shrink-0 items-center justify-center rounded-md bg-foreground/[0.06] text-foreground'
        )}
      >
        <svg width={mark} height={mark} viewBox={brand.viewBox} aria-hidden role="presentation">
          {brand.paths.map((d) => (
            <path key={d} d={d} fill={brand.fill} />
          ))}
        </svg>
      </span>
    )
  }
  return (
    <span
      aria-hidden
      data-testid="mcp-catalog-icon-placeholder"
      className={cn(
        tile,
        'flex shrink-0 select-none items-center justify-center rounded-md bg-foreground/[0.06] font-medium text-secondary-foreground',
        size === 40 ? 'text-base' : 'text-sm'
      )}
    >
      {Array.from(displayName.trim())[0]?.toUpperCase() ?? '?'}
    </span>
  )
}
