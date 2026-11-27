/**
 * Runtime-neutral contracts for Troupe Comu (local Computer Use).
 *
 * Shared across the Tauri desktop renderer and remote browser/phone clients.
 */

export interface AppComuPolicy {
  revision: number
  readOnly: boolean
  enabled: boolean
  cursorEnabled: boolean
  askBeforeDrivingApps: boolean
  askBeforeForegroundDesktop: boolean
  askBeforeClipboardRead: boolean
  allowedApps: string[]
}

export interface ProjectComuPolicy {
  projectId: string
  revision: number
  readOnly: boolean
  allowedApps: string[]
}

export type AppComuPolicyOp =
  | { op: 'set_enabled'; enabled: boolean }
  | { op: 'set_cursor_enabled'; enabled: boolean }
  | { op: 'set_ask_before_driving_apps'; enabled: boolean }
  | { op: 'set_ask_before_foreground_desktop'; enabled: boolean }
  | { op: 'set_ask_before_clipboard_read'; enabled: boolean }
  | { op: 'add_app'; app: string }
  | { op: 'remove_app'; app: string }

export type ProjectComuPolicyOp = { op: 'add_app'; app: string } | { op: 'remove_app'; app: string }

export type ComuPolicyReason =
  | 'invalid_app_id'
  | 'limit_exceeded'
  | 'malformed_operation'
  | 'invalid_project_id'
  | 'app_scope_required'
  | 'project_scope_mismatch'
  | 'revision_conflict'
  | 'policy_read_only'
  | 'policy_store_unavailable'
  | 'storage_failed'

export type ComuPolicyChange =
  | { scope: 'app'; revision: number }
  | { scope: 'project'; projectId: string; revision: number }

export type ComuOsPermissionKind = 'accessibility' | 'screen_recording'

export interface ComuOsPermissionsStatus {
  platform: string
  accessibilityGranted: boolean
  screenRecordingGranted: boolean
  displayAvailable: boolean
  ready: boolean
  summary: string
}

export type ComuConsentAction = 'drive_app' | 'foreground_desktop' | 'clipboard_read'

export type ComuConsentOptionId = 'allow_session' | 'allow_project' | 'deny'

export interface ComuConsentTicket {
  requestId: string
  sessionId: string
  appName: string
  appId: string
  action: ComuConsentAction
  toolName: string
  options: ComuConsentOptionId[]
}
