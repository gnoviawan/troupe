import {
  agentToolsLabel,
  automationState,
  formatDuration,
  nodeKeyDetail,
  runStatusPresentation,
  triggerSummary
} from '../../../../src/renderer/components/automations/automation-format'
import { refuseConnect } from '../../../../src/renderer/components/automations/canvas/draft/apply-op'
import {
  changedPositions,
  type Draft,
  type DraftAction,
  draftReducer,
  type LayoutPatch,
  openDraft,
  pendingOps,
  planGesture
} from '../../../../src/renderer/components/automations/canvas/draft/draft'
import {
  type AddPlacement,
  addGesture,
  movePatch,
  removeGesture
} from '../../../../src/renderer/components/automations/canvas/draft/gestures'
import { stepOrder } from '../../../../src/renderer/components/automations/canvas/layout'
import {
  chainPort,
  newNode
} from '../../../../src/renderer/components/automations/canvas/palette/node-kinds'
import {
  openingStep,
  runOverlay
} from '../../../../src/renderer/components/automations/canvas/run/run-overlay'
import fixture from '../../../../src/shared/types/__fixtures__/automation.json'
import opsFixture from '../../../../src/shared/types/__fixtures__/automation-ops.json'
import type {
  Automation,
  AutomationNode,
  AutomationOp,
  AutomationRun,
  AutomationStep,
  Edge,
  Graph,
  Issue,
  NodeKind,
  NodePosition
} from '../../../../src/shared/types/automation.types'
import type { Scenario, StoreRecorder, VectorEvent } from '../record'
import { type Api, routesRecorder } from './git-routes'

const ev = (action: string, ...args: unknown[]): VectorEvent => ({ action, args })
const q = (action: string, ...args: unknown[]) => ({ action, args, query: true as const })
const DIR = 'src/renderer/components/automations'

/**
 * Every card of the fixture automation placed, so a gesture freezes no laid-out card: the
 * Rune canvas lays out unsaved cards on its own, and only saved positions must match.
 */
const PLACED = {
  trigger: { x: 0, y: 40 },
  fetch: { x: 240, y: 40.5 },
  extract: { x: 480, y: -16 },
  alert: { x: 480, y: 160 },
  check: { x: 720, y: 0 },
  shape: { x: 960, y: 0 },
  post: { x: 1200, y: 0 }
}

const base = fixture.automation as unknown as Automation
const head: Automation = { ...base, layout: PLACED }
const issue = (node: string | null, code: string): Issue =>
  ({ node_id: node, code, message: `${code} on ${node ?? 'the trigger'}` }) as Issue

let draft: Draft = openDraft({ automation: head, issues: [] })

function reduce(action: DraftAction): void {
  draft = draftReducer(draft, action)
}

/** Plans and applies a gesture; returns why the host would refuse its ops. */
function gesture(ops: AutomationOp[], layout: LayoutPatch = {}): unknown {
  const planned = planGesture(draft, ops, layout)
  if (!('forward' in planned)) return planned
  reduce({ type: 'gesture', gesture: planned })
  return undefined
}

/**
 * The automation canvas draft (`canvas/draft/`): gestures through `planGesture`, then the
 * reducer. `previewed` takes whether its pending list is the draft's current one, which the
 * web client checks by identity.
 */
export const automationDraftRecorder: StoreRecorder = {
  store: 'automation-draft',
  sources: [
    `${DIR}/canvas/draft/draft.ts`,
    `${DIR}/canvas/draft/apply-op.ts`,
    `${DIR}/canvas/draft/inverse.ts`,
    `${DIR}/canvas/draft/gestures.ts`,
    `${DIR}/canvas/layout.ts`,
    `${DIR}/canvas/ports.ts`,
    `${DIR}/canvas/palette/node-kinds.ts`
  ],
  reset() {
    draft = openDraft({ automation: head, issues: [] })
  },
  seed(initial) {
    draft = openDraft({
      automation: initial.automation as Automation,
      issues: (initial.issues as Issue[]) ?? []
    })
  },
  snapshot: () => draft,
  dispatch({ action, args }) {
    const { graph } = draft.doc
    switch (action) {
      case 'gesture':
        return gesture(args[0] as AutomationOp[], (args[1] as LayoutPatch) ?? {})
      case 'add': {
        const kind = args[0] as NodeKind
        const node = newNode(kind, graph)
        const { ops, layout } = addGesture(
          graph,
          draft.layout,
          node,
          chainPort(kind),
          args[1] as AddPlacement
        )
        return gesture(ops, layout)
      }
      case 'remove': {
        const { ops, layout } = removeGesture(
          graph,
          draft.layout,
          args[0] as AutomationNode['id'][],
          args[1] as Edge[]
        )
        return ops.length > 0 ? gesture(ops, layout) : undefined
      }
      case 'move': {
        const patch = movePatch(graph, draft.layout, args[0] as Record<string, NodePosition>)
        return patch ? gesture([], patch) : undefined
      }
      case 'connect': {
        const edge = args[0] as Edge
        return refuseConnect(graph, edge) ?? gesture([{ op: 'connect', edge }])
      }
      case 'previewed':
        return reduce({
          type: 'previewed',
          pending: args[1] ? draft.pending : [...draft.pending],
          issues: args[0] as Issue[]
        })
      case 'saved':
        return reduce({
          type: 'saved',
          automation: args[0] as Automation,
          issues: args[1] as Issue[]
        })
      case 'requeue':
        return reduce({ type: 'requeue', layout: args[0] as LayoutPatch })
      case 'head':
        return reduce({
          type: 'head',
          head: { automation: args[0] as Automation, issues: args[1] as Issue[] }
        })
      case 'rebased':
        return reduce({
          type: 'rebased',
          head: { automation: args[0] as Automation, issues: args[1] as Issue[] },
          by: args[2] as string | null
        })
      case 'undo':
      case 'redo':
      case 'discard':
      case 'dismiss_conflict':
        return reduce({ type: action })
      case 'pendingOps':
        return pendingOps(draft)
      case 'changedPositions':
        return changedPositions(draft.base.layout, draft.layout)
      default:
        throw new Error(`automation-draft: no action ${action}`)
    }
  }
}

const transform = (id: string): AutomationNode =>
  ({
    id,
    label: id,
    spec: { kind: 'transform', fields: [{ name: 'text', value: '{{nodes.fetch.output}}' }] }
  }) as AutomationNode

/** A later head: another author renamed it, removed `alert` and added `note`. */
const later: Automation = {
  ...head,
  revision: head.revision + 1,
  name: 'Renamed elsewhere',
  graph: {
    nodes: [...head.graph.nodes.filter((node) => node.id !== 'alert'), transform('note')],
    edges: head.graph.edges.filter((edge) => edge.to !== 'alert')
  },
  layout: { ...PLACED, note: { x: 1440, y: 0 } }
}

const saved = (rev: number, graph: Graph): Automation => ({ ...head, revision: rev, graph })

export const automationDraftScenarios: Scenario[] = [
  {
    name: 'each fixture batch as a gesture, undone and redone',
    steps: [
      ...(opsFixture.apply as { ops: AutomationOp[] }[])
        .slice(0, 6)
        .flatMap((c) => [ev('gesture', c.ops)]),
      q('pendingOps'),
      ev('undo'),
      ev('undo'),
      ev('redo'),
      ev('gesture', [{ op: 'rename', name: '  ' }]),
      ev('gesture', []),
      q('pendingOps')
    ]
  },
  {
    name: 'add, connect, move and remove on a placed canvas',
    steps: [
      ev('add', 'wait', { at: { x: 1440, y: 0 }, from: { from: 'post', port: 'out' } }),
      ev('add', 'condition', { at: { x: 600, y: 300 } }),
      ev('connect', { from: 'condition', port: 'out', to: 'wait' }),
      ev('connect', { from: 'condition', port: 'true', to: 'wait' }),
      ev('connect', { from: 'condition', port: 'true', to: 'wait' }),
      ev('connect', { from: 'wait', port: 'out', to: 'fetch' }),
      ev('add', 'transform', {
        at: { x: 360, y: 200 },
        split: { from: 'fetch', port: 'out', to: 'extract' }
      }),
      ev('move', { condition: { x: 610, y: 320 }, wait: { x: 1440, y: 0 } }),
      ev('move', { wait: { x: 1440, y: 0 } }),
      ev('remove', ['condition'], [{ from: 'fetch', port: 'error', to: 'alert' }]),
      q('changedPositions'),
      ev('undo'),
      ev('undo'),
      q('changedPositions'),
      ev('redo'),
      q('pendingOps')
    ]
  },
  {
    name: 'dry runs land only on the pending list they ran',
    steps: [
      ev('gesture', [{ op: 'rename', name: 'Digest' }]),
      ev('previewed', [issue('fetch', 'invalid_url')], true),
      ev('previewed', [issue(null, 'invalid_schedule')], false),
      ev('gesture', [], { fetch: { x: 250, y: 50 } }),
      ev('previewed', [issue(null, 'cycle')], true),
      ev('undo'),
      ev('undo'),
      ev('previewed', [issue('fetch', 'invalid_url')], true)
    ]
  },
  {
    name: 'save keeps the history on our own revision only',
    steps: [
      ev('gesture', [{ op: 'remove_node', id: 'alert' }]),
      ev('saved', saved(head.revision + 1, later.graph), [issue('check', 'unreachable')]),
      ev('undo'),
      ev('gesture', [], { post: { x: 1300, y: 10 } }),
      ev('requeue', { post: { x: 1300, y: 10 } }),
      ev('saved', saved(head.revision + 5, head.graph), [])
    ]
  },
  {
    name: 'a new head replaces a clean draft and waits behind a dirty one',
    steps: [
      ev('head', later, []),
      ev('head', head, []),
      ev('gesture', [{ op: 'set_isolation', isolation: 'worktree_per_run' }]),
      ev('head', { ...later, revision: later.revision + 1 }, []),
      ev('discard'),
      ev('head', later, [issue(null, 'invalid_schedule')])
    ]
  },
  {
    name: 'a rebase replays the edits that still apply and names the rest',
    steps: [
      ev('gesture', [{ op: 'update_node', id: 'alert', label: 'Alarm' }]),
      ev('gesture', [{ op: 'connect', edge: { from: 'check', port: 'false', to: 'alert' } }]),
      ev('gesture', [{ op: 'add_node', node: transform('note') }]),
      ev('gesture', [{ op: 'rename', name: 'Mine' }], { post: { x: 1250, y: 5 } }),
      ev('rebased', later, [], 'Claude'),
      ev('dismiss_conflict'),
      ev('rebased', later, [], null)
    ]
  }
]

/** Plain objects for the Maps the overlay holds. */
function plainOverlay(overlay: ReturnType<typeof runOverlay>) {
  return {
    fired: overlay.fired,
    cards: Object.fromEntries(overlay.cards),
    edges: Object.fromEntries(overlay.edges),
    loops: Object.fromEntries(overlay.loops),
    loopOf: Object.fromEntries(overlay.loopOf),
    rows: Object.fromEntries(overlay.rows)
  }
}

const runs = fixture.runs as unknown as AutomationRun[]
const steps = fixture.steps as unknown as AutomationStep[]
const stepsOf = (runId: string) => steps.filter((step) => step.run_id === runId)

/** What a run shows (`canvas/run/`), how automations read and the add menu's new nodes. */
export const automationRunRecorder: StoreRecorder = {
  store: 'automation-run',
  sources: [
    `${DIR}/canvas/run/run-overlay.ts`,
    `${DIR}/canvas/inspector/template-scope.ts`,
    `${DIR}/automation-format.ts`
  ],
  reset() {},
  seed() {},
  snapshot: () => null,
  dispatch({ action, args }) {
    switch (action) {
      case 'runOverlay': {
        const run = runs[args[0] as number]
        return plainOverlay(
          runOverlay(run, stepsOf(run.id), (args[1] as Record<string, number>) ?? {})
        )
      }
      case 'openingStep': {
        const run = runs[args[0] as number]
        return openingStep(runOverlay(run, stepsOf(run.id), {}), stepOrder(run.graph))
      }
      case 'stepOrder':
        return stepOrder(args[0] as Graph).map((node) => node.id)
      case 'automationState':
        return automationState(args[0] as Automation)
      case 'formatDuration':
        return formatDuration(args[0] as number)
      case 'triggerSummary':
        return triggerSummary(args[0] as Automation['trigger'])
      case 'nodeKeyDetail':
        return nodeKeyDetail(args[0] as AutomationNode['spec'])
      case 'agentToolsLabel':
        return agentToolsLabel(args[0] as Parameters<typeof agentToolsLabel>[0])
      case 'runStatusLabel':
        return runStatusPresentation(runs[args[0] as number]).label
      case 'newNode':
        return newNode(args[0] as NodeKind, args[1] as Graph)
      case 'chainPort':
        return chainPort(args[0] as NodeKind)
      default:
        throw new Error(`automation-run: no action ${action}`)
    }
  }
}

const KINDS: NodeKind[] = [
  'http',
  'agent',
  'mcp_tool',
  'condition',
  'for_each',
  'merge',
  'wait',
  'fail',
  'approval',
  'transform'
]

export const automationRunScenarios: Scenario[] = [
  {
    name: 'the fixture runs on their own graphs',
    steps: [
      q('runOverlay', 0),
      q('runOverlay', 0, { each: 0 }),
      q('runOverlay', 0, { each: 7 }),
      q('runOverlay', 1),
      q('runOverlay', 2),
      q('openingStep', 0),
      q('openingStep', 1),
      q('openingStep', 2),
      q('stepOrder', runs[0].graph),
      q('stepOrder', base.graph),
      q('runStatusLabel', 0),
      q('runStatusLabel', 1),
      q('runStatusLabel', 2)
    ]
  },
  {
    name: 'how automations read',
    steps: [
      q('automationState', base),
      q('automationState', { ...base, approved_revision: base.revision, enabled: true }),
      q('automationState', { ...base, approved_revision: base.revision, enabled: false }),
      ...[0, 999, 1000, 59_400, 60_000, 125_000, 3_600_000, 5_400_000, 86_400_000, 90_000_000].map(
        (ms) => q('formatDuration', ms)
      ),
      ...(fixture.triggers as Automation['trigger'][])
        .filter((trigger) => trigger.kind === 'manual' || trigger.schedule.kind !== 'at')
        .map((trigger) => q('triggerSummary', trigger)),
      ...base.graph.nodes.map((node) => q('nodeKeyDetail', node.spec)),
      ...runs[0].graph.nodes.map((node) => q('nodeKeyDetail', node.spec)),
      q('agentToolsLabel', []),
      q('agentToolsLabel', ['read', 'execute', 'other'])
    ]
  },
  {
    name: 'new nodes from the add menu',
    steps: [
      ...KINDS.flatMap((kind) => [q('newNode', kind, base.graph), q('chainPort', kind)]),
      q('newNode', 'http', {
        nodes: [
          { id: 'http', label: 'Other', spec: base.graph.nodes[0].spec },
          { id: 'other', label: 'HTTP 2', spec: base.graph.nodes[0].spec }
        ],
        edges: []
      })
    ]
  }
]

export function automationRoutesRecorder(api: Api): StoreRecorder {
  return routesRecorder(
    'automation-routes',
    ['src/renderer/lib/automation-api.ts', 'src/renderer/lib/ipc/http.ts'],
    api
  )
}

const ID = 'auto 1/é'
export const automationRoutesScenarios: Scenario[] = [
  {
    name: 'every route the Rune client calls',
    steps: [
      ev('list', 'proj 1'),
      ev('get', ID),
      ev('create', 'proj 1', 'Digest'),
      ev('applyOps', ID, 3, [{ op: 'rename', name: 'x' }]),
      ev('previewOps', ID, 3, [{ op: 'remove_node', id: 'fetch' }]),
      ev('setLayout', ID, { trigger: { x: 0, y: 1.5 } }),
      ev('remove', ID),
      ev('setEnabled', ID, false),
      ev('approve', ID, 4),
      ev('runNow', ID),
      ev('cancelRun', 'run/1'),
      ev('decideApproval', 'run/1', 'gate', true),
      ev('listRuns', ID, 25),
      ev('runSteps', 'run/1'),
      ev('markSeen', ID),
      ev('listAgents'),
      ev('listSecrets', 'proj 1')
    ]
  }
]
