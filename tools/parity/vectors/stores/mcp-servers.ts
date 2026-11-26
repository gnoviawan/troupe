import {
  mcpServerStatus,
  mcpServerTarget,
  mcpToolCount
} from '../../../../src/renderer/components/settings/mcp-server-view'
import {
  normalizeMcpRegistry,
  type StoredMcpServer,
  sameLaunch,
  validateMcpServer
} from '../../../../src/renderer/lib/acp-mcp-persistence'
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
    'src/renderer/components/settings/mcp-server-view.ts'
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
  }
]
