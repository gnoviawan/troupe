/**
 * Troupe Comu policy and OS permissions facade.
 *
 * Exposes identical methods over Tauri IPC (`comu_*` commands) on desktop and
 * HTTP (`/host/comu/*`) on remote clients, with optimistic CAS retry on
 * `revision_conflict` and structured failure logging through `log-api`.
 */

import type {
  AppComuPolicy,
  AppComuPolicyOp,
  ComuOsPermissionKind,
  ComuOsPermissionsStatus,
  ComuPolicyChange,
  ComuPolicyReason,
  ProjectComuPolicy,
  ProjectComuPolicyOp
} from '@shared/types/comu.types'
import type { IpcResult } from '@shared/types/ipc.types'
import { getJson, postJsonOnce } from './ipc/http'
import { invokeIpc } from './ipc/tauri'
import { logFrontendError } from './log-api'
import { isTauriContext } from './tauri-runtime'

const COMU_BASE = '/host/comu'
const SOURCE = 'comu-policy-api'
const MAX_CONFLICT_RETRIES = 4

export interface ComuPolicyApi {
  getProject(projectId: string): Promise<IpcResult<ProjectComuPolicy>>
  getApp(): Promise<IpcResult<AppComuPolicy>>
  applyProject(
    projectId: string,
    expectedRevision: number,
    op: ProjectComuPolicyOp
  ): Promise<IpcResult<ProjectComuPolicy>>
  applyApp(expectedRevision: number, op: AppComuPolicyOp): Promise<IpcResult<AppComuPolicy>>
  getPermissions(): Promise<IpcResult<ComuOsPermissionsStatus>>
  requestPermission(kind: ComuOsPermissionKind): Promise<IpcResult<ComuOsPermissionsStatus>>
}

export const desktopComuPolicyApi: ComuPolicyApi = {
  getProject: (projectId) => invokeIpc('comu_policy_get_project', { projectId }),
  getApp: () => invokeIpc('comu_policy_get_app'),
  applyProject: (projectId, expectedRevision, op) =>
    invokeIpc('comu_policy_apply_project', { projectId, expectedRevision, op }),
  applyApp: (expectedRevision, op) => invokeIpc('comu_policy_apply_app', { expectedRevision, op }),
  getPermissions: () => invokeIpc('comu_permissions_status'),
  requestPermission: (kind) => invokeIpc('comu_request_permission', { kind })
}

export const webComuPolicyApi: ComuPolicyApi = {
  getProject: (projectId) =>
    getJson(`${COMU_BASE}/policy/project?projectId=${encodeURIComponent(projectId)}`),
  getApp: () => getJson(`${COMU_BASE}/policy/app`),
  applyProject: (projectId, expectedRevision, op) =>
    postJsonOnce(`${COMU_BASE}/policy/project`, { projectId, expectedRevision, op }),
  applyApp: (expectedRevision, op) =>
    postJsonOnce(`${COMU_BASE}/policy/app`, { expectedRevision, op }),
  getPermissions: () => getJson(`${COMU_BASE}/permissions`),
  requestPermission: (kind) => postJsonOnce(`${COMU_BASE}/permissions/request`, { kind })
}

function transport(): ComuPolicyApi {
  return isTauriContext() ? desktopComuPolicyApi : webComuPolicyApi
}

export const comuPolicyApi: ComuPolicyApi = {
  getProject: (projectId) => transport().getProject(projectId),
  getApp: () => transport().getApp(),
  applyProject: (projectId, expectedRevision, op) =>
    transport().applyProject(projectId, expectedRevision, op),
  applyApp: (expectedRevision, op) => transport().applyApp(expectedRevision, op),
  getPermissions: () => transport().getPermissions(),
  requestPermission: (kind) => transport().requestPermission(kind)
}

type Failure = Extract<IpcResult<unknown>, { success: false }>

const KNOWN_REASONS: ReadonlySet<string> = new Set<ComuPolicyReason>([
  'invalid_app_id',
  'limit_exceeded',
  'malformed_operation',
  'invalid_project_id',
  'app_scope_required',
  'project_scope_mismatch',
  'revision_conflict',
  'policy_read_only',
  'policy_store_unavailable',
  'storage_failed'
])

export interface ComuPolicyFailure {
  code: string
  reason: ComuPolicyReason | null
  message: string
}

export function comuPolicyFailure(failure: Failure): ComuPolicyFailure {
  const split = failure.error.indexOf(': ')
  const head = split > 0 ? failure.error.slice(0, split) : ''
  if (KNOWN_REASONS.has(head)) {
    return {
      code: failure.code,
      reason: head as ComuPolicyReason,
      message: failure.error.slice(split + 2)
    }
  }
  return { code: failure.code, reason: null, message: failure.error }
}

function logFailure(operation: string, failure: Failure): void {
  const { code, reason } = comuPolicyFailure(failure)
  void logFrontendError({
    level: 'warn',
    source: SOURCE,
    message: `${operation} failed: code=${code} reason=${reason ?? 'none'}`
  })
}

function isConflict(result: IpcResult<unknown>): result is Failure {
  return !result.success && comuPolicyFailure(result).reason === 'revision_conflict'
}

export async function editProjectComuPolicy(
  projectId: string,
  op: ProjectComuPolicyOp,
  known?: ProjectComuPolicy
): Promise<IpcResult<ProjectComuPolicy>> {
  let revision = known?.revision
  for (let attempt = 0; attempt <= MAX_CONFLICT_RETRIES; attempt += 1) {
    if (revision === undefined) {
      const fresh = await comuPolicyApi.getProject(projectId)
      if (!fresh.success) {
        logFailure('read project comu policy', fresh)
        return fresh
      }
      revision = fresh.data.revision
    }
    const result = await comuPolicyApi.applyProject(projectId, revision, op)
    if (!isConflict(result)) {
      if (!result.success) logFailure(`project comu policy ${op.op}`, result)
      return result
    }
    revision = undefined
  }
  return {
    success: false,
    error: 'revision_conflict: the policy kept changing; reload and retry',
    code: 'CONFLICT'
  }
}

export async function editAppComuPolicy(
  op: AppComuPolicyOp,
  known?: AppComuPolicy
): Promise<IpcResult<AppComuPolicy>> {
  let revision = known?.revision
  for (let attempt = 0; attempt <= MAX_CONFLICT_RETRIES; attempt += 1) {
    if (revision === undefined) {
      const fresh = await comuPolicyApi.getApp()
      if (!fresh.success) {
        logFailure('read app comu policy', fresh)
        return fresh
      }
      revision = fresh.data.revision
    }
    const result = await comuPolicyApi.applyApp(revision, op)
    if (!isConflict(result)) {
      if (!result.success) logFailure(`app comu policy ${op.op}`, result)
      return result
    }
    revision = undefined
  }
  return {
    success: false,
    error: 'revision_conflict: the policy kept changing; reload and retry',
    code: 'CONFLICT'
  }
}

type ChangeListener = (change: ComuPolicyChange) => void

const changeListeners = new Set<ChangeListener>()
const resyncListeners = new Set<() => void>()

export function onComuPolicyChanged(listener: ChangeListener): () => void {
  changeListeners.add(listener)
  return () => {
    changeListeners.delete(listener)
  }
}

export function onComuPolicyResync(listener: () => void): () => void {
  resyncListeners.add(listener)
  return () => {
    resyncListeners.delete(listener)
  }
}

export function requestComuPolicyResync(): void {
  for (const listener of resyncListeners) listener()
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null
}

export function publishComuPolicyChange(raw: unknown): void {
  if (!isRecord(raw) || typeof raw.revision !== 'number') return
  if (raw.scope === 'app') {
    const change: ComuPolicyChange = { scope: 'app', revision: raw.revision }
    for (const listener of changeListeners) listener(change)
    return
  }
  if (raw.scope === 'project' && typeof raw.projectId === 'string') {
    const change: ComuPolicyChange = {
      scope: 'project',
      projectId: raw.projectId,
      revision: raw.revision
    }
    for (const listener of changeListeners) listener(change)
  }
}
