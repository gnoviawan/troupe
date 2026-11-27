/**
 * Unit tests for Troupe Comu policy and consent/Stop facades across desktop
 * (`invoke`) and remote web (`fetch` / `acp-transport`).
 */

import type { AppComuPolicy, ProjectComuPolicy } from '@shared/types/comu.types'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const { mockFetch, mockIsTauriContext, mockInvoke, mockRespondPermission } = vi.hoisted(() => ({
  mockFetch: vi.fn(),
  mockIsTauriContext: vi.fn(),
  mockInvoke: vi.fn(),
  mockRespondPermission: vi.fn()
}))

vi.mock('../tauri-runtime', () => ({ isTauriContext: mockIsTauriContext }))
vi.mock('@tauri-apps/api/core', () => ({ invoke: mockInvoke }))
vi.mock('../log-api', () => ({ logFrontendError: vi.fn(() => Promise.resolve()) }))
vi.mock('../acp-transport', () => ({
  getAcpTransport: () => ({ respondPermission: mockRespondPermission })
}))

import { answerComuConsent, parseComuConsentTicket, stopComuAgent } from '../comu-consent-api'
import {
  comuPolicyApi,
  comuPolicyFailure,
  editAppComuPolicy,
  editProjectComuPolicy,
  onComuPolicyChanged,
  publishComuPolicyChange
} from '../comu-policy-api'

function jsonResponse(body: unknown): Response {
  return { ok: true, status: 200, statusText: 'OK', json: () => Promise.resolve(body) } as Response
}

function makeAppPolicy(overrides: Partial<AppComuPolicy> = {}): AppComuPolicy {
  return {
    revision: 0,
    readOnly: false,
    enabled: true,
    cursorEnabled: true,
    askBeforeDrivingApps: true,
    askBeforeForegroundDesktop: true,
    askBeforeClipboardRead: true,
    allowedApps: [],
    ...overrides
  }
}

function makeProjectPolicy(
  projectId: string,
  overrides: Partial<ProjectComuPolicy> = {}
): ProjectComuPolicy {
  return {
    projectId,
    revision: 0,
    readOnly: false,
    allowedApps: [],
    ...overrides
  }
}

describe('comuPolicyApi transport routing and CAS retries', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.stubGlobal('fetch', mockFetch)
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('desktop: routes all 6 methods through comu_* Tauri commands', async () => {
    mockIsTauriContext.mockReturnValue(true)
    mockInvoke.mockResolvedValue({ success: true, data: makeAppPolicy() })

    await comuPolicyApi.getProject('proj-1')
    await comuPolicyApi.getApp()
    await comuPolicyApi.applyProject('proj-1', 2, { op: 'add_app', app: 'Calculator' })
    await comuPolicyApi.applyApp(3, { op: 'set_cursor_enabled', enabled: false })
    await comuPolicyApi.getPermissions()
    await comuPolicyApi.requestPermission('accessibility')

    expect(mockInvoke.mock.calls).toEqual([
      ['comu_policy_get_project', { projectId: 'proj-1' }],
      ['comu_policy_get_app', undefined],
      [
        'comu_policy_apply_project',
        { projectId: 'proj-1', expectedRevision: 2, op: { op: 'add_app', app: 'Calculator' } }
      ],
      [
        'comu_policy_apply_app',
        { expectedRevision: 3, op: { op: 'set_cursor_enabled', enabled: false } }
      ],
      ['comu_permissions_status', undefined],
      ['comu_request_permission', { kind: 'accessibility' }]
    ])
    expect(mockFetch).not.toHaveBeenCalled()
  })

  it('web: routes all 6 methods through /host/comu/* HTTP endpoints', async () => {
    mockIsTauriContext.mockReturnValue(false)
    mockFetch.mockImplementation(async () => jsonResponse({ success: true, data: makeAppPolicy() }))

    await comuPolicyApi.getProject('proj 1')
    await comuPolicyApi.getApp()
    await comuPolicyApi.applyProject('proj-1', 1, { op: 'remove_app', app: 'Calculator' })
    await comuPolicyApi.applyApp(0, { op: 'set_enabled', enabled: false })
    await comuPolicyApi.getPermissions()
    await comuPolicyApi.requestPermission('screen_recording')

    const calls = mockFetch.mock.calls.map(([url, init]) => [
      String(url).replace(/^https?:\/\/[^/]+/, ''),
      (init as RequestInit)?.method ?? 'GET',
      (init as RequestInit)?.body
    ])
    expect(calls).toEqual([
      ['/host/comu/policy/project?projectId=proj%201', 'GET', undefined],
      ['/host/comu/policy/app', 'GET', undefined],
      [
        '/host/comu/policy/project',
        'POST',
        JSON.stringify({
          projectId: 'proj-1',
          expectedRevision: 1,
          op: { op: 'remove_app', app: 'Calculator' }
        })
      ],
      [
        '/host/comu/policy/app',
        'POST',
        JSON.stringify({ expectedRevision: 0, op: { op: 'set_enabled', enabled: false } })
      ],
      ['/host/comu/permissions', 'GET', undefined],
      ['/host/comu/permissions/request', 'POST', JSON.stringify({ kind: 'screen_recording' })]
    ])
    expect(mockInvoke).not.toHaveBeenCalled()
  })

  it('reloads and retries on revision_conflict for both app and project policies', async () => {
    mockIsTauriContext.mockReturnValue(true)
    let appConflictSent = false
    mockInvoke.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === 'comu_policy_apply_app' && !appConflictSent) {
        appConflictSent = true
        return {
          success: false,
          code: 'CONFLICT',
          error: 'revision_conflict: policy changed on another surface (current revision 4)'
        }
      }
      if (cmd === 'comu_policy_get_app') {
        return { success: true, data: makeAppPolicy({ revision: 4 }) }
      }
      if (cmd === 'comu_policy_apply_app') {
        expect(args?.expectedRevision).toBe(4)
        return { success: true, data: makeAppPolicy({ revision: 5, cursorEnabled: false }) }
      }
      if (cmd === 'comu_policy_get_project') {
        return { success: true, data: makeProjectPolicy('p1', { revision: 2 }) }
      }
      if (cmd === 'comu_policy_apply_project') {
        return {
          success: true,
          data: makeProjectPolicy('p1', { revision: 3, allowedApps: ['Calculator'] })
        }
      }
      throw new Error(`unexpected command ${cmd}`)
    })

    const appRes = await editAppComuPolicy(
      { op: 'set_cursor_enabled', enabled: false },
      makeAppPolicy({ revision: 1 })
    )
    expect(appRes.success && appRes.data.revision).toBe(5)

    const projRes = await editProjectComuPolicy('p1', { op: 'add_app', app: 'Calculator' })
    expect(projRes.success && projRes.data.allowedApps).toEqual(['Calculator'])
  })

  it('parses structured policy refusals and delivers comu_policy_changed updates', () => {
    expect(
      comuPolicyFailure({
        success: false,
        code: 'INVALID_PARAMS',
        error: 'invalid_app_id: Troupe itself cannot be added to Allowed Apps'
      })
    ).toEqual({
      code: 'INVALID_PARAMS',
      reason: 'invalid_app_id',
      message: 'Troupe itself cannot be added to Allowed Apps'
    })

    const seen: unknown[] = []
    const unsub = onComuPolicyChanged((change) => seen.push(change))
    publishComuPolicyChange({ scope: 'app', revision: 2 })
    publishComuPolicyChange({ scope: 'project', projectId: 'p1', revision: 4 })
    publishComuPolicyChange({ scope: 'project', revision: 4 })
    publishComuPolicyChange(null)
    unsub()
    publishComuPolicyChange({ scope: 'app', revision: 3 })

    expect(seen).toEqual([
      { scope: 'app', revision: 2 },
      { scope: 'project', projectId: 'p1', revision: 4 }
    ])
  })
})

describe('comu-consent-api ticket parsing, consent answers, and Stop', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.stubGlobal('fetch', mockFetch)
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('parses host-minted comu tickets and ignores agent or browser permission events', () => {
    expect(parseComuConsentTicket(null)).toBeNull()
    expect(
      parseComuConsentTicket({
        requestId: 'agent-req-1',
        sessionId: 's1',
        _meta: { termul: { origin_kind: 'comu', comu: { appName: 'Calc', appId: 'calc' } } }
      })
    ).toBeNull()
    expect(
      parseComuConsentTicket({
        requestId: 'host:1234',
        sessionId: 's1',
        _meta: { termul: { origin_kind: 'browser' } }
      })
    ).toBeNull()

    const parsed = parseComuConsentTicket({
      requestId: 'host:abcd-1234',
      sessionId: 'sess-9',
      options: [{ optionId: 'allow_session' }, { optionId: 'allow_project' }, { optionId: 'deny' }],
      _meta: {
        termul: {
          origin_kind: 'comu',
          comu: {
            appName: 'Simulator',
            appId: 'com.apple.iphonesimulator',
            action: 'drive_app',
            toolName: 'comu_click'
          }
        }
      }
    })
    expect(parsed).toEqual({
      requestId: 'host:abcd-1234',
      sessionId: 'sess-9',
      appName: 'Simulator',
      appId: 'com.apple.iphonesimulator',
      action: 'drive_app',
      toolName: 'comu_click',
      options: ['allow_session', 'allow_project', 'deny']
    })
  })

  it('answers consent and invokes Stop across desktop and remote transports', async () => {
    mockIsTauriContext.mockReturnValue(true)
    mockInvoke
      .mockResolvedValueOnce({ success: true, data: null })
      .mockResolvedValueOnce({ success: false, code: 'TICKET_TOO_EARLY', error: 'too early' })
      .mockResolvedValueOnce({ success: true, data: true })

    expect(await answerComuConsent({ requestId: 'host:1' }, 'allow_session', 'claude')).toBe('ok')
    expect(await answerComuConsent({ requestId: 'host:1' }, 'deny', 'claude')).toBe('too_early')
    expect(await stopComuAgent('s1')).toBe(true)
    expect(mockInvoke).toHaveBeenLastCalledWith('comu_stop', { sessionId: 's1' })

    mockIsTauriContext.mockReturnValue(false)
    mockRespondPermission.mockResolvedValueOnce(undefined)
    mockRespondPermission.mockRejectedValueOnce({ code: 'TICKET_ALREADY_RESOLVED' })
    mockFetch.mockResolvedValueOnce(jsonResponse({ success: true, data: true }))

    expect(await answerComuConsent({ requestId: 'host:2' }, 'allow_project', 'claude')).toBe('ok')
    expect(await answerComuConsent({ requestId: 'host:2' }, 'deny', 'claude')).toBe('stale')
    expect(await stopComuAgent('s2')).toBe(true)
    expect(mockFetch).toHaveBeenCalledTimes(1)
  })
})
