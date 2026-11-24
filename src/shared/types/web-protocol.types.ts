/**
 * WS relay protocol frame schema (Story 1.4).
 *
 * The wire contract between the `troupe-server` WS relay and any browser
 * client. The message types and name sets come from `crates/troupe-protocol`
 * through the generated `./generated/protocol.ts`, re-exported here under
 * their contract names. The types declared here are the ones the crate does
 * not model one to one: generic replies, the client's event-type subset, and
 * the client-side registries.
 *
 * # Wire casing (AC3 — deviation from architecture text, MUST follow)
 *
 * The **envelope** fields (`sid`, `seq`, `type`, `payload`) are snake_case.
 * The **payload** is the existing camelCase-serialized ACP event struct
 * `Value`. The relay
 * serializes the payload ONCE via `fan_out` and forwards the `Value`
 * verbatim — this file does NOT re-case payload fields. `acp-api.ts` becomes
 * a no-op case boundary for payload fields; only the envelope is mapped.
 *
 * # Reliability tiers (AC5)
 *
 * `WS_RELAY_TIERS` is the single registry mapping each event `type` to its
 * delivery tier. Mirrors the Rust `ReliabilityTier` enum + `tier_of(type_)`.
 * Lossy events drop-oldest on a slow client; reliable events never drop
 * (unbounded per-client queue in this story); idempotent events dedup by
 * turn-id. Full ack/backpressure lands in Story 1.7.
 *
 * # OS vs human cap boundary (AC8)
 *
 * `WS_OS_FULFILLED_CAPS` / `WS_HUMAN_RELAYED_CAPS` mirror the Rust
 * `OS_FULFILLED_CAPS` / `HUMAN_RELAYED_CAPS` registries. The server is the
 * ACP client-of-record: OS caps are fulfilled locally; only human caps are
 * relayed to the browser. A browser WS request for an OS cap is rejected
 * with `err.code: "unsupported"`.
 *
 * Runtime-neutral: no `@tauri-apps/*` imports, no `@renderer/*` imports.
 * ESM-first. Strict-typed (no `any`).
 */

import type {
  ProjectBusyDetails,
  ReliabilityTier,
  WsErrorCode,
  WsRequestType
} from './generated/protocol'
import { WsRequestTypeValues, WsWatchNameValues } from './generated/protocol'

export type {
  AcpAuthenticatePayload,
  AcpAuthenticateReply,
  AcpRuntimePolicy,
  AddProjectPayload,
  AgentCapabilities,
  AgentsWatchDelta,
  BackgroundCommand,
  BackgroundCommandList,
  ChatActivityRecord,
  ChatActivityWatchDelta,
  ChatActivityWatchState,
  ChatHistoryWatchDelta,
  ChatReadMark,
  DeleteSessionPayload,
  FsWatchChangeDelta,
  FsWatchDelta,
  FsWatchEndedDelta,
  FsWatchState,
  HistoryMode,
  PersistedSessionSummary,
  ProjectBusyDetails,
  RecordAgentSwitchPayload,
  ReliabilityTier,
  RemoveProjectPayload,
  SessionCursorReply,
  SessionSubscribeReply,
  StoreDeletePayload,
  StoreDeleteReply,
  StoreDeviceWriteReply,
  StoreReadPayload,
  StoreReadReply,
  StoreSharedWriteReply,
  StoreWatchState,
  StoreWritePayload,
  TerminalsWatchState,
  UpdateProjectPayload,
  UserPromptEvent,
  WatchResyncDelta,
  WsAgentSummary,
  WsCanvasCallFrame,
  WsCanvasLeaseLostFrame,
  WsCanvasMountReply,
  WsCanvasReplyPayload,
  WsErrorCode,
  WsGapFrame,
  WsHeartbeatFrame,
  WsRequest,
  WsRequestType,
  WsTerminalSetEntry,
  WsWatchDeltaFrame,
  WsWatchFrame,
  WsWatchName,
  WsWatchReply,
  WsWatchRevision,
  WsWatchStateFrame
} from './generated/protocol'
export {
  MAX_CLIENT_ID_LEN,
  MAX_COMMAND_ID_LEN,
  WS_PROTOCOL_VERSION
} from './generated/protocol'

// ============================================================================
// Event `type` names (17) — acp:* prefix-dropped, snake_case (AC2)
// ============================================================================

/**
 * The `acp:*` event names from `src-tauri/src/acp/events.rs` with the `acp:`
 * prefix dropped that the relay sends as session log events, plus the
 * relay-level `auth_required` event and the top-level delivery frames (`gap`,
 * `heartbeat`). Events that belong to no session (`browser_open_request`,
 * `worktree_branch_renamed`, `fs_changed`, ...) are watch deltas, not events:
 * the relay sends no `seq = 0` event except `auth_required`.
 * The `watch` frame has the same name as the `watch` request, so its type
 * lives in {@link WS_WATCH_FRAME_TYPE}, outside this registry.
 */
export const WS_EVENT_TYPES = [
  // Session/agent lifecycle (reliable)
  'agent_spawned',
  'session_created',
  'session_closed',
  'agent_disconnected',
  'agent_error',
  // Session state (reliable)
  'tool_call',
  'mode_update',
  'config_options_update',
  'session_info_update',
  'usage_update',
  // Permission (reliable)
  'permission_request',
  // Prompt turn lifecycle (idempotent)
  'prompt_complete',
  // High-frequency streams (lossy)
  'message_chunk',
  'tool_call_update',
  'commands_update',
  'plan_update',
  // Relay-level (not from events.rs) (reliable)
  'auth_required',
  // Server-authored authoritative user input (durable/replayable).
  'user_prompt',
  // CAP-2 (spec-in-chat-agent-switch): durable agent-switch marker (Reliable
  // tier). The host numbers it once; the durable record and this live event
  // share one seq.
  'agent_switch',
  // Host-authored durable title record (Reliable tier). Occupies a seq in the
  // session log like any record, so live delivery keeps the seq run
  // contiguous; the title itself reaches the UI through the
  // `session_info_update` that follows it, so clients have no handler for it.
  'local_title_generated',
  // Live-only, never persisted: the agent started or stopped working in the
  // session outside a prompt turn (reliable tier).
  'agent_working',
  // Live-only, never persisted: the session's whole list of commands the agent
  // left running, replacing the previous one (reliable tier).
  'background_commands',
  // Top-level frame `{ type: 'gap', sid }` (no `seq`): LOSSY events of `sid`
  // were dropped for this connection because it could not keep up. The client
  // installs an atomic snapshot of the session.
  'gap',
  // Top-level frame `{ type: 'heartbeat' }` (no `seq`): the host writes one
  // every 10 s straight to the socket, so the client judges liveness from any
  // host frame instead of a ping round trip.
  'heartbeat'
] as const

/** Union of all WS event `type` strings. */
export type WsEventType = (typeof WS_EVENT_TYPES)[number]

// ============================================================================
// Request `type` names — `acp_*` operation names prefix-dropped (AC2)
// ============================================================================

/**
 * Every `/ws` request `type`, in the crate's order. Each carrier (the
 * WebSocket and the desktop's Tauri channel) sends these.
 */
export const WS_REQUEST_TYPES = WsRequestTypeValues

/** How the host's receipt ledger treats a request type. Mirrors the Rust `ClassKind`. */
export type WsCommandKind = 'create' | 'command' | 'idempotent' | 'read'

/**
 * How a settled command is answered on a same-id resend: `payload` replays the
 * stored reply, `entity` rebuilds it from the entity stamped with the
 * `commandId` (`send_prompt`: its `turnId` is its `commandId`), `receipt-only`
 * never stores a payload (credential-bearing kinds), `never` keeps the command
 * out of the ledger (one-shot kinds whose reply carries secrets). `null` for
 * `idempotent` and `read`. Mirrors the Rust `Replay`.
 */
export type WsCommandReplay = 'payload' | 'entity' | 'receipt-only' | 'never'

/**
 * The command class of every `/ws` request type — mirrors the Rust
 * `WS_COMMAND_CLASSES` table row for row (pinned by the Rust
 * `command_ledger::class_tests` parity test). `create` and `command` mint a
 * `commandId`; `idempotent` and `read` never need one.
 */
export const WS_COMMAND_CLASSES: Readonly<
  Record<WsRequestType, readonly [WsCommandKind, WsCommandReplay | null]>
> = {
  send_prompt: ['command', 'entity'],
  cancel_prompt: ['idempotent', null],
  stop_subagent: ['idempotent', null],
  set_config_option: ['idempotent', null],
  set_mode: ['idempotent', null],
  set_model: ['idempotent', null],
  respond_permission: ['command', 'payload'],
  answer_question: ['command', 'payload'],
  answer_mcp_install: ['command', 'never'],
  browser_stop: ['idempotent', null],
  create_session: ['create', 'entity'],
  load_session: ['command', 'payload'],
  resume_session: ['command', 'payload'],
  close_session: ['command', 'payload'],
  dispose_ephemeral_session: ['command', 'payload'],
  promote_session: ['command', 'payload'],
  list_sessions: ['read', null],
  register_discovered_session: ['command', 'payload'],
  import_agent_session: ['command', 'payload'],
  spawn_agent: ['create', 'entity'],
  list_claude_accounts: ['read', null],
  list_claude_usage: ['read', null],
  set_default_claude_account: ['idempotent', null],
  set_project_claude_account: ['idempotent', null],
  set_chat_claude_account: ['idempotent', null],
  set_claude_auto_switch: ['idempotent', null],
  set_claude_resume: ['idempotent', null],
  add_claude_account: ['command', 'payload'],
  rename_claude_account: ['idempotent', null],
  remove_claude_account: ['idempotent', null],
  start_claude_link_login: ['idempotent', null],
  finish_claude_link_login: ['command', 'receipt-only'],
  cancel_claude_login: ['idempotent', null],
  list_codex_accounts: ['read', null],
  list_codex_usage: ['read', null],
  set_default_codex_account: ['idempotent', null],
  set_project_codex_account: ['idempotent', null],
  set_chat_codex_account: ['idempotent', null],
  set_codex_auto_switch: ['idempotent', null],
  set_codex_resume: ['idempotent', null],
  add_codex_account: ['command', 'payload'],
  rename_codex_account: ['idempotent', null],
  remove_codex_account: ['idempotent', null],
  start_codex_account_login: ['command', 'payload'],
  list_antigravity_accounts: ['read', null],
  list_antigravity_usage: ['read', null],
  set_default_antigravity_account: ['idempotent', null],
  set_project_antigravity_account: ['idempotent', null],
  set_chat_antigravity_account: ['idempotent', null],
  set_antigravity_auto_switch: ['idempotent', null],
  set_antigravity_resume: ['idempotent', null],
  add_antigravity_account: ['command', 'payload'],
  rename_antigravity_account: ['idempotent', null],
  remove_antigravity_account: ['idempotent', null],
  start_antigravity_account_login: ['command', 'payload'],
  kill_agent: ['idempotent', null],
  list_agents: ['read', null],
  authenticate: ['read', null],
  authenticate_agent: ['command', 'receipt-only'],
  logout_agent: ['idempotent', null],
  delete_agent_session: ['idempotent', null],
  respond_elicitation: ['command', 'receipt-only'],
  subscribe: ['read', null],
  ping: ['read', null],
  list_persisted_sessions: ['read', null],
  delete_session: ['idempotent', null],
  rename_session: ['idempotent', null],
  open_persisted_session: ['read', null],
  get_session_payload: ['read', null],
  recover_session_snapshot: ['read', null],
  get_session_cursor: ['read', null],
  get_session_payload_tail: ['read', null],
  list_acp_catalog: ['read', null],
  set_catalog_opt_in: ['idempotent', null],
  fetch_acp_registry_snapshot: ['read', null],
  search_mcp_catalog: ['read', null],
  get_mcp_catalog_status: ['read', null],
  set_mcp_catalog_sync: ['idempotent', null],
  describe_mcp_install: ['read', null],
  plan_mcp_install: ['idempotent', null],
  commit_mcp_install: ['command', 'never'],
  cancel_mcp_install: ['idempotent', null],
  get_mcp_catalog_icon: ['read', null],
  install_acp_agent: ['command', 'payload'],
  acp_deliver_auth_redirect: ['command', 'receipt-only'],
  store_read: ['read', null],
  store_write: ['idempotent', null],
  store_delete: ['idempotent', null],
  add_project: ['idempotent', null],
  update_project: ['idempotent', null],
  remove_project: ['idempotent', null],
  set_default_project: ['idempotent', null],
  subagent_settings_get: ['read', null],
  subagent_settings_set: ['idempotent', null],
  record_agent_switch: ['command', 'payload'],
  mark_chat_read: ['idempotent', null],
  watch: ['read', null],
  unwatch: ['read', null],
  canvas_mount: ['idempotent', null],
  canvas_unmount: ['idempotent', null],
  canvas_reply: ['idempotent', null]
}

/** A well-formed `commandId`: 1-128 characters of `[A-Za-z0-9_-]`. */
export const COMMAND_ID_PATTERN = /^[A-Za-z0-9_-]{1,128}$/

// Frozen replay contract 1: `resume_session` NEVER emits replay events or a
// replay snapshot — history reconstruction belongs to `get_session_payload` /
// `recover_session_snapshot`. The `resume_session` ok payload carries the
// explicit `"replaySnapshot": null` marker documenting that absence.

// ============================================================================
// Error codes — stable machine strings (AC2)
// ============================================================================

/**
 * The stable `err.code` machine strings. Mirrors the Rust `WsErrorCode`
 * enum (snake_case `code`). Extended from the architecture's 7 by
 * `unsupported` (OS-cap rejection, AC8), `not_implemented` (stub request
 * handlers, AC10), and `agent_auth_required` (agent rejected session entry with ACP
 * AuthRequired -32000, Story 7).
 */
export const WS_ERROR_CODES = {
  NOT_FOUND: 'not_found',
  UNAUTHORIZED: 'unauthorized',
  RATE_LIMITED: 'rate_limited',
  AGENT_CRASHED: 'agent_crashed',
  PERMISSION_DENIED: 'permission_denied',
  STALE: 'stale',
  DUPLICATE: 'duplicate',
  UNSUPPORTED: 'unsupported',
  NOT_IMPLEMENTED: 'not_implemented',
  // Agent rejected session entry with ACP AuthRequired (-32000) — the user
  // must authenticate first (Story 7; additive — old clients ignore unknown
  // codes).
  AGENT_AUTH_REQUIRED: 'agent_auth_required',
  // resume_session/load_session rejected because a DIFFERENT live agent owns
  // the session and is mid-turn (issue #837). Retryable: re-ask the host for
  // the owner (`list_agents.ownsSession`) and resume on that agent.
  SESSION_OWNED_BY_OTHER: 'session_owned_by_other',
  // `authenticate` carried a missing or different `protocolVersion`
  // (`WS_PROTOCOL_VERSION`). The client must reload; no compat shim (AD-15).
  PROTOCOL_VERSION_MISMATCH: 'protocol_version_mismatch',
  // `authenticate` carried a missing, empty or oversized `deviceId` /
  // `clientInstanceId` (AD-4). The host never mints either id.
  INVALID_CLIENT_IDENTITY: 'invalid_client_identity',
  // A host terminal cap (per project or global) refused a spawn. The terminal
  // carriers answer it as `TERMINAL_LIMIT` (`WEB_TERMINAL_ERROR_CODES`).
  TERMINAL_LIMIT: 'terminal_limit',
  // Project remove/archive refused while bound sessions/PTYs are live;
  // `err.details` is a `ProjectBusyDetails`. Resend with `cascade: true` to
  // have the host stop them.
  PROJECT_BUSY: 'project_busy',
  // A command resend found no stored outcome (evicted ledger entry or an
  // interrupted handler); never re-executed.
  OUTCOME_UNKNOWN: 'outcome_unknown',
  // Branch switch / worktree removal refused while a session in the target
  // path has an active turn.
  DESTRUCTIVE_OP_BLOCKED: 'destructive_op_blocked',
  // Upstream provider refused the call: quota / usage limit.
  PROVIDER_QUOTA_EXCEEDED: 'provider_quota_exceeded',
  // Upstream provider refused the call: session limit.
  PROVIDER_SESSION_LIMIT: 'provider_session_limit',
  // Upstream provider rejected the request with a 4xx class error.
  PROVIDER_REQUEST_REJECTED: 'provider_request_rejected',
  // The connection lacks the capability the request needs (view-only
  // connection attempting a mutation outside `device/<deviceId>/`).
  FORBIDDEN: 'forbidden',
  // respond_permission / answer_question named a ticket that was already
  // resolved (bounded tombstones; an older ticket reads `stale`).
  ALREADY_RESOLVED: 'already_resolved',
  // Spawn rejected deterministically: the agent binary is missing, not
  // installed, or cannot start. Definite (AD-5): never auto-retried.
  AGENT_UNAVAILABLE: 'agent_unavailable',
  // Spawn rejected deterministically: empty/invalid agent name, command or
  // configId. Definite (AD-5): never auto-retried.
  INVALID_AGENT_CONFIG: 'invalid_agent_config',
  // A `commandId` was resent with a different request than it was first used
  // for. Never replayed, never executed.
  COMMAND_CONFLICT: 'command_conflict',
  // A request's `commandId` is malformed (not 1-128 chars of [A-Za-z0-9_-]).
  INVALID_COMMAND_ID: 'invalid_command_id',
  // The Claude Account a request needs is not signed in: choosing it for a
  // Project or as the default (nothing changed), or starting a managed Claude
  // agent on it. The message names the Account and where to sign in.
  CLAUDE_ACCOUNT_SIGNED_OUT: 'claude_account_signed_out',
  // The Codex Account a managed Codex spawn needs is not signed in. The
  // message names the Account or the project `CODEX_HOME`.
  CODEX_ACCOUNT_SIGNED_OUT: 'codex_account_signed_out',
  // The Antigravity Account a managed Antigravity spawn needs is not signed
  // in. The message names the Account or the project `GEMINI_HOME`.
  ANTIGRAVITY_ACCOUNT_SIGNED_OUT: 'antigravity_account_signed_out',
  // `finish_claude_link_login` got a code the Claude CLI calls incomplete.
  // The sign-in still waits for the whole code.
  CLAUDE_LOGIN_CODE_INCOMPLETE: 'claude_login_code_incomplete',
  // `finish_claude_link_login` got a code while the Claude CLI still checks an
  // earlier one. The sign-in goes on; its first code gets the verdict.
  CLAUDE_LOGIN_BUSY: 'claude_login_busy'
} as const

/**
 * The stable upstream-provider failure codes. They travel as the `err.code` of
 * a failed session/prompt reply and as `providerCode` on `agent_error`.
 */
export const PROVIDER_ERROR_CODES = [
  WS_ERROR_CODES.PROVIDER_QUOTA_EXCEEDED,
  WS_ERROR_CODES.PROVIDER_SESSION_LIMIT,
  WS_ERROR_CODES.PROVIDER_REQUEST_REJECTED
] as const

/** One of {@link PROVIDER_ERROR_CODES}. */
export type ProviderErrorCode = (typeof PROVIDER_ERROR_CODES)[number]

/** True when `value` is a known provider failure code. */
export function isProviderErrorCode(value: unknown): value is ProviderErrorCode {
  return PROVIDER_ERROR_CODES.some((code) => code === value)
}

// ============================================================================
// Reliability tiers (AC5) — single registry mirroring the Rust enum
// ============================================================================

/** The three delivery tiers. Mirrors the Rust `ReliabilityTier` enum. */
export const WS_RELAY_TIERS = {
  LOSSY: 'lossy',
  RELIABLE: 'reliable',
  IDEMPOTENT: 'idempotent'
} as const

/**
 * The single tier registry: maps each `WsEventType` to its `ReliabilityTier`.
 * Mirrors the Rust `tier_of(type_: &str) -> ReliabilityTier`.
 *
 * - `lossy` (dropped for a slow client, followed by a `gap` frame): `message_chunk`, `tool_call_update`,
 *   `commands_update`, `plan_update`.
 * - `idempotent` (dedup by turn-id): `prompt_complete`.
 * - `reliable` (never dropped silently; a subscriber that cannot take one loses
 *   its connection and resumes by cursor): everything else, including
 *   `permission_request` and all request↔reply.
 */
export const WS_EVENT_TIERS: Readonly<Record<WsEventType, ReliabilityTier>> = {
  // Lossy — high-frequency, low-value streams.
  message_chunk: WS_RELAY_TIERS.LOSSY,
  tool_call_update: WS_RELAY_TIERS.LOSSY,
  commands_update: WS_RELAY_TIERS.LOSSY,
  plan_update: WS_RELAY_TIERS.LOSSY,
  // Idempotent — dedup by turn-id.
  prompt_complete: WS_RELAY_TIERS.IDEMPOTENT,
  // Reliable — state/lifecycle/permission events must not be dropped.
  agent_spawned: WS_RELAY_TIERS.RELIABLE,
  session_created: WS_RELAY_TIERS.RELIABLE,
  session_closed: WS_RELAY_TIERS.RELIABLE,
  agent_disconnected: WS_RELAY_TIERS.RELIABLE,
  agent_error: WS_RELAY_TIERS.RELIABLE,
  tool_call: WS_RELAY_TIERS.RELIABLE,
  mode_update: WS_RELAY_TIERS.RELIABLE,
  config_options_update: WS_RELAY_TIERS.RELIABLE,
  session_info_update: WS_RELAY_TIERS.RELIABLE,
  usage_update: WS_RELAY_TIERS.RELIABLE,
  permission_request: WS_RELAY_TIERS.RELIABLE,
  auth_required: WS_RELAY_TIERS.RELIABLE,
  user_prompt: WS_RELAY_TIERS.RELIABLE,
  // CAP-2: switch markers are one-shot durable-backed events — reliable.
  agent_switch: WS_RELAY_TIERS.RELIABLE,
  // Durable host record that owns a seq — reliable so no seq goes missing.
  local_title_generated: WS_RELAY_TIERS.RELIABLE,
  agent_working: WS_RELAY_TIERS.RELIABLE,
  background_commands: WS_RELAY_TIERS.RELIABLE,
  // Frames about delivery itself: a gap must reach the client in order, and a
  // heartbeat is never queued, so neither may be a lossy event.
  gap: WS_RELAY_TIERS.RELIABLE,
  heartbeat: WS_RELAY_TIERS.RELIABLE
}

/**
 * Atomic stale-recovery payload emitted before post-watermark live events.
 * `logEpoch` + `headSeq` are read under the host's seq lock at registration:
 * the client's next cursor is `(logEpoch, watermark)`.
 */
export interface SessionSnapshotEvent {
  sessionId: string
  watermark: number
  /** The allocated seq frontier at registration (equals `watermark`). */
  headSeq: number
  /** Identity of the log incarnation the cursor belongs to. */
  logEpoch: string
  events: WsEvent[]
  /** The session's still-pending approvals (see `SessionSubscribeReply`). */
  pendingPermissions?: unknown[]
  /** The session's still-pending questions (see `SessionSubscribeReply`). */
  pendingQuestions?: unknown[]
  /** The session's still-pending elicitations, always present (see `SessionSubscribeReply`). */
  pendingElicitations?: unknown[]
}

/**
 * Look up the reliability tier for an event `type`. Mirrors the Rust
 * `tier_of(type_)` free function. Unknown types default to `reliable`
 * (the safe choice — never drop an event the relay does not recognize).
 */
export function wsTierOf(type: WsEventType): ReliabilityTier {
  return WS_EVENT_TIERS[type] ?? WS_RELAY_TIERS.RELIABLE
}

// ============================================================================
// OS vs human cap boundary (AC8) — TS-side mirror of the Rust registries
// ============================================================================

/**
 * ACP caps the SERVER fulfills locally (the browser cannot perform them).
 * The browser must never send these as WS requests; a request for one is
 * rejected with `err.code: "unsupported"`.
 *
 * `terminal/*` is a prefix — every cap under the `terminal/` namespace is
 * OS-fulfilled. The enforcement layer matches prefixes ending in `/*`.
 */
export const WS_OS_FULFILLED_CAPS = [
  'fs/read_text_file',
  'fs/write_text_file',
  'terminal/*'
] as const

/**
 * ACP caps RELAYED to the browser (human-in-the-loop). The server forwards
 * these to the connected browser client which renders the human UI.
 * `session_notification` fan-out + `request_permission`.
 */
export const WS_HUMAN_RELAYED_CAPS = ['session_notification', 'request_permission'] as const

/**
 * Whether `cap` matches an OS-fulfilled cap entry (exact match, or prefix
 * match for entries ending in `/*`). Mirrors the Rust enforcement check.
 */
export function isOsFulfilledCap(cap: string): boolean {
  return WS_OS_FULFILLED_CAPS.some(
    (entry) => entry === cap || (entry.endsWith('/*') && cap.startsWith(entry.slice(0, -1)))
  )
}

/**
 * Whether `cap` matches a human-relayed cap entry (exact match).
 */
export function isHumanRelayedCap(cap: string): boolean {
  return WS_HUMAN_RELAYED_CAPS.some((entry) => entry === cap)
}

// ============================================================================
// Frame envelopes (AC2 + AC3)
// ============================================================================

/**
 * A WS event envelope pushed server→client.
 *
 * Envelope fields are snake_case (`sid`, `seq`, `type`, `payload`); the
 * `payload` is the existing camelCase ACP event struct value. `sid` is `null` for agent-level events
 * (`agent_spawned`, `agent_disconnected`, `agent_error` without a session,
 * `auth_required`); `seq` is `0` for agent-level + relay-level events.
 */
export interface WsEvent {
  /** Session id, or `null` for agent-level / relay-level events. */
  sid: string | null
  /** Per-session monotonic sequence number (starts at 1). `0` for agent-level. */
  seq: number
  /** The event `type` (one of `WsEventType`). */
  type: WsEventType
  /** The camelCase ACP event struct value (passed through verbatim). */
  payload: unknown
}

/** `type` of the top-level frame that forwards an agent canvas call (no `seq`). */
export const WS_CANVAS_CALL_FRAME_TYPE = 'canvas_call'

/** `type` of the top-level frame that says a renderer lost its canvas mount. */
export const WS_CANVAS_LEASE_LOST_FRAME_TYPE = 'canvas_lease_lost'

/**
 * `type` of the top-level watch frame (no `seq`): the full state of a watch
 * (`reset: true`, revision `n = 1`) or one delta after it. It shares its name
 * with the `watch` request, which is told apart by the request's `id`. It has
 * no tier: a delta the host cannot queue leaves a revision gap, and the client
 * resubscribes for fresh state.
 */
export const WS_WATCH_FRAME_TYPE = 'watch'

/**
 * The watches a connection can hold. `projects`, `chat_history`, `agents`
 * and `chat_activity` have the empty scope; `fs` and `terminals` are scoped to a
 * project id, `store` to a store key.
 */
export const WS_WATCH_NAMES = WsWatchNameValues

/**
 * A WS reply envelope sent server→client in response to a `WsRequest`.
 *
 * Discriminated union on `ok`: success carries `payload`; failure carries
 * `err` with a stable machine `code` (one of `WsErrorCode`) + human message.
 * Mirrors the `IpcResult<T>` philosophy from `ipc.types.ts`.
 */
export type WsReply<T = unknown> =
  | { id: string; ok: true; payload?: T; err?: never; visible_at?: unknown }
  | {
      id: string
      ok: false
      payload?: never
      err: { code: WsErrorCode; message: string; details?: ProjectBusyDetails }
      visible_at?: unknown
    }

/**
 * The `err` object shape inside a failing `WsReply`.
 */
export interface WsError {
  code: WsErrorCode
  message: string
  /** Structured context for the code (additive): set on `project_busy`. */
  details?: ProjectBusyDetails
}
