import {
  mcpServerStatus,
  mcpServerTarget,
  mcpToolCount
} from '../../../../src/renderer/components/settings/mcp-server-view'
import {
  initialInstallValues,
  splitInstallValues
} from '../../../../src/renderer/components/settings/McpInstallForm'
import { canStartOver, type McpInstallFailure } from '../../../../src/renderer/hooks/use-mcp-catalog-install'
import {
  normalizeMcpRegistry,
  type StoredMcpServer,
  sameLaunch,
  validateMcpServer
} from '../../../../src/renderer/lib/acp-mcp-persistence'
import {
  MCP_CATALOG_STATE_LABEL,
  mcpCatalogReasonText,
  mcpCatalogSampleText,
  mcpCatalogSyncErrorText,
  mcpCatalogSyncText,
  mcpCatalogUpdatedText
} from '../../../../src/renderer/lib/mcp-catalog-copy'
import {
  blankRow,
  draftFromServer,
  draftToConfig,
  draftToUncheckedConfig,
  joinCommandLine,
  type KeyValueRow,
  liftEnvPrefix,
  type McpServerDraft,
  rowProblem,
  tokenizeCommandLine,
  transportForUrl,
  withAuth,
  withUrl
} from '../../../../src/renderer/lib/mcp-server-draft'
import { parseMcpJsonImport } from '../../../../src/renderer/lib/mcp-json-import'
import type { McpInstallField } from '../../../../src/shared/types/mcp-install.types'
import type {
  McpCatalogReason,
  McpCatalogState,
  McpCatalogStatus
} from '../../../../src/shared/types/mcp-catalog.types'
import type { Scenario, StoreRecorder, VectorEvent } from '../record'

type Row = { name: string; value: string }
type RawDraft = Omit<McpServerDraft, 'remote' | 'local'> & {
  remote: Omit<McpServerDraft['remote'], 'headers'> & { headers: Row[] }
  local: { commandLine: string; env: Row[] }
}

const withIds = (rows: Row[]): KeyValueRow[] => rows.map((row) => blankRow(row.name, row.value))
const withoutIds = (rows: KeyValueRow[]): Row[] =>
  rows.map((row) => ({ name: row.name, value: row.value }))
const toDraft = (raw: RawDraft): McpServerDraft => ({
  ...raw,
  remote: { ...raw.remote, headers: withIds(raw.remote.headers) },
  local: { ...raw.local, env: withIds(raw.local.env) }
})
const fromDraft = (draft: McpServerDraft): RawDraft => ({
  ...draft,
  remote: { ...draft.remote, headers: withoutIds(draft.remote.headers) },
  local: { ...draft.local, env: withoutIds(draft.local.env) }
})

/**
 * The MCP servers page's pure logic: the registry read, validation, the Add and Edit form's
 * draft and its command line, and the status word. `tests/settings` replays each as a read
 * against `model::settings::mcp`. Form rows travel without their React keys.
 */
export const mcpServersRecorder: StoreRecorder = {
  store: 'mcp-servers',
  sources: [
    'src/renderer/lib/acp-mcp-persistence.ts',
    'src/renderer/lib/mcp-server-draft.ts',
    'src/renderer/components/settings/mcp-server-view.ts',
    'src/renderer/lib/mcp-catalog-copy.ts',
    'src/renderer/lib/mcp-json-import.ts',
    'src/renderer/components/settings/McpInstallForm.tsx',
    'src/renderer/hooks/use-mcp-catalog-install.ts'
  ],
  reset() {},
  seed() {},
  snapshot() {
    return null
  },
  dispatch({ action, args }) {
    const draft = () => toDraft(args[0] as RawDraft)
    switch (action) {
      case 'normalize':
        return normalizeMcpRegistry(args[0])
      case 'validate':
        return validateMcpServer(args[0] as StoredMcpServer).issues
      case 'sameLaunch':
        return sameLaunch(args[0] as StoredMcpServer, args[1] as StoredMcpServer)
      case 'tokenize':
        return tokenizeCommandLine(args[0] as string)
      case 'join':
        return joinCommandLine(args[0] as string[])
      case 'transportForUrl':
        return transportForUrl(args[0] as string)
      case 'liftEnvPrefix': {
        const local = liftEnvPrefix(draft().local)
        return { commandLine: local.commandLine, env: withoutIds(local.env) }
      }
      case 'rowProblem':
        return rowProblem(blankRow(args[0] as string, args[1] as string))
      case 'toConfig':
        return draftToConfig(draft())
      case 'uncheckedConfig':
        return draftToUncheckedConfig(draft())
      case 'fromServer':
        return fromDraft(draftFromServer(args[0] as StoredMcpServer))
      case 'withUrl':
        return fromDraft({ ...draft(), remote: withUrl(draft().remote, args[1] as string) })
      case 'withAuth':
        return fromDraft({
          ...draft(),
          remote: withAuth(draft().remote, args[1] as 'browser' | 'token')
        })
      case 'status':
        return mcpServerStatus({
          enabled: args[0] as boolean,
          probing: args[1] as boolean,
          probeStatus: (args[2] ?? undefined) as 'connected' | undefined
        })
      case 'target':
        return mcpServerTarget(args[0] as StoredMcpServer)
      case 'toolCount':
        return mcpToolCount(Array.from({ length: args[0] as number }))
      case 'reasonText':
        return mcpCatalogReasonText(args[0] as McpCatalogReason)
      case 'stateLabel':
        return MCP_CATALOG_STATE_LABEL[args[0] as McpCatalogState]
      case 'sampleText':
        return mcpCatalogSampleText(args[0] as McpCatalogStatus)
      case 'syncText':
        return mcpCatalogSyncText(args[0] as McpCatalogStatus)
      case 'syncErrorText':
        return mcpCatalogSyncErrorText(args[0] as string)
      case 'updatedText':
        return mcpCatalogUpdatedText(null)
      case 'jsonImport':
        return parseMcpJsonImport(args[0] as string)
      case 'installInitial':
        return initialInstallValues(args[0] as McpInstallField[])
      case 'installSplit':
        return splitInstallValues(args[0] as McpInstallField[], args[1] as Record<string, string>)
      case 'canStartOver':
        return canStartOver(args[0] as McpInstallFailure)
      default:
        throw new Error(`mcp-servers has no action ${action}`)
    }
  }
}

const read = (action: string, ...args: unknown[]): VectorEvent & { query: true } => ({
  action,
  args,
  query: true
})
const blank = { name: '', value: '' }
const remote = (over: Partial<RawDraft['remote']> = {}, name = 'Linear'): RawDraft => ({
  kind: 'remote',
  name,
  remote: {
    url: 'https://mcp.linear.app/mcp',
    transport: 'http',
    transportPinned: false,
    auth: 'browser',
    headers: [blank],
    ...over
  },
  local: { commandLine: '', env: [blank] }
})
const local = (commandLine: string, env: Row[] = [blank], name = 'docs'): RawDraft => ({
  kind: 'local',
  name,
  remote: remote().remote,
  local: { commandLine, env }
})
const stdio = {
  id: 's1',
  type: 'stdio',
  name: 'fs',
  command: 'npx',
  args: ['-y', '@mcp/fs', 'my dir'],
  env: [{ name: 'ROOT', value: '/w' }],
  enabled: true
}
const sse = {
  id: 's2',
  type: 'sse',
  name: 'Linear',
  url: 'https://mcp.linear.app/sse',
  headers: [{ name: 'authorization', value: 'Bearer x' }],
  enabled: false
}
const catalog = {
  source: 'mcp-registry',
  name: 'io.github/linear',
  version: '1.0.0',
  kind: 'remote',
  index: 0,
  repositoryId: null,
  launchDigest: 'abc'
}

const field = (id: string, over: Partial<McpInstallField>): McpInstallField => ({
  id,
  label: id,
  target: { kind: 'env', name: id.toUpperCase() },
  variable: null,
  isRequired: false,
  isSecret: false,
  format: 'string',
  choices: null,
  default: null,
  placeholder: null,
  description: null,
  ...over
})
const installFields: McpInstallField[] = [
  field('key', { isSecret: true, isRequired: true }),
  field('dir', { format: 'filepath', default: '/tmp' }),
  field('flag', { format: 'boolean', default: ' TRUE ' }),
  field('off', { format: 'boolean', default: null }),
  field('n', { format: 'number', default: '3' })
]

export const mcpServersScenarios: Scenario[] = [
  {
    name: 'the_registry_keeps_valid_entries',
    steps: [
      read('normalize', [
        stdio,
        sse,
        { id: 'n', name: 'no type', command: 'uvx', extra: 1 },
        { id: 'h', type: 'http', name: 'h', url: 'https://x.dev/mcp', catalog },
        {
          id: 'c',
          type: 'http',
          name: 'bad catalog',
          url: 'https://x.dev',
          catalog: { source: 'x' }
        },
        { id: 'bad-url', type: 'http', name: 'b', url: 'not a url' },
        { id: 'bad-args', name: 'b', command: 'x', args: ['a', 1] },
        { id: 'bad-env', name: 'b', command: 'x', env: [{ name: 'A' }] },
        { id: 'null-enabled', name: 'b', command: 'x', enabled: null },
        { id: 'ws', type: 'ws', name: 'b', url: 'wss://x' },
        { id: 'blank', name: '  ', command: 'x' },
        { name: 'no id', command: 'x' },
        'text'
      ]),
      read('normalize', { not: 'a list' }),
      read('normalize', null)
    ]
  },
  {
    name: 'validation_names_each_problem',
    steps: [
      read('validate', { name: '', command: ' ' }),
      read('validate', { type: 'http', name: 'a', url: '' }),
      ...[
        'https://mcp.example.com/mcp',
        'http://localhost:3000/sse',
        'localhost:3000',
        'example.com:8080/mcp',
        'mcp.example.com',
        'https://',
        'http://a b/',
        'http://host:99999/',
        'http://[::1]:8080/mcp',
        'file:///tmp/x',
        ' https://padded.dev '
      ].map((url) => read('validate', { type: 'sse', name: 'a', url }))
    ]
  },
  {
    name: 'command_lines_split_and_join_like_a_shell',
    steps: [
      read('tokenize', 'npx -y @mcp/fs "/my dir" \'it\'"s"'),
      read('tokenize', 'a\\ b "q\\"x\\\\" \'\\n\' c\\d'),
      read('tokenize', '  spaced   out  '),
      read('tokenize', 'npx "open'),
      read('tokenize', ''),
      read('join', ['npx', '-y', 'my dir', "it's", '', 'a"b', 'plain']),
      read('liftEnvPrefix', local('A=1 B=two npx server --flag=1', [blank])),
      read('liftEnvPrefix', local('A=1  B=2', [{ name: 'A', value: 'old' }, blank])),
      read('liftEnvPrefix', local('npx A=1')),
      read('liftEnvPrefix', local('A="open')),
      read('rowProblem', 'A', ''),
      read('rowProblem', '', 'v'),
      read('rowProblem', ' ', ' ')
    ]
  },
  {
    name: 'drafts_become_servers_or_name_their_errors',
    steps: [
      read(
        'toConfig',
        local('API=1 npx -y "my server"', [{ name: ' ROOT ', value: '/w' }, blank], ' fs ')
      ),
      read('toConfig', local("npx 'open", [{ name: 'X', value: '' }], '')),
      read('toConfig', local('')),
      read('toConfig', remote()),
      read('toConfig', remote({ url: 'nope', headers: [{ name: '', value: 'v' }] }, '')),
      read('toConfig', remote({ url: '' })),
      read(
        'toConfig',
        remote({ transport: 'sse', headers: [{ name: 'Authorization', value: 'Bearer t' }, blank] })
      ),
      read('uncheckedConfig', local("npx 'open", [{ name: 'X', value: '' }])),
      read('uncheckedConfig', remote({ headers: [{ name: 'X', value: '' }] }))
    ]
  },
  {
    name: 'servers_open_as_drafts',
    steps: [
      read('fromServer', stdio),
      read('fromServer', sse),
      read('fromServer', { id: 'h', type: 'http', name: 'h', url: 'https://x.dev/sse' }),
      read('fromServer', { id: 'n', name: 'n', command: 'uvx', args: [] }),
      read('sameLaunch', stdio, {
        ...stdio,
        env: [{ name: 'ROOT', value: '/other' }],
        enabled: false
      }),
      read('sameLaunch', stdio, { ...stdio, args: ['-y'] }),
      read('sameLaunch', sse, { ...sse, headers: [] })
    ]
  },
  {
    name: 'url_and_sign_in_edits',
    steps: [
      read('transportForUrl', 'https://x.dev/sse'),
      read('transportForUrl', 'https://x.dev/SSE/?a=1'),
      read('transportForUrl', 'https://sse'),
      read('transportForUrl', 'x.dev/sse#frag'),
      read('transportForUrl', 'https://x.dev/sse/more'),
      read('withUrl', remote(), 'https://x.dev/sse'),
      read('withUrl', remote({ transportPinned: true }), 'https://x.dev/sse'),
      read('withAuth', remote({ headers: [{ name: 'X-Team', value: '1' }, blank] }), 'token'),
      read('withAuth', remote({ headers: [{ name: 'authorization', value: 'k' }] }), 'token'),
      read(
        'withAuth',
        remote({ auth: 'token', headers: [{ name: 'Authorization', value: 'k' }] }),
        'browser'
      ),
      read(
        'withAuth',
        remote({
          headers: [
            { name: 'Authorization', value: '' },
            { name: 'X', value: '1' }
          ]
        }),
        'browser'
      )
    ]
  },
  {
    name: 'cards_show_status_target_and_tools',
    steps: [
      read('status', false, true, 'connected'),
      read('status', true, true, null),
      read('status', true, false, 'connected'),
      read('status', true, false, 'disconnected'),
      read('status', true, false, 'authRequired'),
      read('status', true, false, null),
      read('target', stdio),
      read('target', sse),
      read('toolCount', 0),
      read('toolCount', 1),
      read('toolCount', 12)
    ]
  },
  {
    name: 'catalog_rows_and_sync_say_what_react_says',
    steps: [
      ...[
        { code: 'needs-runtime', runtime: 'npx' },
        { code: 'needs-runtime', runtime: 'dotnet' },
        { code: 'no-install-options' },
        { code: 'unparseable' },
        { code: 'insecure-url' },
        { code: 'local-network-url' },
        { code: 'invalid-url' },
        { code: 'unknown-remote-transport', transport: 'ws' },
        { code: 'secret-in-url' },
        { code: 'secret-in-arguments' },
        { code: 'runtime-arguments' },
        { code: 'runtime-hint-mismatch', runtimeHint: 'bunx' },
        { code: 'denied-env', name: 'LD_PRELOAD' },
        { code: 'invalid-env-name', name: '1X' },
        { code: 'unsupported-registry-type', registryType: 'cargo' },
        { code: 'non-stdio-transport', transport: 'sse' },
        { code: 'unpinned-version' },
        { code: 'invalid-package' },
        { code: 'unsupported-registry', registry: 'example.com' },
        { code: 'invalid-command' },
        { code: 'invalid-header-name', name: 'a b' },
        { code: 'control-character', field: 'description' },
        { code: 'duplicate-name', name: 'API_KEY' },
        { code: 'cmd-metacharacter', field: 'argument' },
        { code: 'unsupported-launch-argument', command: 'npx', argument: '--package' },
        { code: 'unsupported-runner', command: 'bunx' }
      ].map((reason) => read('reasonText', reason)),
      ...['installable', 'needs-runtime', 'unsupported', 'source-only'].map((state) =>
        read('stateLabel', state)
      ),
      read('sampleText', { sample: false, updatedAt: null, servers: 0, sync: { state: 'idle' } }),
      read('sampleText', {
        sample: true,
        updatedAt: null,
        servers: 0,
        sync: { state: 'disabled' }
      }),
      read('sampleText', { sample: true, updatedAt: null, servers: 0, sync: { state: 'idle' } }),
      read('syncText', { sample: false, updatedAt: null, servers: 0, sync: { state: 'disabled' } }),
      read('syncText', { sample: false, updatedAt: null, servers: 0, sync: { state: 'idle' } }),
      read('syncText', { sample: true, updatedAt: null, servers: 0, sync: { state: 'idle' } }),
      read('syncText', {
        sample: false,
        updatedAt: null,
        servers: 0,
        sync: { state: 'syncing', full: true, fetched: 12345 }
      }),
      read('syncText', {
        sample: false,
        updatedAt: null,
        servers: 0,
        sync: { state: 'syncing', full: false, fetched: 0 }
      }),
      read('syncErrorText', 'FORBIDDEN'),
      read('syncErrorText', 'NETWORK_ERROR'),
      read('updatedText')
    ]
  },
  {
    name: 'pasted_json_imports_like_the_dialog',
    steps: [
      read('jsonImport', '{"mcpServers": {"b": {"command": " npx ", "args": [" -y ", "pkg"], "env": {"TOKEN": "x", "N": 3, "F": false}}, "1": {"url": " https://mcp.example.com/mcp "}}}'),
      read('jsonImport', '{"mcpServers": {"ok": {"command": "x"}, "bad": {"env": 3}, "h": {"headers": [{"name": "A"}], "url": "https://h.dev"}, "noname": {"type": "sse"}, "list": [], "acp": {"type": "acp", "url": "https://a.dev/sse"}, "argz": {"command": "x", "args": [1]}}}'),
      read('jsonImport', '{"name": "solo", "type": "sse", "url": "https://s.dev/sse", "headers": [{"name": "Authorization", "value": "Bearer t", "extra": 1}], "alwaysAllow": []}'),
      read('jsonImport', '{"command": "x"}'),
      read('jsonImport', '[1]'),
      read('jsonImport', '{"mcpServers": 1}'),
      read('jsonImport', '{"mcpServers": {"u": {"url": 5}}}')
    ]
  },
  {
    name: 'install_form_values_and_retry',
    steps: [
      read('installInitial', installFields),
      read('installSplit', installFields, { key: 's3cret', dir: '/w', flag: 'true' }),
      read('installSplit', installFields, {}),
      read('canStartOver', { kind: 'host', code: 'MCP_CATALOG_NOT_FOUND' }),
      read('canStartOver', { kind: 'host', code: 'MCP_PLAN_EXPIRED' }),
      read('canStartOver', { kind: 'save' })
    ]
  }
]
