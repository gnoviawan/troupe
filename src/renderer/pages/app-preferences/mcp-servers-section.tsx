import { useState } from 'react'
import { McpCatalogBrowser } from '@/components/settings/McpCatalogBrowser'
import { McpServersSettings } from '@/components/settings/McpServersSettings'
import { SettingsSection } from '@/components/settings/SettingsLayout'

/**
 * Settings → MCP servers: the servers you added as cards, then the catalog.
 * A card opens that server's detail page in place; the catalog stays mounted
 * under it so its search and scroll survive the round trip.
 */
export function McpServersSection(): React.JSX.Element {
  const [detailId, setDetailId] = useState<string | null>(null)
  return (
    <SettingsSection id="mcp-servers">
      <McpServersSettings detailId={detailId} onDetailChange={setDetailId} />
      <div hidden={detailId !== null} className="flex flex-col">
        <McpCatalogBrowser />
      </div>
    </SettingsSection>
  )
}
