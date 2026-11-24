// The ACP store's MCP scenarios for `acp.ts`. Each restates a case of the
// `mcp-*.test.ts` suites. An OAuth flow left in flight keeps its server id in
// the store's module map, so each such flow uses an id no other vector does.
import type { Scenario } from '../record'
import { ev } from './acp-fixtures'

const http = (id: string, extra: Record<string, unknown> = {}) => ({
  id,
  type: 'http',
  name: `srv-${id}`,
  url: `https://${id}.example/mcp`,
  headers: [],
  enabled: true,
  ...extra
})

const stdio = (id: string, extra: Record<string, unknown> = {}) => ({
  id,
  type: 'stdio',
  name: `cmd-${id}`,
  command: 'npx',
  args: ['server'],
  env: [],
  enabled: true,
  ...extra
})

const connected = { status: 'connected', tools: [{ name: 'search' }] }
const disconnected = { status: 'disconnected', tools: [], error: 'refused' }

const loaded = (...servers: unknown[]) => ({ mcpServers: servers, mcpServersLoaded: true })

const provenance = { name: 'io.github/server', version: '1.2.0' }

function catalogResult(transport: string, extra: Record<string, unknown> = {}) {
  return {
    name: 'github',
    config:
      transport === 'stdio'
        ? { transport, command: 'uvx', args: ['gh'], env: [{ name: 'TOKEN' }] }
        : { transport, url: 'https://gh.example/mcp', headers: [{ name: 'Authorization' }] },
    secrets: [
      { slot: 'env', name: 'TOKEN', value: 't0k' },
      { slot: 'header', name: 'Authorization', value: 'Bearer x' }
    ],
    provenance,
    launchDigest: '0123456789abcdef0123',
    ...extra
  }
}

const installRequest = (requestId: string) => ({
  agentId: 'agent-1',
  sessionId: 's1',
  requestId,
  name: 'github',
  title: 'GitHub',
  transport: 'http',
  form: { fields: [] },
  consent: null,
  planId: 'plan-1',
  launchDigest: 'digest-1'
})

export const mcpScenarios: Scenario[] = [
  {
    name: 'mcp_load_sets_the_list_and_starts_the_shared_watch',
    context: { mcpReplies: [[http('a'), stdio('b')], connected] },
    steps: [
      ev('loadMcpServers'),
      ev('mcpSharedChanged', {
        exists: true,
        data: [http('a', { url: 'https://moved.example/mcp' }), stdio('b'), http('c')]
      }),
      ev('mcpSharedChanged', { exists: false }),
      ev('mcpSharedChanged', { exists: true, data: [http('z')] }),
      ev('mcpSharedChanged', { exists: false, data: [http('z')] })
    ]
  },
  {
    name: 'mcp_load_failure_logs_and_toasts',
    context: { mcpReplies: [{ reject: 'disk gone' }] },
    steps: [ev('loadMcpServers')]
  },
  {
    name: 'mcp_save_adds_or_replaces_and_reprobes_an_edited_server',
    context: { mcpReplies: [null, null, null, connected, null] },
    initial: {
      ...loaded(http('a'), stdio('b', { enabled: false })),
      mcpProbeStatus: { a: 'connected', b: 'disconnected' },
      mcpTools: { a: [{ name: 'search' }] },
      mcpToolsLoaded: { a: true },
      mcpProbeError: { b: 'refused' }
    },
    steps: [
      ev('saveMcpServer', { id: 'c', type: 'http', name: 'srv-c', url: 'https://c.example' }),
      ev('saveMcpServer', http('a', { enabled: false })),
      ev('saveMcpServer', http('a', { url: 'https://moved.example/mcp' })),
      ev('saveMcpServer', stdio('b', { enabled: false, args: ['other'] }))
    ]
  },
  {
    name: 'mcp_registry_writes_roll_back_when_the_save_fails',
    context: {
      mcpReplies: [{ reject: 'disk full' }, null, { reject: 'disk full' }, null, { reject: 'no' }]
    },
    initial: loaded(http('a')),
    steps: [
      ev('saveMcpServer', http('c')),
      ev('importMcpServers', [http('d'), stdio('e')]),
      ev('importMcpServers', []),
      ev('importMcpServers', [http('f')]),
      ev('setMcpServerEnabled', 'a', false),
      ev('setMcpServerEnabled', 'a', true)
    ]
  },
  {
    name: 'mcp_catalog_install_names_and_saves_the_entry',
    context: {
      mcpReplies: [[stdio('g', { name: 'github' })], null, null, { reject: 'disk full' }]
    },
    steps: [
      ev('saveCatalogInstall', catalogResult('stdio')),
      ev('loadMcpServers'),
      ev('saveCatalogInstall', catalogResult('stdio')),
      ev('saveCatalogInstall', catalogResult('sse', { name: 'github' })),
      ev('saveCatalogInstall', catalogResult('http', { provenance: null })),
      ev('saveCatalogInstall', catalogResult('http', { secrets: [] })),
      ev('saveCatalogInstall', catalogResult('http', { name: 'other' }))
    ]
  },
  {
    name: 'mcp_delete_clears_the_server_state',
    context: { mcpReplies: [null, { reject: 'disk full' }] },
    initial: {
      ...loaded(http('a'), http('b')),
      mcpProbeStatus: { a: 'connected', b: 'connected' },
      mcpTools: { a: [{ name: 'search' }] },
      mcpToolsLoaded: { a: true },
      mcpProbing: { a: false },
      mcpOAuthConnected: { a: true },
      mcpOAuthConnecting: { a: false }
    },
    steps: [ev('deleteMcpServer', 'a'), ev('deleteMcpServer', 'b')]
  },
  {
    name: 'mcp_probe_records_the_result',
    context: {
      mcpReplies: [
        { ...connected, error: 'stale' },
        disconnected,
        { reject: 'spawn failed' },
        disconnected
      ]
    },
    initial: {
      ...loaded(http('a'), http('b'), stdio('c'), http('d')),
      mcpProbing: { d: true },
      mcpTools: { c: [{ name: 'old' }] }
    },
    steps: [
      ev('probeMcpServer', 'a'),
      ev('probeMcpServer', 'b'),
      ev('probeMcpServer', 'c'),
      ev('loadMcpTools', 'a'),
      ev('loadMcpTools', 'b'),
      ev('probeMcpServer', 'missing'),
      ev('probeMcpServer', 'd'),
      ev('loadMcpTools', 'd')
    ]
  },
  {
    name: 'mcp_oauth_connect_confirms_the_token_and_probes',
    context: { mcpReplies: [null, connected, null] },
    initial: { ...loaded(http('a'), stdio('b'), http('q')), mcpProbing: { q: true } },
    steps: [
      ev('connectMcpOAuth', 'p1', 'a'),
      ev('connectMcpOAuth', 'p1', 'b'),
      ev('connectMcpOAuth', 'p1', 'missing'),
      ev('connectMcpOAuth', 'p1', 'q')
    ]
  },
  {
    name: 'mcp_oauth_connect_failure_rejects_and_a_cancel_does_not',
    context: {
      mcpReplies: [
        { reject: 'boom', code: 'OAUTH_FAILED' },
        { reject: 'stopped', code: 'OAUTH_CANCELLED' }
      ]
    },
    initial: loaded(http('g'), http('h')),
    steps: [ev('connectMcpOAuth', 'p1', 'g'), ev('connectMcpOAuth', 'p1', 'h')]
  },
  {
    name: 'mcp_oauth_paste_back_and_a_cancel_of_the_held_flow',
    context: {
      mcpReplies: [
        { authUrl: 'https://auth.example/k', redirectUri: 'https://cb.example', hold: true },
        null,
        { hold: true },
        null,
        connected
      ]
    },
    initial: loaded(http('k'), http('m')),
    steps: [
      ev('connectMcpOAuth', 'p1', 'k'),
      ev('completeMcpOAuthPaste', 'k', 'https://cb.example/?code=1'),
      ev('cancelMcpOAuth', 'k'),
      ev('cancelMcpOAuth', 'k'),
      ev('connectMcpOAuth', 'p1', 'm'),
      ev('connectMcpOAuth', 'p1', 'm')
    ]
  },
  {
    name: 'mcp_oauth_paste_back_of_an_unknown_state_checks_the_token',
    context: {
      mcpReplies: [
        { authUrl: 'https://auth.example/n', redirectUri: 'https://cb.example', hold: true },
        { reject: 'gone', code: 'OAUTH_UNKNOWN_STATE' },
        true,
        { authUrl: 'https://auth.example/o', redirectUri: 'https://cb.example', hold: true },
        { reject: 'gone', code: 'OAUTH_UNKNOWN_STATE' },
        false,
        { authUrl: 'https://auth.example/r', redirectUri: 'https://cb.example', hold: true },
        { reject: 'bad code', code: 'OAUTH_INVALID' }
      ]
    },
    initial: loaded(http('n'), http('o'), http('r')),
    steps: [
      ev('connectMcpOAuth', 'p1', 'n'),
      ev('completeMcpOAuthPaste', 'n', 'https://cb.example/?code=1'),
      ev('connectMcpOAuth', 'p1', 'o'),
      ev('completeMcpOAuthPaste', 'o', 'https://cb.example/?code=2'),
      ev('connectMcpOAuth', 'p1', 'r'),
      ev('completeMcpOAuthPaste', 'r', 'https://cb.example/?code=3'),
      ev('completeMcpOAuthPaste', 'none', 'https://cb.example/?code=4')
    ]
  },
  {
    name: 'mcp_oauth_status_and_disconnect',
    context: {
      mcpReplies: [true, { reject: 'offline' }, null, disconnected, { reject: 'refused' }]
    },
    initial: loaded(http('a'), stdio('b')),
    steps: [
      ev('checkMcpOAuthStatus', 'p1', 'a'),
      ev('checkMcpOAuthStatus', 'p1', 'a'),
      ev('checkMcpOAuthStatus', 'p1', 'b'),
      ev('disconnectMcpOAuth', 'p1', 'a'),
      ev('disconnectMcpOAuth', 'p1', 'a')
    ]
  },
  {
    name: 'mcp_install_requests_answers_and_outcomes',
    context: { mcpReplies: [null, { reject: 'host gone' }, { reject: 'busy' }] },
    steps: [
      ev('_onMcpInstallRequest', installRequest('r1')),
      ev('answerMcpInstall', 'r1', 'approve', { repo: 'x' }, { token: 'y' }),
      ev('_onMcpInstallResolved', {
        agentId: 'agent-1',
        sessionId: 's1',
        requestId: 'r1',
        outcome: 'installed',
        name: 'github',
        serverId: 'srv-1',
        nextSteps: [{ kind: 'oauth', message: 'Sign in' }],
        oauth: true
      }),
      ev('_onMcpInstallRequest', installRequest('r1')),
      ev('answerMcpInstall', 'r1', 'approve'),
      ev('_onMcpInstallRequest', { ...installRequest('r2'), error: 'bad input' }),
      ev('answerMcpInstall', 'r2', 'decline', undefined, undefined, 'plan-2'),
      ev('_onMcpInstallResolved', {
        agentId: 'agent-1',
        sessionId: 's1',
        requestId: 'r2',
        outcome: 'declined',
        nextSteps: [],
        oauth: false
      }),
      ev('_onMcpInstallResolved', {
        requestId: 'r9',
        outcome: 'installed',
        nextSteps: [],
        oauth: false
      }),
      ev('_onMcpInstallRequest', installRequest('r4')),
      ev('answerMcpInstall', 'r4', 'approve'),
      ev('_onMcpInstallRequest', installRequest('r3')),
      ev('dismissMcpInstall', 'r3'),
      ev('dismissMcpInstall', 'r3')
    ]
  },
  {
    name: 'mcp_registry_change_reloads_the_list',
    context: { mcpReplies: [[http('a')]] },
    steps: [
      ev('_onMcpRegistryChanged', {
        agentId: 'agent-1',
        sessionId: 's1',
        serverId: 'a',
        name: 'srv-a'
      })
    ]
  }
]
