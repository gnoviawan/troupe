import type {
  AppComuPolicy,
  AppComuPolicyOp,
  ComuOsPermissionKind,
  ComuOsPermissionsStatus,
  ComuPolicyChange,
  ProjectComuPolicy,
  ProjectComuPolicyOp
} from '@shared/types/comu.types'
import type { IpcResult } from '@shared/types/ipc.types'
import { useCallback, useEffect, useRef, useState } from 'react'
import {
  type ComuPolicyFailure,
  comuPolicyApi,
  comuPolicyFailure,
  editAppComuPolicy,
  editProjectComuPolicy,
  onComuPolicyChanged,
  onComuPolicyResync
} from '@/lib/comu-policy-api'

export type ComuPolicyResource<P> =
  | { status: 'loading' }
  | { status: 'ready'; policy: P }
  | { status: 'error'; failure: ComuPolicyFailure }

interface Resource<P, Op> {
  state: ComuPolicyResource<P>
  edit: (op: Op) => Promise<ComuPolicyFailure | null>
}

function useComuResource<P, Op>(
  scopeKey: string | null,
  load: () => Promise<IpcResult<P>>,
  apply: (op: Op, known: P | null) => Promise<IpcResult<P>>,
  isRelevant: (change: ComuPolicyChange) => boolean
): Resource<P, Op> {
  const [state, setState] = useState<ComuPolicyResource<P>>({ status: 'loading' })
  const generation = useRef(0)
  const latest = useRef({ load, apply, isRelevant })
  latest.current = { load, apply, isRelevant }
  const policyRef = useRef<P | null>(null)

  const reload = useCallback(async () => {
    const mine = ++generation.current
    const result = await latest.current.load()
    if (mine !== generation.current) return
    if (result.success) {
      policyRef.current = result.data
      setState({ status: 'ready', policy: result.data })
    } else {
      policyRef.current = null
      setState({ status: 'error', failure: comuPolicyFailure(result) })
    }
  }, [])

  useEffect(() => {
    if (scopeKey === null) return
    policyRef.current = null
    setState({ status: 'loading' })
    void reload()
    const stop = onComuPolicyChanged((change) => {
      if (latest.current.isRelevant(change)) void reload()
    })
    const stopResync = onComuPolicyResync(() => void reload())
    const onVisible = (): void => {
      if (document.visibilityState === 'visible') void reload()
    }
    document.addEventListener('visibilitychange', onVisible)
    return () => {
      generation.current += 1
      stop()
      stopResync()
      document.removeEventListener('visibilitychange', onVisible)
    }
  }, [scopeKey, reload])

  const edit = useCallback(async (op: Op): Promise<ComuPolicyFailure | null> => {
    const result = await latest.current.apply(op, policyRef.current)
    if (!result.success) return comuPolicyFailure(result)
    generation.current += 1
    policyRef.current = result.data
    setState({ status: 'ready', policy: result.data })
    return null
  }, [])

  return { state: scopeKey === null ? { status: 'loading' } : state, edit }
}

export function useProjectComuPolicy(
  projectId: string | null
): Resource<ProjectComuPolicy, ProjectComuPolicyOp> {
  return useComuResource<ProjectComuPolicy, ProjectComuPolicyOp>(
    projectId,
    () => comuPolicyApi.getProject(projectId ?? ''),
    (op, known) => editProjectComuPolicy(projectId ?? '', op, known ?? undefined),
    (change) => change.scope === 'project' && change.projectId === projectId
  )
}

export function useAppComuPolicy(): Resource<AppComuPolicy, AppComuPolicyOp> {
  return useComuResource<AppComuPolicy, AppComuPolicyOp>(
    'app',
    () => comuPolicyApi.getApp(),
    (op, known) => editAppComuPolicy(op, known ?? undefined),
    (change) => change.scope === 'app'
  )
}

export interface ComuPermissionsResource {
  status: ComuOsPermissionsStatus | null
  loading: boolean
  recheck: () => Promise<void>
  request: (kind: ComuOsPermissionKind) => Promise<void>
}

export function useComuPermissions(enabled: boolean): ComuPermissionsResource {
  const [status, setStatus] = useState<ComuOsPermissionsStatus | null>(null)
  const [loading, setLoading] = useState(false)

  const recheck = useCallback(async () => {
    setLoading(true)
    try {
      const result = await comuPolicyApi.getPermissions()
      if (result.success) setStatus(result.data)
    } finally {
      setLoading(false)
    }
  }, [])

  const request = useCallback(async (kind: ComuOsPermissionKind) => {
    setLoading(true)
    try {
      const result = await comuPolicyApi.requestPermission(kind)
      if (result.success) setStatus(result.data)
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    if (!enabled) return
    void recheck()
  }, [enabled, recheck])

  return { status, loading, recheck, request }
}
