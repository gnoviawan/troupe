import type { StatusTone } from '@/components/settings/StatusWord'
import type { ProbeStatus } from '@/lib/acp-api'
import { type StoredMcpServer, transportOf } from '@/lib/acp-mcp-persistence'

export interface McpServerStatus {
  tone: StatusTone
  label: string
  /** Checking or connecting: the word shows a spinner. */
  pending: boolean
}

/** The command (stdio) or URL (http, sse) a server runs or calls. */
export function mcpServerTarget(server: StoredMcpServer): string {
  return transportOf(server) === 'stdio'
    ? (server as Extract<StoredMcpServer, { type?: 'stdio' }>).command
    : (server as Extract<StoredMcpServer, { type: 'http' | 'sse' }>).url
}

/** True for servers reached over the network, which can use OAuth. */
export function mcpServerIsRemote(server: StoredMcpServer): boolean {
  const transport = transportOf(server)
  return transport === 'http' || transport === 'sse'
}

/**
 * One word for a server's state. Only problems get a colour: Needs sign-in
 * (warning) and Unreachable (danger); Connected is a green dot, muted text.
 */
export function mcpServerStatus({
  enabled,
  probing,
  probeStatus
}: {
  enabled: boolean
  probing: boolean
  probeStatus: ProbeStatus | undefined
}): McpServerStatus {
  if (!enabled) return { tone: 'muted', label: 'Off', pending: false }
  if (probing) return { tone: 'muted', label: 'Checking…', pending: true }
  if (probeStatus === 'connected') return { tone: 'success', label: 'Connected', pending: false }
  if (probeStatus === 'disconnected') {
    return { tone: 'danger', label: 'Unreachable', pending: false }
  }
  if (probeStatus === 'authRequired') {
    return { tone: 'warning', label: 'Needs sign-in', pending: false }
  }
  return { tone: 'muted', label: 'Not checked', pending: false }
}

/** "41 tools", "1 tool", or null before the tool list loads. */
export function mcpToolCount(tools: readonly unknown[] | undefined): string | null {
  if (!tools) return null
  return tools.length === 1 ? '1 tool' : `${tools.length} tools`
}
