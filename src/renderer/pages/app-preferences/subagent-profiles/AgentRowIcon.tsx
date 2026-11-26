import { useAgentIconCatalog } from '@/lib/agents/agent-icon-catalog'
import { sanitizeInlineAgentSvg } from '@/lib/agents/sanitize-agent-icon'
import { useAgentIcon, useAgentTemplateId } from '@/stores/acp-store'

/**
 * The launchable agent's glyph: its persisted config's custom icon, else the
 * bundled icon of the config's templateId, else its initial. Same resolution
 * path the chat surfaces use (config -> templateId -> catalog).
 */
export function AgentRowIcon({
  agentId,
  name
}: {
  agentId: string
  name: string
}): React.JSX.Element {
  const templateId = useAgentTemplateId(null, agentId)
  const customIcon = useAgentIcon(null, agentId)
  const catalog = useAgentIconCatalog()
  const svg = customIcon ?? (templateId ? catalog.byKey(`acp:${templateId}`)?.svg : null)
  const sanitized = svg ? sanitizeInlineAgentSvg(svg) : null
  if (sanitized) {
    return (
      <span
        aria-hidden="true"
        className="inline-flex size-3.5 shrink-0 text-foreground/80 [&_svg]:h-full [&_svg]:w-full"
        // biome-ignore lint/security/noDangerouslySetInnerHtml: icon SVG is sanitized via sanitizeInlineAgentSvg (DOMPurify)
        dangerouslySetInnerHTML={{ __html: sanitized }}
      />
    )
  }
  return (
    <span
      aria-hidden="true"
      className="flex size-3.5 shrink-0 items-center justify-center text-3xs font-semibold uppercase text-muted-foreground"
    >
      {name.charAt(0)}
    </span>
  )
}
