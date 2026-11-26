import type {
  WebTerminalDataFrame,
  WebTerminalEventFrame,
  WebTerminalEventPayload,
  WebTerminalGapFrame,
  WebTerminalReplayFrame,
  WebTerminalRequestType
} from './generated/protocol'
import type {
  RotatedClaim,
  SpawnedTerminal,
  TerminalAttachResult,
  TerminalSpawnOptions
} from './ipc.types'

export type {
  WebTerminalDataFrame,
  WebTerminalEventFrame,
  WebTerminalEventPayload,
  WebTerminalGapFrame,
  WebTerminalReplayFrame,
  WebTerminalRequestType
} from './generated/protocol'

/** The `session_lifecycle` event payload. */
export type SessionLifecyclePayload = Extract<
  WebTerminalEventPayload,
  { type: 'session_lifecycle' }
>

/**
 * Machine codes the `/terminal/ws` handshake adds to the reply `code` string
 * (SCREAMING_SNAKE, unlike `/ws`'s snake_case `WS_ERROR_CODES`). Mirror the
 * Rust `terminal_ws` handshake refusals (AD-15, AD-4).
 */
export const WEB_TERMINAL_ERROR_CODES = {
  // Bad token on a gated host (unchanged generic refusal).
  UNAUTHORIZED: 'UNAUTHORIZED',
  // `authenticate` carried a missing or different `protocolVersion`.
  PROTOCOL_VERSION_MISMATCH: 'PROTOCOL_VERSION_MISMATCH',
  // `authenticate` carried a missing, empty or oversized `deviceId` /
  // `clientInstanceId`.
  INVALID_CLIENT_IDENTITY: 'INVALID_CLIENT_IDENTITY',
  // The connection is view-only (shared-live deployment, or a non-loopback
  // peer without `--allow-remote-writes`) and the request mutates the host.
  // Per-request refusal: the connection stays open and keeps its reads.
  FORBIDDEN: 'FORBIDDEN',
  // A ledgered command (`spawn`, `write`, `kill`, `rotate_claim`,
  // `revoke_claim`) was resent but its outcome is no longer known (evicted
  // receipt, or a rotated claim whose credential was never kept). The host did
  // NOT run it again; recover via `list_preserved` / the terminal's state.
  OUTCOME_UNKNOWN: 'OUTCOME_UNKNOWN',
  // The `commandId` was already used for a different request.
  COMMAND_CONFLICT: 'COMMAND_CONFLICT',
  // The `commandId` is not 1-128 characters of `[A-Za-z0-9_-]`.
  INVALID_COMMAND_ID: 'INVALID_COMMAND_ID',
  // A `spawn` was refused by a host terminal cap (per project or global).
  // Nothing was created and no slot is held; the reply `error` names both
  // limits. The same code is the desktop `terminal_spawn` `IpcResult.code`.
  TERMINAL_LIMIT: 'TERMINAL_LIMIT',
  // A `spawn` named a project that is being removed or archived (its
  // sessions and terminals are being stopped). Retry never helps: the project
  // is going away.
  PROJECT_BUSY: 'PROJECT_BUSY'
} as const

export interface WebTerminalRequest {
  id: string
  type: WebTerminalRequestType
  payload: Record<string, unknown> | TerminalSpawnOptions
  /**
   * Client-minted command id (AD-5) of a ledgered request: `spawn`, `write`
   * (one per input chunk), `kill`, `rotate_claim`, `revoke_claim`. A resend with
   * the same id never runs twice. `spawn` may instead carry it as
   * `payload.commandId`. Required on those types: a request without it is
   * refused with `INVALID_COMMAND_ID` before it runs.
   * `list_preserved` re-issues claims and is never replayed.
   */
  commandId?: string
}

export type WebTerminalReply<T = unknown> =
  | { id: string; success: true; data: T }
  | { id: string; success: false; error: string; code: string }

export type WebTerminalFrame<T = unknown> =
  | WebTerminalReply<T>
  | WebTerminalDataFrame
  | WebTerminalReplayFrame
  | WebTerminalGapFrame
  | WebTerminalEventFrame

/**
 * CAP-3: the spawn reply carries the issued claim credential (flattened
 * camelCase, same shape as the desktop `terminal_spawn` IpcResult data).
 */
export type WebTerminalSpawnReply = WebTerminalReply<SpawnedTerminal>

/** CAP-3: attach reply — shared TerminalAttachResult shape (never a claim). */
export type WebTerminalAttachReply = WebTerminalReply<TerminalAttachResult>

/** CAP-3: rotate reply — the fresh credential. */
export type WebTerminalRotateClaimReply = WebTerminalReply<RotatedClaim>
